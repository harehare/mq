//! Inline parsing of paragraph, heading and table cell content.
//!
//! The content is scanned once, left to right, into a flat list of items: literal text, finished
//! nodes (code spans, autolinks, breaks), delimiter runs and bracket openers. Links close over the
//! items after their opener, and emphasis is resolved with the delimiter stack algorithm of the
//! CommonMark specification, so nesting never recurses over the source.

mod emphasis;
mod entities;
mod entity;
mod html;
mod link;
mod literal;
mod punctuation;
mod tail;

pub(crate) use entity::unescape;
pub(super) use entity::{decode_references, remove_line_indent};
pub(crate) use html::is_autolink_email;
pub(crate) use link::normalize;
pub(super) use tail::{destination, title_at};

use super::mdx::{self, Fallback, Parsed, TagKind};
use super::resolve::References;
use super::scan::eol_len;
use super::tree::InlineSource;
use crate::node::{
    Break, CodeInline, MathInline, MdxAttributeContent, MdxJsxTextElement, MdxTextExpression, Node, Position, Text,
};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::borrow::Cow;
use std::cell::{OnceCell, RefCell};

/// The value of a text item: a slice of the source, or a decoded string.
pub(super) enum Value {
    Slice(usize, usize),
    Owned(String),
}

/// A run of `*`, `_` or `~` that may open or close emphasis.
pub(super) struct Delim {
    pub(super) ch: u8,
    /// Offset of the first remaining character.
    pub(super) start: usize,
    pub(super) count: usize,
    /// Length of the whole run, which `count` shrinks from as the run is used.
    pub(super) original: usize,
    pub(super) can_open: bool,
    pub(super) can_close: bool,
}

/// A `[` or `![` that may open a link or an image.
pub(super) struct Opener {
    pub(super) image: bool,
    /// Offset of the `[`, or of the `!` of an image.
    pub(super) start: usize,
}

pub(super) enum Item {
    Text {
        start: usize,
        end: usize,
        value: Value,
    },
    /// A finished node and how deeply it nests: a leaf is 1.
    Node(Node, usize),
    Delim(Delim),
    Open(Opener),
    /// An email address in plain text. It is not linked inside link text.
    Email {
        start: usize,
        end: usize,
        prefixed: bool,
    },
    /// A JSX tag in MDX. Tags are paired when the items become nodes.
    Jsx(TextTag),
}

/// A JSX tag in text, with its offsets in the source.
pub(super) struct TextTag {
    name: Option<String>,
    attributes: Vec<MdxAttributeContent>,
    kind: TagKind,
    start: usize,
    end: usize,
}

/// Emphasis and links nested deeper than this are text, and JSX elements are an error. This bounds the
/// recursion of everything that walks the nodes.
pub(super) const MAX_NESTING: usize = 128;

/// How deeply the item nests: nodes carry it, and the rest are text.
pub(super) fn item_depth(item: &Item) -> usize {
    match item {
        Item::Node(_, depth) => *depth,
        _ => 0,
    }
}

/// What the scanner and its helpers share.
#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub(super) source: &'a InlineSource,
    pub(super) references: &'a References,
    /// Whether email addresses in plain text are linked: not inside link text, and not when there is no `@`
    /// in the text, which is looked for once.
    pub(super) emails: bool,
    /// The first error in the content, which only MDX has.
    error: &'a RefCell<Option<String>>,
}

impl Context<'_> {
    /// Records an error, keeping the first one.
    fn fail(&self, message: String) {
        self.error.borrow_mut().get_or_insert(message);
    }

    pub(super) fn src(&self) -> &str {
        &self.source.text
    }

    pub(super) fn position(&self, start: usize, end: usize) -> Position {
        Position {
            start: self.source.point(start),
            end: self.source.end_point(end),
        }
    }
}

