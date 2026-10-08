//! Renders the block tree as HTML, with CommonMark, GFM, math and frontmatter, and with raw HTML
//! allowed.
//!
//! The output is the same as that of `markdown-rs`, line endings included: blocks start on a line of
//! their own, and the line ending that the source has after the last block is kept.

use super::inline;
use super::resolve::{self, References};
use super::tree::{Block, FencedBlock, InlineBlock, InlineKind, InlineSource, ListBlock, TableItem};
use crate::node::{Code, Node, TableAlignKind};
use rustc_hash::FxHashMap;

/// Protocols that a link may have, others are dropped.
const SAFE_PROTOCOL_HREF: [&str; 6] = ["http", "https", "irc", "ircs", "mailto", "xmpp"];
/// Protocols that an image source may have.
const SAFE_PROTOCOL_SRC: [&str; 2] = ["http", "https"];

/// Renders `content` as HTML.
pub(super) fn render(content: &str) -> String {
    let blocks = super::block::parse(content, false);
    let mut references = References::default();
    resolve::collect(&blocks, &mut references);

    let mut html = Html {
        out: String::new(),
        eol: first_line_ending(content),
        references,
        targets: FxHashMap::default(),
        tight: Vec::new(),
        calls: Vec::new(),
        notes: Vec::new(),
        check: None,
        in_link: false,
    };
    html.collect_targets(&blocks);
    html.blocks(&blocks);

    if !html.calls.is_empty() {
        html.footnote_section();
    } else if has_line_ending_after(content, last_rendered(&blocks)) && !list_then_hidden(content, &blocks) {
        html.line_ending_if_needed();
    }
    html.out
}

/// The line ending of the first line that has one, as the document uses it throughout.
fn first_line_ending(content: &str) -> &'static str {
    match content.find(['\n', '\r']) {
        Some(index) if content[index..].starts_with("\r\n") => "\r\n",
        Some(index) if content[index..].starts_with('\r') => "\r",
        _ => "\n",
    }
}

/// Whether the source has a line ending after the line on which `end` ends. A document that has no
/// block to render has none.
fn has_line_ending_after(content: &str, end: Option<usize>) -> bool {
    let Some(end) = end else { return false };
    let mut lines = 1;
    let mut bytes = content.bytes().peekable();
    while let Some(byte) = bytes.next() {
        match byte {
            b'\n' => lines += 1,
            b'\r' => {
                if bytes.peek() == Some(&b'\n') {
                    bytes.next();
                }
                lines += 1;
            }
            _ => {}
        }
    }
    end < lines
}

/// Whether a list is followed by nothing but blocks without output, such as definitions, and no blank
/// line at the end: its last line ending is then part of the list, where it is dropped.
fn list_then_hidden(content: &str, blocks: &[Block]) -> bool {
    let hidden = |block: &Block| {
        matches!(
            block,
            Block::Node(Node::Definition(_) | Node::Yaml(_) | Node::Toml(_)) | Block::Footnote(_)
        )
    };
    let trailing = blocks.iter().rev().take_while(|block| hidden(block)).count();
    let blank_end = {
        let trimmed = content.trim_end_matches([' ', '\t']);
        ["\n\n", "\r\n\r\n", "\n\r\n", "\r\n\n", "\r\r"]
            .iter()
            .any(|end| trimmed.ends_with(end))
    };
    if trailing == 0 || blank_end || blocks.len() == trailing {
        return false;
    }
    let list = &blocks[blocks.len() - trailing - 1];
    // Without a blank line in between, what follows would belong to the list.
    matches!(list, Block::List(_))
        && matches!((end_line(list), start_line(&blocks[blocks.len() - trailing])), (Some(end), Some(start)) if start > end + 1)
}

/// The line on which the last block that produces output ends.
fn last_rendered(blocks: &[Block]) -> Option<usize> {
    blocks.iter().rev().find_map(|block| match block {
        Block::Node(Node::Definition(_) | Node::Yaml(_) | Node::Toml(_)) | Block::Footnote(_) => None,
        _ => end_line(block),
    })
}

