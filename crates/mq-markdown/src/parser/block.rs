//! Block structure parsing: resolves the lines into a tree of [`Block`]s.
//!
//! The constructs that can start at a line are the rules of [`rule::Rules`], tried in order, and a
//! paragraph when none starts. Containers (blockquote, list item, footnote definition) collect their
//! lines without the container prefix and parse them again, up to [`MAX_DEPTH`] levels.

use super::code::Fence;
use super::definition;
use super::flavor::Flavor;
use super::html_flow::{self, Kind as HtmlKind};
use super::inline;
use super::line::{Indent, Line, leading_ws, split_lines, tab_stops};
use super::mdx_flow::{FlowOutcome, absorbed_until, blocks_lazy_continuation, probe_mdx_flow};
use super::table;
mod collect;
mod footnote;
mod html;
mod leaf;
mod list;
mod rule;

use super::tree::{Block, FootnoteBlock, InlineBlock, InlineKind, InlineSource, Item, ListBlock, QuoteBlock};
use crate::node::{HeadingDepth, Html, ListMarker, Node, Point, Position, Toml, Yaml};
use collect::Collector;
use footnote::{FOOTNOTE_INDENT, FootnoteMarker, footnote, footnote_marker};
use html::{html_block, html_start};
use list::{ItemMarker, list};
use rule::{BlockRule, Cx, Rules, fallback};

/// Lines indented by this many columns or more are code, not other blocks.
pub(super) const CODE_INDENT: usize = 4;

/// Containers nested deeper than this are parsed as plain paragraph text, which bounds recursion.
const MAX_DEPTH: usize = 128;

/// What the first line of a container cannot start because the container interrupted a paragraph.
#[derive(Clone, Copy, Default)]
struct Interrupt {
    /// Indented code cannot interrupt a paragraph.
    code: bool,
    /// Empty items and ordered items that do not start at 1 cannot interrupt a paragraph.
    list: bool,
}

pub(super) fn parse(src: &str, flavor: Flavor, read_frontmatter: bool) -> Vec<Block> {
    let stops = tab_stops(src);
    let mut lines = split_lines(src, flavor, &stops);
    // A byte order mark at the start of the document is not content.
    if let Some(first) = lines.first_mut().filter(|line| line.text.starts_with('\u{feff}')) {
        let mark = '\u{feff}'.len_utf8();
        // Columns start after it.
        let text = &first.text[mark..];
        *first = Line {
            text,
            whole: &first.whole[mark..],
            base: first.base + mark,
            ws: leading_ws(text),
            ..*first
        };
    }
    match read_frontmatter.then(|| frontmatter(&lines)).flatten() {
        Some((node, next)) => {
            let mut blocks = vec![Block::Node(node)];
            blocks.extend(parse_blocks(&lines[next..], 0, Interrupt::default()));
            blocks
        }
        None => parse_blocks(&lines, 0, Interrupt::default()),
    }
}

/// Frontmatter at the very start of the document: YAML between `---` lines, or TOML between `+++`
/// lines. Without a closing line it is not frontmatter.
fn frontmatter(lines: &[Line<'_>]) -> Option<(Node, usize)> {
    let is_fence = |line: &Line<'_>, marker: &str| {
        line.text
            .strip_prefix(marker)
            .is_some_and(|rest| rest.trim_matches([' ', '\t']).is_empty())
    };
    let first = lines.first()?;
    let marker = ["---", "+++"].into_iter().find(|marker| is_fence(first, marker))?;
    let close = lines.iter().skip(1).position(|line| is_fence(line, marker))? + 1;

    let parts = lines[1..close]
        .iter()
        .map(|line| (line.text, line.eol))
        .collect::<Vec<_>>();
    let value = join_lines(&parts);
    let position = Some(Position {
        start: first.point(0),
        end: lines[close].end(),
    });
    let node = if marker == "---" {
        Node::Yaml(Yaml { value, position })
    } else {
        Node::Toml(Toml { value, position })
    };

    Some((node, close + 1))
}