/// Parses raw inline content into nodes.
pub(super) fn parse(source: &InlineSource, references: &References) -> Result<Vec<Node>, String> {
    if source.text.is_empty() {
        return Ok(Vec::new());
    }

    let error = RefCell::new(None);
    let context = Context {
        source,
        references,
        emails: !references.mdx && source.text.contains('@'),
        error: &error,
    };
    let mut scanner = Scanner {
        context: &context,
        items: Vec::new(),
        openers: Vec::new(),
        depths: Vec::new(),
        inactive_below: 0,
        pos: 0,
        run: 0,
        misses: html::Misses::default(),
        runs: SpanRuns::default(),
        braces: mdx::Braces::default(),
    };
    scanner.scan();
    scanner.flush();

    let Scanner { mut items, .. } = scanner;
    emphasis::process(&mut items, &context);
    let nodes = to_nodes(items, &context);

    match error.into_inner() {
        Some(message) => Err(message),
        None => Ok(nodes),
    }
}

/// Converts items to nodes, merging adjacent text. Unused delimiters and openers become text.
pub(super) fn to_nodes(items: Vec<Item>, context: &Context<'_>) -> Vec<Node> {
    let src = context.src();
    let items = if context.emails {
        literal::link_emails(src, items)
    } else {
        items
    };
    let mut nodes = Vec::new();
    // Elements that are open, with the nodes that came before them.
    let mut open: Vec<(TextTag, Vec<Node>)> = Vec::new();
    let mut current: Option<(usize, usize, String)> = None;

    let finish = |current: &mut Option<(usize, usize, String)>, nodes: &mut Vec<Node>| {
        if let Some((start, end, value)) = current.take() {
            nodes.push(Node::Text(Text {
                value,
                position: Some(context.position(start, end)),
            }));
        }
    };

    for item in items {
        let (start, end, text): (usize, usize, Cow<'_, str>) = match item {
            Item::Text { start, end, value } => match value {
                // The backslash of an escape is not part of the position of the text it starts.
                Value::Slice(from, to) => (from, end, Cow::Borrowed(&src[from..to])),
                Value::Owned(value) => (start, end, Cow::Owned(value)),
            },
            Item::Delim(delim) => {
                let end = delim.start + delim.count;
                (delim.start, end, Cow::Borrowed(&src[delim.start..end]))
            }
            Item::Open(opener) => {
                let end = opener.start + if opener.image { 2 } else { 1 };
                (opener.start, end, Cow::Borrowed(&src[opener.start..end]))
            }
            Item::Node(node, _) => {
                finish(&mut current, &mut nodes);
                nodes.push(node);
                continue;
            }
            Item::Jsx(tag) => {
                finish(&mut current, &mut nodes);
                match tag.kind {
                    TagKind::Open if open.len() >= MAX_NESTING => {
                        context.fail(format!("Elements are nested deeper than {MAX_NESTING} levels"));
                    }
                    TagKind::Open => open.push((tag, std::mem::take(&mut nodes))),
                    TagKind::SelfClosing => {
                        let end = tag.end;
                        nodes.push(text_element(context, tag, Vec::new(), end));
                    }
                    TagKind::Close => match open.pop() {
                        None => context.fail("Unexpected closing slash `/` in tag, expected an open tag first".into()),
                        Some((opening, before)) => {
                            if opening.name != tag.name {
                                context.fail(format!(
                                    "Unexpected closing tag `</{}>`, expected corresponding closing tag for `<{}>`",
                                    tag.name.as_deref().unwrap_or_default(),
                                    opening.name.as_deref().unwrap_or_default()
                                ));
                            }
                            let children = std::mem::replace(&mut nodes, before);
                            nodes.push(text_element(context, opening, children, tag.end));
                        }
                    },
                }
                continue;
            }
            Item::Email { start, end, prefixed } => {
                finish(&mut current, &mut nodes);
                let text = &src[start..end];
                let position = Some(context.position(start, end));
                nodes.push(Node::Link(crate::node::Link {
                    url: crate::node::Url(if prefixed {
                        format!("mailto:{text}")
                    } else {
                        text.to_string()
                    }),
                    title: None,
                    values: vec![Node::Text(Text {
                        value: text.to_string(),
                        position: position.clone(),
                    })],
                    position,
                }));
                continue;
            }
        };
        match &mut current {
            Some((_, current_end, value)) => {
                value.push_str(&text);
                *current_end = end;
            }
            None => current = Some((start, end, text.into_owned())),
        }
    }
    finish(&mut current, &mut nodes);

    // Elements that never close are an error, and are closed where the content ends.
    while let Some((opening, before)) = open.pop() {
        context.fail(format!(
            "Expected a closing tag for `<{}>` before the end of the content",
            opening.name.as_deref().unwrap_or_default()
        ));
        let end = opening.end;
        let children = std::mem::replace(&mut nodes, before);
        nodes.push(text_element(context, opening, children, end));
    }

    nodes
}