fn end_line(block: &Block) -> Option<usize> {
    match block {
        Block::Node(node) => node.position().map(|position| position.end.line),
        Block::Fenced(fenced) => fenced.node.position().map(|position| position.end.line),
        Block::Inline(InlineBlock {
            kind: InlineKind::Heading { position, .. },
            ..
        }) => Some(position.end.line),
        Block::Inline(block) => block.source.lines.last().map(|(_, point)| point.line),
        Block::Quote(quote) => Some(quote.position.end.line),
        // The position of an item includes the blank lines that follow it.
        Block::List(list) => list.items.last().and_then(|item| {
            item.children
                .iter()
                .rev()
                .find_map(end_line)
                .or(Some(item.position.start.line))
        }),
        Block::Footnote(footnote) => Some(footnote.position.end.line),
        Block::Table(items) => items.last().map(|item| match item {
            TableItem::Cell { position, .. } | TableItem::Align { position, .. } => position.end.line,
        }),
        Block::Jsx(_) | Block::Error(_) => None,
    }
}

fn start_line(block: &Block) -> Option<usize> {
    match block {
        Block::Node(node) => node.position().map(|position| position.start.line),
        Block::Fenced(fenced) => fenced.node.position().map(|position| position.start.line),
        Block::Inline(InlineBlock {
            kind: InlineKind::Heading { position, .. },
            ..
        }) => Some(position.start.line),
        Block::Inline(block) => block.source.lines.first().map(|(_, point)| point.line),
        Block::Quote(quote) => Some(quote.position.start.line),
        Block::List(list) => list.items.first().map(|item| item.position.start.line),
        Block::Footnote(footnote) => Some(footnote.position.start.line),
        Block::Table(items) => items.first().map(|item| match item {
            TableItem::Cell { position, .. } | TableItem::Align { position, .. } => position.start.line,
        }),
        Block::Jsx(_) | Block::Error(_) => None,
    }
}

/// Whether the list is loose: blank lines between its items, or between the blocks of an item.
fn is_loose(list: &ListBlock) -> bool {
    list.spread
        || list.items.iter().any(|item| {
            item.children.windows(2).any(|pair| {
                matches!((end_line(&pair[0]), start_line(&pair[1])), (Some(end), Some(start)) if start > end + 1)
            })
        })
}

struct Html {
    out: String,
    eol: &'static str,
    references: References,
    /// The destination and title of each definition, by normalized label. The first one wins.
    targets: FxHashMap<String, (String, Option<String>)>,
    /// Whether the innermost container is a tight list, which has paragraphs without `<p>`.
    tight: Vec<bool>,
    /// Footnotes in the order they are first referenced, with the number of references.
    calls: Vec<(String, usize)>,
    /// The rendered bodies of footnote definitions.
    notes: Vec<(String, String)>,
    /// The task list check that starts the next paragraph.
    check: Option<bool>,
    /// Whether the node is inside a link, which cannot contain a link.
    in_link: bool,
}

impl Html {
    fn line_ending(&mut self) {
        self.out.push_str(self.eol);
    }

    fn line_ending_if_needed(&mut self) {
        if !matches!(self.out.as_bytes().last(), None | Some(b'\n' | b'\r')) {
            self.line_ending();
        }
    }

    fn collect_targets(&mut self, blocks: &[Block]) {
        for block in blocks {
            match block {
                Block::Node(Node::Definition(definition)) => {
                    self.targets.entry(definition.ident.clone()).or_insert_with(|| {
                        (
                            definition.url.0.clone(),
                            definition.title.as_ref().map(|title| title.0.clone()),
                        )
                    });
                }
                Block::Quote(quote) => self.collect_targets(&quote.children),
                Block::Footnote(footnote) => self.collect_targets(&footnote.children),
                Block::List(list) => list.items.iter().for_each(|item| self.collect_targets(&item.children)),
                _ => {}
            }
        }
    }

    fn blocks(&mut self, blocks: &[Block]) {
        for block in blocks {
            self.block(block);
        }
    }