/// Parses `lines` into blocks. `interrupting` restricts the first line when the container these
/// lines belong to interrupted an open paragraph.
fn parse_blocks(lines: &[Line<'_>], depth: usize, interrupting: Interrupt) -> Vec<Block> {
    let containers = depth < MAX_DEPTH;
    let mut blocks = Vec::new();
    let mut index = 0;
    let mut after_paragraph = interrupting;
    // Set after a container, and whether a blank line or an empty list item preceded the next block.
    let mut after_container = false;
    let mut blank_between = false;

    while index < lines.len() {
        let interrupting = std::mem::take(&mut after_paragraph);
        let line = &lines[index];
        if line.is_blank() {
            blank_between = true;
            index += 1;
            continue;
        }
        let closed_container = std::mem::take(&mut after_container);
        let separated = std::mem::take(&mut blank_between);

        let indent = line.indent();
        let cx = Cx {
            lines,
            index,
            line,
            indent,
            start: LineStart::of(line, indent),
            rest: &line.text[indent.bytes..],
            depth,
            containers,
            interrupting,
            closed_container,
            separated,
        };
        let step = Rules::parse(&cx, &mut blocks).unwrap_or_else(|| fallback(&cx, &mut blocks));
        index = step.next;
        after_container = step.container;
        after_paragraph = step.interrupt;
    }

    blocks
}

/// A block that a line starts by its first characters, before the rest of the document is known.
///
/// The constructs are told apart by their first character, so a line starts at most one of them. Block
/// rules, lazy continuation and the end of paragraphs and tables all decide from this one result.
enum LineStart<'a> {
    Fence(Fence<'a>),
    /// An ATX heading of this depth.
    Atx(HeadingDepth),
    ThematicBreak,
    Blockquote,
    Item(ItemMarker),
    Html(HtmlKind),
    Footnote(FootnoteMarker<'a>),
}

impl LineStart<'_> {
    /// What `line` starts, when it is indented less than code. Setext underlines, tables and MDX flow
    /// content depend on the lines around them, so they are not among these.
    pub(super) fn of<'a>(line: &Line<'a>, indent: Indent) -> Option<LineStart<'a>> {
        if indent.columns >= line.code_indent() {
            return None;
        }
        let rest = &line.text[indent.bytes..];
        match rest.as_bytes().first()? {
            b'`' | b'~' | b'$' => Fence::open(rest, line.flavor).map(LineStart::Fence),
            b'#' => atx_depth(rest).map(LineStart::Atx),
            b'>' => Some(LineStart::Blockquote),
            b'<' => html_start(line, rest).map(LineStart::Html),
            b'[' => footnote_marker(line).map(LineStart::Footnote),
            b'*' | b'-' | b'_' if is_thematic_break(line, indent.bytes) => Some(LineStart::ThematicBreak),
            _ => ItemMarker::parse(line).map(LineStart::Item),
        }
    }

    /// Whether this start ends an open paragraph. A complete HTML tag, an empty item and an ordered
    /// item that does not start at 1 cannot.
    fn interrupts_paragraph(&self) -> bool {
        match self {
            LineStart::Html(kind) => *kind != HtmlKind::Complete,
            LineStart::Item(marker) => marker.interrupts_paragraph(),
            _ => true,
        }
    }
}