fn text_element(context: &Context<'_>, tag: TextTag, children: Vec<Node>, end: usize) -> Node {
    Node::MdxJsxTextElement(MdxJsxTextElement {
        children,
        position: Some(context.position(tag.start, end)),
        name: tag.name.map(SmolStr::new),
        attributes: tag.attributes,
    })
}

/// Where the runs of backticks and of dollar signs start, by length, found in one pass the first time a
/// span asks, so that openers without a closer do not each scan to the end.
#[derive(Default)]
struct SpanRuns {
    backticks: OnceCell<FxHashMap<usize, Vec<usize>>>,
    dollars: OnceCell<FxHashMap<usize, Vec<usize>>>,
}

impl SpanRuns {
    /// The start of the first run of exactly `size` bytes `ch` that starts at or after `from`.
    fn next_of_length(&self, src: &str, ch: u8, size: usize, from: usize) -> Option<usize> {
        let runs = if ch == b'`' { &self.backticks } else { &self.dollars };
        let starts = runs
            .get_or_init(|| {
                let bytes = src.as_bytes();
                let mut runs: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] == ch {
                        let length = bytes[index..].iter().take_while(|&&b| b == ch).count();
                        runs.entry(length).or_default().push(index);
                        index += length;
                    } else {
                        index += 1;
                    }
                }
                runs
            })
            .get(&size)?;
        starts.get(starts.partition_point(|&start| start < from)).copied()
    }
}

struct Scanner<'a> {
    context: &'a Context<'a>,
    items: Vec<Item>,
    /// Indexes of the `Item::Open` items that are still waiting for a `]`.
    openers: Vec<usize>,
    /// For each opener, the deepest node pushed inside it so far.
    depths: Vec<usize>,
    /// Links cannot contain links, so the `[` openers below this many are deactivated.
    inactive_below: usize,
    pos: usize,
    /// Start of the plain text that has not been pushed as an item yet.
    run: usize,
    /// Raw HTML terminators already known to be missing.
    misses: html::Misses,
    runs: SpanRuns,
    /// Where the braces of the content close, for MDX.
    braces: mdx::Braces,
}

/// Bytes where `Scanner::scan` may start a construct. Every other byte is plain text.
static SPECIAL: [bool; 256] = {
    let mut table = [false; 256];
    let special = b"\\&`$*_~[!]<{\n\rhHwW";
    let mut index = 0;
    while index < special.len() {
        table[special[index] as usize] = true;
        index += 1;
    }
    table
};