    fn block(&mut self, block: &Block) {
        match block {
            Block::Node(node) => self.leaf(node),
            Block::Fenced(fenced) => self.fenced(fenced),
            Block::Inline(block) => self.inline_block(block),
            Block::Quote(quote) => {
                self.tight.push(false);
                self.line_ending_if_needed();
                self.out.push_str("<blockquote>");
                self.blocks(&quote.children);
                self.tight.pop();
                self.line_ending_if_needed();
                self.out.push_str("</blockquote>");
            }
            Block::List(list) => self.list(list),
            Block::Footnote(footnote) => {
                self.tight.push(false);
                let outer = std::mem::take(&mut self.out);
                self.blocks(&footnote.children);
                let body = std::mem::replace(&mut self.out, outer);
                self.tight.pop();
                self.notes.push((footnote.ident.clone(), body));
            }
            Block::Table(items) => self.table(items),
            Block::Jsx(_) | Block::Error(_) => {}
        }
    }

    fn leaf(&mut self, node: &Node) {
        match node {
            Node::HorizontalRule(_) => {
                self.line_ending_if_needed();
                self.out.push_str("<hr />");
            }
            Node::Html(html) => {
                self.line_ending_if_needed();
                self.out.push_str(&html.value);
            }
            Node::Code(code) => self.code(code, usize::from(!code.value.is_empty()), true),
            _ => {}
        }
    }

    fn fenced(&mut self, fenced: &FencedBlock) {
        match &fenced.node {
            Node::Code(code) => self.code(code, fenced.lines, fenced.closed),
            Node::Math(math) => {
                self.line_ending_if_needed();
                self.out.push_str("<pre><code class=\"language-math math-display\">");
                self.raw_flow(&math.value, fenced.lines, fenced.closed);
            }
            _ => {}
        }
    }

    fn code(&mut self, code: &Code, lines: usize, closed: bool) {
        self.line_ending_if_needed();
        self.out.push_str("<pre><code");
        if let Some(lang) = code.lang.as_deref().filter(|lang| !lang.is_empty()) {
            self.out.push_str(" class=\"language-");
            encode(&mut self.out, lang);
            self.out.push('"');
        }
        self.out.push('>');
        self.raw_flow(&code.value, lines, closed);
    }

    /// The content of a code or math block: every line ends with a line ending. A fence that is not
    /// closed ends with one as well.
    fn raw_flow(&mut self, value: &str, lines: usize, closed: bool) {
        encode(&mut self.out, value);
        if lines > 0 {
            self.line_ending();
        }
        self.out.push_str("</code></pre>");
        if !closed {
            self.line_ending_if_needed();
        }
    }

    fn inline_block(&mut self, block: &InlineBlock) {
        let nodes = inline::parse(&block.source, &self.references).unwrap_or_default();
        match &block.kind {
            InlineKind::Paragraph => {
                let tight = self.tight.last().copied().unwrap_or(false);
                if !tight {
                    self.line_ending_if_needed();
                    self.out.push_str("<p>");
                } else if !self.out.ends_with("<li>") {
                    // What comes before a tight paragraph is followed by the line ending of the source.
                    self.line_ending_if_needed();
                }
                self.task_check();
                self.inlines(&nodes);
                if !tight {
                    self.out.push_str("</p>");
                }
            }
            InlineKind::Heading { depth, .. } => {
                self.line_ending_if_needed();
                self.out.push_str(&format!("<h{depth}>"));
                self.inlines(&nodes);
                self.out.push_str(&format!("</h{depth}>"));
            }
        }
    }

    fn task_check(&mut self) {
        if let Some(checked) = self.check.take() {
            self.out.push_str("<input type=\"checkbox\" disabled=\"\" ");
            if checked {
                self.out.push_str("checked=\"\" ");
            }
            self.out.push_str("/> ");
        }
    }