/// The line without its blockquote marker and the one space or column after it, if it has a marker.
fn strip_blockquote(line: Line<'_>) -> Option<Line<'_>> {
    let indent = line.indent();
    matches!(LineStart::of(&line, indent), Some(LineStart::Blockquote)).then(|| after_blockquote_marker(line, indent))
}

/// The line after the blockquote marker that follows `indent`, and the one space or column after it.
fn after_blockquote_marker(line: Line<'_>, indent: Indent) -> Line<'_> {
    let after = line.skip(indent.bytes + 1);
    match after.text.as_bytes().first() {
        Some(b' ') => after.skip(1),
        // A tab is consumed by one column only.
        Some(b'\t') => after.skip_columns(1),
        _ => after,
    }
}

fn blockquote(
    lines: &[Line<'_>],
    start: usize,
    depth: usize,
    interrupting: Interrupt,
    blocks: &mut Vec<Block>,
) -> usize {
    let mut collector = Collector::new(depth, interrupting);
    let mut index = start;

    while let Some(line) = lines.get(index) {
        match strip_blockquote(*line) {
            Some(stripped) => collector.push(stripped),
            // Lazy continuation of a paragraph.
            None if collector.lazy(lines, index) => {}
            None => break,
        }
        index += 1;
    }
    let inner = collector.lines;

    let children = parse_blocks(&inner, depth + 1, interrupting);
    let mut end = inner.last().map_or_else(|| lines[start].end(), Line::end);
    // An unclosed fence at the end of the document extends past the last line.
    if let Some(child_end) = children.last().and_then(last_end)
        && (child_end.line, child_end.column) > (end.line, end.column)
    {
        end = child_end;
    }
    let position = Position {
        start: lines[start].point(0),
        end,
    };
    blocks.push(Block::Quote(QuoteBlock { children, position }));

    index
}

/// The end of the last node that `block` produces.
fn last_end(block: &Block) -> Option<Point> {
    match block {
        Block::Node(node) => node.position().map(|position| position.end),
        Block::Fenced(fenced) => fenced.node.position().map(|position| position.end),
        Block::Quote(quote) => Some(quote.position.end.clone()),
        Block::List(list) => list.items.last().map(|item| item.position.end.clone()),
        Block::Footnote(footnote) => Some(footnote.position.end.clone()),
        Block::Inline(_) | Block::Table(_) | Block::Error(_) => None,
        Block::Jsx(tag) => Some(tag.position.end.clone()),
    }
}

/// Extends the position of a trailing blockquote, or of the last item of a trailing list and of the
/// lists nested at its end, over the blank lines that follow.
fn extend_last_items(blocks: &mut [Block], end: &Point, quotes: bool) {
    match blocks.last_mut() {
        Some(Block::List(list)) => {
            if let Some(item) = list.items.last_mut()
                // An empty innermost item keeps its own end.
                && !item.children.is_empty()
            {
                item.position.end = end.clone();
                extend_last_items(&mut item.children, end, quotes);
            }
        }
        // A blockquote is only extended by a single whitespace-only line that ends the document.
        Some(Block::Quote(quote)) if quotes => extend_quote(quote, end),
        // A footnote definition also runs over the blank lines that follow.
        Some(Block::Footnote(footnote)) => {
            footnote.position.end = end.clone();
            extend_last_items(&mut footnote.children, end, quotes);
        }
        _ => {}
    }
}

/// Whether the content of `block` ends with a paragraph or a setext heading.
fn ends_in_text(block: &Block) -> bool {
    match block {
        Block::Inline(block) => match &block.kind {
            InlineKind::Paragraph => true,
            // A setext heading spans its underline line, an ATX heading does not.
            InlineKind::Heading { position, .. } => position.start.line != position.end.line,
        },
        Block::List(list) => list
            .items
            .last()
            .and_then(|item| item.children.last())
            .is_some_and(ends_in_text),
        Block::Quote(quote) => quote.children.last().is_some_and(ends_in_text),
        Block::Footnote(footnote) => footnote.children.last().is_some_and(ends_in_text),
        // A definition is a paragraph whose content was consumed.
        Block::Node(node) => matches!(node, Node::Definition(_)),
        Block::Fenced(_) | Block::Table(_) | Block::Jsx(_) | Block::Error(_) => false,
    }
}

/// Extends a blockquote that ends in a paragraph, and the blockquotes nested at its end.
fn extend_quote(quote: &mut QuoteBlock, end: &Point) {
    if !quote.children.last().is_some_and(ends_in_text) {
        return;
    }
    quote.position.end = end.clone();
    match quote.children.last_mut() {
        Some(Block::Quote(inner)) => extend_quote(inner, end),
        Some(Block::Footnote(footnote)) => footnote.position.end = end.clone(),
        _ => {}
    }
}

/// Returns the heading depth when `rest` (indent already removed) is an ATX heading line.
fn atx_depth(rest: &str) -> Option<HeadingDepth> {
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    let delimited = matches!(rest[hashes..].chars().next(), None | Some(' ' | '\t'));
    let depth = u8::try_from(hashes).ok().and_then(HeadingDepth::new)?;
    delimited.then_some(depth)
}

fn atx_heading(line: &Line<'_>, indent: usize, depth: HeadingDepth) -> Block {
    let rest = &line.text[indent + usize::from(depth.get())..];
    let content = rest.trim_matches([' ', '\t']);
    // A closing sequence must be preceded by whitespace, unless it is the whole content.
    let content = {
        let without_closing = content.trim_end_matches('#');
        if without_closing.is_empty() || without_closing.ends_with([' ', '\t']) {
            without_closing.trim_end_matches([' ', '\t'])
        } else {
            content
        }
    };
    let start = line.point(offset_in(line.text, content));

    Block::Inline(InlineBlock {
        source: InlineSource::new(std::iter::once((content, "", start))),
        kind: InlineKind::Heading {
            depth,
            position: Position {
                start: line.point(0),
                end: line.end(),
            },
        },
    })
}

/// Joins `(text, eol)` pairs, keeping each line's original terminator between lines.
pub(super) fn join_lines(parts: &[(&str, &str)]) -> String {
    let mut value = String::with_capacity(parts.iter().map(|(text, eol)| text.len() + eol.len()).sum());
    for (index, (text, eol)) in parts.iter().enumerate() {
        value.push_str(text);
        if index + 1 < parts.len() {
            value.push_str(eol);
        }
    }
    value
}

/// Byte offset of `child` within `parent`, where `child` is a subslice of `parent`.
fn offset_in(parent: &str, child: &str) -> usize {
    child.as_ptr() as usize - parent.as_ptr() as usize
}

/// Whether the rest of `line` after `indent` bytes is a thematic break.
fn is_thematic_break(line: &Line<'_>, indent: usize) -> bool {
    let rest = &line.text[indent..];
    let marker = match rest.as_bytes().first() {
        Some(b'*') => 0,
        Some(b'-') => 1,
        Some(b'_') => 2,
        _ => return false,
    };
    // Anything else after this point rules it out without looking at the rest of the line.
    if line.others[marker] > line.column + indent {
        return false;
    }
    let marker = rest.as_bytes()[0];
    rest.bytes().filter(|&byte| byte == marker).take(3).count() == 3
}

/// Returns the setext heading depth when `line` is a setext underline.
fn setext_depth(line: &Line<'_>) -> Option<HeadingDepth> {
    let Indent { columns, bytes: indent } = line.indent();
    let content = line.text[indent..].trim_end_matches([' ', '\t']);
    let marker = content.chars().next()?;
    if line.lazy || columns >= line.code_indent() || !content.chars().all(|c| c == marker) {
        return None;
    }
    match marker {
        '=' => Some(HeadingDepth::H1),
        '-' => Some(HeadingDepth::H2),
        _ => None,
    }
}

/// Whether a line without its container prefix can continue an open paragraph of that container.
/// Any list marker ends the paragraph here, even one that could not interrupt it elsewhere.
fn is_lazy_continuation(line: &Line<'_>) -> bool {
    let start = LineStart::of(line, line.indent());
    !ends_paragraph(line, start.as_ref()) && !matches!(start, Some(LineStart::Item(_)))
}

/// Whether `line` ends the paragraph that is being collected.
fn interrupts_paragraph(line: &Line<'_>) -> bool {
    ends_paragraph(line, LineStart::of(line, line.indent()).as_ref())
}

/// Whether `line`, which starts `start`, ends an open paragraph.
fn ends_paragraph(line: &Line<'_>, start: Option<&LineStart<'_>>) -> bool {
    // A lazy line was already accepted as a continuation by an enclosing container.
    line.is_blank() || (!line.lazy && start.is_some_and(LineStart::interrupts_paragraph))
}

/// Whether `line` ends a table. Unlike a paragraph, a table is also ended by a list item that is empty
/// or that is numbered from other than one.
fn ends_table(line: &Line<'_>) -> bool {
    let start = LineStart::of(line, line.indent());
    ends_paragraph(line, start.as_ref()) || (!line.lazy && matches!(start, Some(LineStart::Item(_))))
}

/// Collects a paragraph or a setext heading starting at `lines[start]`, with the definitions at its
/// start as blocks of their own. Returns the index after it and what it restricts on the next line.
fn paragraph(lines: &[Line<'_>], start: usize, blocks: &mut Vec<Block>) -> (usize, Interrupt) {
    let mut index = start + 1;
    // Lines up to this one are taken by flow content that turned out to be text.
    let mut absorbed = match probe_mdx_flow(lines, start) {
        Some(FlowOutcome::Nok(Some(last))) => absorbed_until(lines, start, last),
        _ => start,
    };
    let mut setext = None;

    while let Some(line) = lines.get(index) {
        if table::starts_at(lines, index, interrupts_paragraph) {
            break;
        }
        // MDX flow content, such as a tag or an expression alone on a line, interrupts a paragraph.
        // What only looks like flow content still takes the lines it spans, whatever they start.
        match probe_mdx_flow(lines, index) {
            Some(FlowOutcome::Flow(_)) => break,
            Some(FlowOutcome::Nok(Some(last))) => absorbed = absorbed.max(absorbed_until(lines, index, last)),
            _ => {}
        }
        if index <= absorbed {
            index += 1;
            continue;
        }
        if let Some(depth) = setext_depth(line) {
            setext = Some((depth, index));
            break;
        }
        if interrupts_paragraph(line) {
            break;
        }
        index += 1;
    }

    // Definitions at the start of the paragraph are blocks of their own.
    let mut first = start;
    if lines[start].text.trim_start_matches([' ', '\t']).starts_with('[') {
        let source = paragraph_source(&lines[start..index]);
        let spans = lines[start..index]
            .iter()
            .map(|line| Position {
                start: line.point(0),
                end: line.end(),
            })
            .collect::<Vec<_>>();
        let (definitions, used) = definition::extract(&source, &spans);
        blocks.extend(definitions.into_iter().map(Block::Node));
        first += used;
    }
    if first == index {
        // Nothing is left of the paragraph, so a setext underline starts a paragraph of text instead.
        return match setext {
            Some((_, underline)) => (
                underline,
                // The underline is still a line of a paragraph, which an empty list item cannot interrupt.
                Interrupt {
                    code: false,
                    list: true,
                },
            ),
            None => (index, Interrupt::default()),
        };
    }
    let source = paragraph_source(&lines[first..index]);

    match setext {
        Some((depth, underline)) => {
            blocks.push(Block::Inline(InlineBlock {
                source,
                kind: InlineKind::Heading {
                    depth,
                    position: Position {
                        start: lines[first].point(0),
                        end: lines[underline].end(),
                    },
                },
            }));
            (underline + 1, Interrupt::default())
        }
        None => {
            blocks.push(Block::Inline(InlineBlock {
                source,
                kind: InlineKind::Paragraph,
            }));
            // A one-line paragraph right after a container does not restrict what follows it.
            (index, Interrupt::default())
        }
    }
}

/// The raw inline content of the lines of a paragraph. Trailing spaces only matter before a line
/// ending, where they can make a hard break, so only the last line is trimmed at its end.
fn paragraph_source(lines: &[Line<'_>]) -> InlineSource {
    let last = lines.len() - 1;
    InlineSource::new(lines.iter().enumerate().map(|(index, line)| {
        // The indentation of later lines stays in the content, where code and JSX keep it as it is.
        let text = if index == 0 {
            line.text.trim_start_matches([' ', '\t'])
        } else {
            line.text
        };
        let text = if index == last {
            text.trim_end_matches([' ', '\t'])
        } else {
            text
        };
        (text, line.eol, line.point(offset_in(line.text, text)))
    }))
}