impl Scanner<'_> {
    fn src(&self) -> &str {
        self.context.src()
    }

    fn scan(&mut self) {
        let len = self.src().len();
        while self.pos < len {
            let bytes = self.src().as_bytes();
            let skip = bytes[self.pos..].iter().take_while(|&&b| !SPECIAL[b as usize]).count();
            let pos = self.pos + skip;
            if pos == len {
                self.pos = len;
                break;
            }
            let (byte, image) = (bytes[pos], bytes.get(pos + 1) == Some(&b'['));
            self.pos = pos;
            let link = if matches!(byte, b'[' | b'!') {
                self.obsidian_link()
            } else {
                None
            };
            match byte {
                _ if link.is_some() => {
                    if let Some((node, end)) = link {
                        self.push_node(node, end);
                    }
                }
                b'\\' => self.escape(),
                b'&' => self.entity(),
                b'`' => self.span(b'`'),
                b'$' if !self.context.references.mdx => self.span(b'$'),
                b'*' | b'_' => self.delimiter(),
                b'~' if !self.context.references.mdx => self.delimiter(),
                b'[' => self.open(false),
                b'!' if image => self.open(true),
                b']' => self.close(),
                b'<' if !self.context.references.mdx => self.angle(),
                b'<' => self.jsx(),
                b'{' if self.context.references.mdx => self.expression(),
                b'\n' | b'\r' => self.line_ending(),
                b'h' | b'H' | b'w' | b'W' if !self.context.references.mdx => self.literal_url(),
                _ => self.pos += 1,
            }
        }
    }

    /// The wikilink `[[target|text]]` or the embed `![[target|display]]` at the current position, and the
    /// offset where it ends. The target is not empty, and neither part holds a bracket or a line ending.
    #[cfg(any(feature = "wikilink", feature = "embed"))]
    fn obsidian_link(&self) -> Option<(Node, usize)> {
        if self.context.references.mdx {
            return None;
        }
        let src = self.src();
        let start = self.pos;
        let rest = &src[start..];
        let (embed, open) = match rest.as_bytes() {
            [b'!', b'[', b'[', ..] if cfg!(feature = "embed") => (true, start + 3),
            [b'[', b'[', ..] if cfg!(feature = "wikilink") => (false, start + 2),
            _ => return None,
        };
        // The content holds no bracket or line ending, so the first one found must start the `]]`.
        let close = open + src[open..].find(['[', ']', '\n', '\r'])?;
        if !src[close..].starts_with("]]") {
            return None;
        }
        let content = &src[open..close];
        let (target, label) = match content.split_once('|') {
            Some((target, label)) => (target.trim(), Some(label.trim().to_string())),
            None => (content.trim(), None),
        };
        if target.is_empty() {
            return None;
        }
        let end = close + 2;
        let position = Some(self.context.position(start, end));
        let target = target.to_string();
        let node = if embed {
            #[cfg(feature = "embed")]
            {
                Node::Embed(crate::node::Embed {
                    target,
                    display: label,
                    position,
                })
            }
            #[cfg(not(feature = "embed"))]
            unreachable!("embeds are off")
        } else {
            #[cfg(feature = "wikilink")]
            {
                Node::WikiLink(crate::node::WikiLink {
                    target,
                    text: label,
                    position,
                })
            }
            #[cfg(not(feature = "wikilink"))]
            unreachable!("wikilinks are off")
        };
        Some((node, end))
    }

    #[cfg(not(any(feature = "wikilink", feature = "embed")))]
    fn obsidian_link(&self) -> Option<(Node, usize)> {
        None
    }

    /// Pushes the pending plain text up to the current position as an item.
    fn flush(&mut self) {
        if self.run < self.pos {
            self.items.push(Item::Text {
                start: self.run,
                end: self.pos,
                value: Value::Slice(self.run, self.pos),
            });
        }
        self.run = self.pos;
    }

    /// Pushes `item` after the pending text and continues scanning at `end`.
    fn push(&mut self, item: Item, end: usize) {
        self.flush();
        if let Item::Node(_, depth) = &item {
            self.note_depth(*depth);
        }
        self.items.push(item);
        self.pos = end;
        self.run = end;
    }

    /// Records that a node of `depth` is inside the innermost open bracket.
    fn note_depth(&mut self, depth: usize) {
        if let Some(innermost) = self.depths.last_mut() {
            *innermost = (*innermost).max(depth);
        }
    }

    fn push_node(&mut self, node: Node, end: usize) {
        self.push(Item::Node(node, 1), end);
    }

    fn escape(&mut self) {
        let src = self.src();
        let bytes = src.as_bytes();
        let pos = self.pos;
        match bytes.get(pos + 1) {
            Some(next) if next.is_ascii_punctuation() => {
                let item = Item::Text {
                    start: pos,
                    end: pos + 2,
                    value: Value::Slice(pos + 1, pos + 2),
                };
                self.push(item, pos + 2);
            }
            Some(b'\n' | b'\r') => {
                let end = pos + 1 + eol_len(bytes, pos + 1);
                let node = Node::Break(Break {
                    position: Some(self.context.position(pos, end)),
                });
                self.push_node(node, end);
                // The indentation of the next line is not part of the text.
                while matches!(self.src().as_bytes().get(self.pos), Some(b' ' | b'\t')) {
                    self.pos += 1;
                }
                self.run = self.pos;
            }
            _ => self.pos += 1,
        }
    }

    fn entity(&mut self) {
        match entity::decode(self.src(), self.pos) {
            Some((end, value)) => {
                let item = Item::Text {
                    start: self.pos,
                    end,
                    value: Value::Owned(value),
                };
                self.push(item, end);
            }
            None => self.pos += 1,
        }
    }

    fn line_ending(&mut self) {
        let src = self.src();
        let bytes = src.as_bytes();
        let pos = self.pos;
        let eol = eol_len(bytes, pos);

        // Trailing whitespace before the line ending is dropped, or makes a hard break.
        let mut start = pos;
        while start > self.run && matches!(bytes[start - 1], b' ' | b'\t') {
            start -= 1;
        }
        let hard = pos - start >= 2 && bytes[start..pos].iter().all(|&b| b == b' ');
        // The indentation of the next line is not part of the text.
        let mut after = pos + eol;
        while matches!(bytes.get(after), Some(b' ' | b'\t')) {
            after += 1;
        }

        self.pos = start;
        self.flush();
        if hard {
            let node = Node::Break(Break {
                position: Some(self.context.position(start, pos + eol)),
            });
            self.note_depth(1);
            self.items.push(Item::Node(node, 1));
        } else {
            self.items.push(Item::Text {
                start: pos,
                end: pos + eol,
                value: Value::Slice(pos, pos + eol),
            });
        }
        self.pos = after;
        self.run = after;
    }

    /// A code span (`` ` ``) or an inline math span (`$`).
    fn span(&mut self, ch: u8) {
        let src = self.context.src();
        let bytes = src.as_bytes();
        let start = self.pos;
        let size = bytes[start..].iter().take_while(|&&b| b == ch).count();

        let close = self.runs.next_of_length(src, ch, size, start + size);

        let Some(close) = close else {
            self.pos = start + size;
            return;
        };

        let end = close + size;
        // In a table cell, `\|` stands for `|` inside code as well.
        let content = &src[start + size..close];
        // The lines of a paragraph lose their leading whitespace before the inline content is read.
        let content = if self.context.references.mdx {
            Cow::Borrowed(content)
        } else {
            remove_line_indent(content)
        };
        let content = content.as_ref();
        let value = if self.context.source.table && content.contains("\\|") {
            span_value(&content.replace("\\|", "|"))
        } else {
            span_value(content)
        };
        let position = Some(self.context.position(start, end));
        let node = if ch == b'`' {
            Node::CodeInline(CodeInline { value, position })
        } else {
            Node::MathInline(MathInline { value, position })
        };
        self.push_node(node, end);
    }

    fn delimiter(&mut self) {
        let src = self.src();
        let bytes = src.as_bytes();
        let start = self.pos;
        let ch = bytes[start];
        let size = bytes[start..].iter().take_while(|&&b| b == ch).count();
        let end = start + size;

        // Strikethrough runs are one or two tildes.
        if ch == b'~' && size > 2 {
            self.pos = end;
            return;
        }

        let before = src[..start].chars().next_back();
        let after = src[end..].chars().next();
        let (can_open, can_close) = emphasis::flanking(ch, before, after, !self.context.references.mdx);
        if !can_open && !can_close {
            self.pos = end;
            return;
        }

        let delim = Delim {
            ch,
            start,
            count: size,
            original: size,
            can_open,
            can_close,
        };
        self.push(Item::Delim(delim), end);
    }

    fn open(&mut self, image: bool) {
        let opener = Opener { image, start: self.pos };
        let end = self.pos + if image { 2 } else { 1 };
        self.flush();
        self.openers.push(self.items.len());
        self.depths.push(0);
        self.items.push(Item::Open(opener));
        self.pos = end;
        self.run = end;
    }

    fn close(&mut self) {
        self.flush();
        if !link::close(self) {
            // Not a link: the bracket stays plain text.
            self.pos += 1;
        }
    }

    /// `<` starts an autolink or raw inline HTML.
    fn angle(&mut self) {
        let context = self.context;
        let src = context.src();
        let pos = self.pos;
        if let Some(autolink) = html::autolink(src, pos) {
            let position = |start, end| Some(self.context.position(start, end));
            let node = Node::Link(crate::node::Link {
                url: crate::node::Url(autolink.url),
                title: None,
                values: vec![Node::Text(Text {
                    value: src[autolink.text.0..autolink.text.1].to_string(),
                    position: position(autolink.text.0, autolink.text.1),
                })],
                position: position(pos, autolink.end),
            });
            self.push_node(node, autolink.end);
        } else if let Some(end) = html::inline_html(src, pos, &mut self.misses) {
            // Lines of a paragraph lose their leading whitespace, also inside a tag.
            let value = src[pos..end]
                .split_inclusive('\n')
                .enumerate()
                .map(|(index, line)| {
                    if index == 0 {
                        line
                    } else {
                        line.trim_start_matches([' ', '\t'])
                    }
                })
                .collect::<String>();
            let node = Node::Html(crate::node::Html {
                value,
                position: Some(self.context.position(pos, end)),
            });
            self.push_node(node, end);
        } else {
            self.pos += 1;
        }
    }

    /// A JSX tag in MDX text.
    fn jsx(&mut self) {
        match mdx::tag(self.src(), self.pos, Some(&self.braces)) {
            Parsed::Ok(tag) => {
                let end = tag.end;
                let item = Item::Jsx(TextTag {
                    name: tag.name,
                    attributes: tag.attributes,
                    kind: tag.kind,
                    start: self.pos,
                    end,
                });
                self.push(item, end);
            }
            Parsed::Error(message) => {
                self.context.fail(message);
                self.pos += 1;
            }
            Parsed::More(Fallback::Error(message)) => {
                self.context.fail(message);
                self.pos += 1;
            }
            Parsed::Nok | Parsed::More(Fallback::Nok) => self.pos += 1,
        }
    }

    /// An expression in MDX text.
    fn expression(&mut self) {
        match mdx::expression(self.src(), self.pos, Some(&self.braces)) {
            Parsed::Ok((end, value)) => {
                let node = Node::MdxTextExpression(MdxTextExpression {
                    value,
                    position: Some(self.context.position(self.pos, end)),
                });
                self.push_node(node, end);
            }
            Parsed::Error(message) => {
                self.context.fail(message);
                self.pos += 1;
            }
            Parsed::More(Fallback::Error(message)) => {
                self.context.fail(message);
                self.pos += 1;
            }
            Parsed::Nok | Parsed::More(Fallback::Nok) => self.pos += 1,
        }
    }

    fn literal_url(&mut self) {
        literal::url(self);
    }
}

/// The value of a code or math span: the content as it is, except that one space or line ending is
/// removed from both ends when both have one and the content is not only spaces and line endings.
fn span_value(content: &str) -> SmolStr {
    let is_space = |b: &u8| matches!(b, b' ' | b'\n');
    let bytes = content.as_bytes();
    if bytes.len() > 2 && is_space(&bytes[0]) && is_space(&bytes[bytes.len() - 1]) && !bytes.iter().all(is_space) {
        SmolStr::new(&content[1..content.len() - 1])
    } else {
        SmolStr::new(content)
    }
}