    fn list(&mut self, list: &ListBlock) {
        let tight = !is_loose(list);
        self.tight.push(tight);
        self.line_ending_if_needed();
        self.out.push_str(if list.ordered { "<ol" } else { "<ul" });
        if let Some(start) = list.start.filter(|&start| list.ordered && start != 1) {
            self.out.push_str(&format!(" start=\"{start}\""));
        }
        self.out.push('>');

        for item in &list.items {
            self.line_ending_if_needed();
            self.out.push_str("<li>");
            self.check = item.checked;
            let empty = !item.children.iter().any(is_content);
            self.blocks(&item.children);
            self.check = None;

            let tight_paragraph =
                tight
                    && item.children.iter().rfind(|block| is_content(block)).is_some_and(
                        |block| matches!(block, Block::Inline(b) if matches!(b.kind, InlineKind::Paragraph)),
                    );
            if !tight_paragraph && !empty {
                self.line_ending_if_needed();
            }
            self.out.push_str("</li>");
        }

        self.tight.pop();
        self.line_ending();
        self.out.push_str(if list.ordered { "</ol>" } else { "</ul>" });
    }

    fn table(&mut self, items: &[TableItem]) {
        let aligns = items
            .iter()
            .find_map(|item| match item {
                TableItem::Align { align, .. } => Some(align.as_slice()),
                TableItem::Cell { .. } => None,
            })
            .unwrap_or_default();

        let mut rows: Vec<Vec<&Option<InlineSource>>> = Vec::new();
        for item in items {
            if let TableItem::Cell { row, source, .. } = item {
                if rows.len() <= *row {
                    rows.resize_with(row + 1, Vec::new);
                }
                rows[*row].push(source);
            }
        }

        self.line_ending_if_needed();
        self.out.push_str("<table>");
        for (index, row) in rows.iter().enumerate() {
            if index == 0 {
                self.line_ending_if_needed();
                self.out.push_str("<thead>");
            } else if index == 1 {
                self.line_ending_if_needed();
                self.out.push_str("<tbody>");
            }
            self.line_ending_if_needed();
            self.out.push_str("<tr>");
            let tag = if index == 0 { "th" } else { "td" };
            for (column, align) in aligns.iter().enumerate() {
                self.line_ending_if_needed();
                self.out.push_str(&format!("<{tag}"));
                match align {
                    TableAlignKind::Left => self.out.push_str(" align=\"left\""),
                    TableAlignKind::Right => self.out.push_str(" align=\"right\""),
                    TableAlignKind::Center => self.out.push_str(" align=\"center\""),
                    TableAlignKind::None => {}
                }
                self.out.push('>');
                if let Some(Some(source)) = row.get(column) {
                    let nodes = inline::parse(source, &self.references).unwrap_or_default();
                    self.inlines(&nodes);
                }
                self.out.push_str(&format!("</{tag}>"));
            }
            self.line_ending_if_needed();
            self.out.push_str("</tr>");
            if index == 0 {
                self.line_ending_if_needed();
                self.out.push_str("</thead>");
            }
        }
        if rows.len() > 1 {
            self.line_ending_if_needed();
            self.out.push_str("</tbody>");
        }
        self.line_ending_if_needed();
        self.out.push_str("</table>");
    }

    fn inlines(&mut self, nodes: &[Node]) {
        for node in nodes {
            self.inline_node(node);
        }
    }

    fn inline_node(&mut self, node: &Node) {
        match node {
            Node::Text(text) => encode(&mut self.out, &text.value),
            Node::Emphasis(node) => self.wrapped("em", &node.values),
            Node::Strong(node) => self.wrapped("strong", &node.values),
            Node::Delete(node) => self.wrapped("del", &node.values),
            Node::CodeInline(code) => {
                self.out.push_str("<code>");
                self.code_span(&code.value);
            }
            Node::MathInline(math) => {
                self.out.push_str("<code class=\"language-math math-inline\">");
                self.code_span(&math.value);
            }
            Node::Break(_) => {
                self.out.push_str("<br />");
                self.line_ending();
            }
            Node::Html(html) => self.out.push_str(&html.value),
            Node::Link(link) => {
                let title = link.title.as_ref().map(|title| title.0.as_str());
                self.link(&link.url.0, title, &link.values);
            }
            Node::LinkRef(link) => {
                let (url, title) = self.targets.get(&link.ident).cloned().unwrap_or_default();
                self.link(&url, title.as_deref(), &link.values);
            }
            Node::Image(image) => {
                self.image(&image.url, &image.alt, image.title.as_deref());
            }
            Node::ImageRef(image) => {
                let (url, title) = self.targets.get(&image.ident).cloned().unwrap_or_default();
                self.image(&url, &image.alt, title.as_deref());
            }
            Node::FootnoteRef(reference) => self.footnote_call(&reference.ident),
            _ => {}
        }
    }

    /// The content of a code span: line endings are spaces, and one space is dropped on both sides
    /// unless there is nothing but spaces.
    fn code_span(&mut self, value: &str) {
        let mut text = String::with_capacity(value.len());
        let mut chars = value.chars().peekable();
        while let Some(char) = chars.next() {
            match char {
                '\r' if chars.peek() == Some(&'\n') => {}
                '\n' | '\r' => text.push(' '),
                _ => text.push(char),
            }
        }
        let bytes = text.as_bytes();
        // Spaces at the edges were already dropped in the value, but line endings were not.
        let edge = value.starts_with(['\n', '\r']) || value.ends_with(['\n', '\r']);
        let trim = edge
            && bytes.len() > 2
            && bytes[0] == b' '
            && bytes[bytes.len() - 1] == b' '
            && bytes.iter().any(|&b| b != b' ');
        encode(&mut self.out, if trim { &text[1..text.len() - 1] } else { &text });
        self.out.push_str("</code>");
    }

    fn wrapped(&mut self, tag: &str, nodes: &[Node]) {
        self.out.push_str(&format!("<{tag}>"));
        self.inlines(nodes);
        self.out.push_str(&format!("</{tag}>"));
    }

    fn link(&mut self, url: &str, title: Option<&str>, values: &[Node]) {
        if self.in_link {
            self.inlines(values);
            return;
        }
        self.out.push_str("<a href=\"");
        self.out.push_str(&sanitize_with_protocols(url, &SAFE_PROTOCOL_HREF));
        self.out.push('"');
        self.title(title);
        self.out.push('>');
        self.in_link = true;
        self.inlines(values);
        self.in_link = false;
        self.out.push_str("</a>");
    }

    fn image(&mut self, url: &str, alt: &str, title: Option<&str>) {
        self.out.push_str("<img src=\"");
        self.out.push_str(&sanitize_with_protocols(url, &SAFE_PROTOCOL_SRC));
        self.out.push_str("\" alt=\"");
        encode(&mut self.out, alt);
        self.out.push('"');
        self.title(title);
        self.out.push_str(" />");
    }

    fn title(&mut self, title: Option<&str>) {
        if let Some(title) = title {
            self.out.push_str(" title=\"");
            encode(&mut self.out, title);
            self.out.push('"');
        }
    }

    fn footnote_call(&mut self, ident: &str) {
        let index = match self.calls.iter().position(|(id, _)| id == ident) {
            Some(index) => index,
            None => {
                self.calls.push((ident.to_string(), 0));
                self.calls.len() - 1
            }
        };
        self.calls[index].1 += 1;
        let count = self.calls[index].1;

        let id = sanitize(ident);
        self.out.push_str(&format!(
            "<sup><a href=\"#user-content-fn-{id}\" id=\"user-content-fnref-{id}"
        ));
        if count > 1 {
            self.out.push_str(&format!("-{count}"));
        }
        self.out.push_str(&format!(
            "\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">{}</a></sup>",
            index + 1
        ));
    }

    /// The list of the footnotes that were referenced, after everything else.
    fn footnote_section(&mut self) {
        self.line_ending_if_needed();
        self.out
            .push_str("<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>");
        self.line_ending();
        self.out.push_str("<ol>");

        for (ident, count) in std::mem::take(&mut self.calls) {
            let id = sanitize(&ident);
            self.line_ending();
            self.out.push_str(&format!("<li id=\"user-content-fn-{id}\">"));
            self.line_ending();

            let mut backreferences = String::new();
            for number in 1..=count {
                if number > 1 {
                    backreferences.push(' ');
                }
                backreferences.push_str(&format!("<a href=\"#user-content-fnref-{id}"));
                if number > 1 {
                    backreferences.push_str(&format!("-{number}"));
                }
                backreferences.push_str(
                    "\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">\u{21a9}",
                );
                if number > 1 {
                    backreferences.push_str(&format!("<sup>{number}</sup>"));
                }
                backreferences.push_str("</a>");
            }

            let body = self
                .notes
                .iter()
                .find(|(note, _)| *note == ident)
                .map(|(_, body)| body.as_str())
                .unwrap_or_default();
            let trimmed = body.trim_end_matches(['\n', '\r']);
            if let Some(before) = trimmed.strip_suffix("</p>") {
                self.out.push_str(before);
                self.out.push(' ');
                self.out.push_str(&backreferences);
                self.out.push_str("</p>");
                self.out.push_str(&body[trimmed.len()..]);
            } else {
                self.out.push_str(body);
                self.line_ending_if_needed();
                self.out.push_str(&backreferences);
            }
            self.line_ending_if_needed();
            self.out.push_str("</li>");
        }

        self.line_ending();
        self.out.push_str("</ol>");
        self.line_ending();
        self.out.push_str("</section>");
        self.line_ending();
    }
}

/// Whether a block shows up in the output of a list item.
fn is_content(block: &Block) -> bool {
    !matches!(block, Block::Node(Node::Definition(_)) | Block::Footnote(_))
}

/// Writes `value` with the characters that are special in HTML escaped, and NUL replaced.
fn encode(out: &mut String, value: &str) {
    let mut start = 0;
    for (index, byte) in value.bytes().enumerate() {
        let replacement = match byte {
            b'\0' => "\u{fffd}",
            b'&' => "&amp;",
            b'"' => "&quot;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            _ => continue,
        };
        out.push_str(&value[start..index]);
        out.push_str(replacement);
        start = index + 1;
    }
    out.push_str(&value[start..]);
}

/// Percent-encodes what is not allowed in a URL, keeps what already is encoded, and escapes it for HTML.
fn sanitize(value: &str) -> String {
    // NUL is replaced before the URL is encoded.
    let chars = value
        .chars()
        .map(|char| if char == '\0' { '\u{fffd}' } else { char })
        .collect::<Vec<_>>();
    let mut normalized = String::with_capacity(value.len());
    let mut index = 0;
    let mut start = 0;
    let mut buffer = [0; 4];

    while index < chars.len() {
        let char = chars[index];

        // A correct percent encoded value.
        if char == '%'
            && index + 2 < chars.len()
            && chars[index + 1].is_ascii_alphanumeric()
            && chars[index + 2].is_ascii_alphanumeric()
        {
            index += 3;
            continue;
        }

        if char >= '\u{80}' || !matches!(char, '!' | '#' | '$' | '&'..=';' | '=' | '?'..='Z' | '_' | 'a'..='z' | '~') {
            normalized.extend(&chars[start..index]);
            char.encode_utf8(&mut buffer);
            for byte in &buffer[..char.len_utf8()] {
                normalized.push_str(&format!("%{byte:02X}"));
            }
            start = index + 1;
        }
        index += 1;
    }
    normalized.extend(&chars[start..]);

    let mut result = String::with_capacity(normalized.len());
    encode(&mut result, &normalized);
    result
}

/// Like [`sanitize`], and empty when the URL has a protocol that is not in `protocols`.
fn sanitize_with_protocols(value: &str, protocols: &[&str]) -> String {
    let value = sanitize(value);
    let end = value.find(['?', '#', '/']);
    let mut colon = value.find(':');

    // A colon after `?`, `#` or `/` is not the end of a protocol.
    if let (Some(end), Some(index)) = (end, colon)
        && index > end
    {
        colon = None;
    }
    if let Some(colon) = colon
        && !protocols.contains(&value[..colon].to_lowercase().as_str())
    {
        return String::new();
    }
    value
}
