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
use super::line::{Indent, Line, split_lines};
use super::mdx_flow::{FlowOutcome, absorbed_until, blocks_lazy_continuation, looks_like_mdx_flow, probe_mdx_flow};
use super::table;
mod rule;

use super::tree::{Block, FootnoteBlock, InlineBlock, InlineKind, InlineSource, Item, ListBlock, QuoteBlock};
use crate::node::{HeadingDepth, Html, ListMarker, Node, Point, Position, Toml, Yaml};
use rule::{BlockRule, Cx, Rules, fallback};

/// Lines indented by this many columns or more are code, not other blocks.
pub(super) const CODE_INDENT: usize = 4;

/// The indentation of the lines that continue a footnote definition.
const FOOTNOTE_INDENT: usize = 4;

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
    let mut lines = split_lines(src, flavor);
    // A byte order mark at the start of the document is not content.
    if let Some(first) = lines.first_mut().filter(|line| line.text.starts_with('\u{feff}')) {
        *first = first.skip('\u{feff}'.len_utf8());
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
pub(super) enum LineStart<'a> {
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

/// Tracks just enough of the block state of already collected lines to tell whether a paragraph
/// is open, which decides whether a following line can be a lazy continuation.
struct LeafState<'a> {
    fence: Option<Fence<'a>>,
    /// An HTML block that has not ended yet.
    html: Option<HtmlKind>,
    /// A table that has not ended yet. Its rows cannot be continued lazily.
    table: bool,
    /// The number of cells of the last line when it was paragraph text with a pipe, which a delimiter
    /// row with as many cells can turn into a header.
    header: Option<usize>,
    paragraph: bool,
    /// Number of container markers on the line that opened the paragraph.
    paragraph_containers: usize,
    /// Restrictions on the next fed line, the first line of a container that interrupted a paragraph.
    restricted: Interrupt,
    /// How many more containers a line can open before the depth limit, which no longer open any.
    budget: usize,
}

impl<'a> LeafState<'a> {
    /// `depth` is the depth of the container that collects the lines.
    fn new(restricted: Interrupt, depth: usize) -> Self {
        Self {
            fence: None,
            html: None,
            table: false,
            header: None,
            paragraph: false,
            paragraph_containers: 0,
            restricted,
            budget: MAX_DEPTH.saturating_sub(depth + 1),
        }
    }

    /// Whether `line` without its container prefix continues the paragraph of the collected lines.
    fn continues_with(&self, line: &Line<'_>) -> bool {
        self.paragraph && is_lazy_continuation(line)
    }

    fn feed(&mut self, line: &Line<'a>) {
        let mut line = *line;
        let restricted = std::mem::take(&mut self.restricted);
        let mut containers = 0;
        // Whether a paragraph was open when the line started, for markers deeper on the same line.
        let open = self.paragraph;
        loop {
            let line_indent = line.indent();
            let Indent { columns, bytes: indent } = line_indent;
            let rest = &line.text[indent..];

            if let Some(fence) = &self.fence {
                if columns < line.code_indent() && fence.is_closed_by(rest) {
                    self.fence = None;
                }
                return;
            }

            if let Some(kind) = self.html {
                let ends = match kind {
                    HtmlKind::Basic | HtmlKind::Complete => line.is_blank(),
                    _ => html_flow::ends_in(kind, line.text),
                };
                if ends {
                    self.html = None;
                }
                self.paragraph = false;
                return;
            }
            if line.is_blank() {
                self.paragraph = false;
                self.table = false;
                self.header = None;
                return;
            }
            let header = std::mem::take(&mut self.header);
            if self.table {
                if interrupts_paragraph(&line) {
                    self.table = false;
                } else {
                    self.paragraph = false;
                    return;
                }
            } else if self.paragraph
                && !line.lazy
                && header.is_some()
                && table::delimiter_cells(&line) == header
                && !interrupts_paragraph(&line)
            {
                self.table = true;
                self.paragraph = false;
                return;
            }
            if columns >= line.code_indent() {
                // Continues an open paragraph of this container, otherwise it is indented code (or
                // text when the container interrupted a paragraph).
                if !(self.paragraph && containers <= self.paragraph_containers) {
                    self.paragraph = restricted.code;
                    self.paragraph_containers = containers;
                }
                return;
            }
            let start = LineStart::of(&line, line_indent);
            if let Some(LineStart::Fence(fence)) = start {
                self.fence = Some(fence);
                self.paragraph = false;
                return;
            }
            if self.paragraph && containers == self.paragraph_containers && setext_depth(&line).is_some() {
                self.paragraph = false;
                return;
            }
            match start {
                Some(LineStart::Atx(_) | LineStart::ThematicBreak) => {
                    self.paragraph = false;
                    return;
                }
                Some(LineStart::Html(kind)) if kind != HtmlKind::Complete || !self.paragraph => {
                    let closed = match kind {
                        HtmlKind::Basic | HtmlKind::Complete => false,
                        _ => html_flow::ends_in(kind, &rest[html_flow::first_line_offset(kind)..]),
                    };
                    self.html = (!closed).then_some(kind);
                    self.paragraph = false;
                    return;
                }
                Some(LineStart::Blockquote) if containers < self.budget => {
                    line = after_blockquote_marker(line, line_indent);
                    containers += 1;
                    continue;
                }
                // A marker in or inside the container of the open paragraph has to be able to interrupt it.
                Some(LineStart::Item(marker))
                    if containers < self.budget
                        && (!(restricted.list || (open && containers >= self.paragraph_containers))
                            || marker.interrupts_paragraph()) =>
                {
                    line = marker.content(line);
                    containers += 1;
                    // The rest of the line starts a new item.
                    self.paragraph = false;
                    continue;
                }
                _ => {}
            }

            // MDX flow content is not part of a paragraph.
            if looks_like_mdx_flow(&line) {
                self.paragraph = false;
                return;
            }
            self.header = rest.contains('|').then(|| table::row_cells(&line));
            // A lazy line belongs to the paragraph that is already open, in its container.
            if !(line.lazy && self.paragraph) {
                self.paragraph_containers = containers;
            }
            self.paragraph = true;
            return;
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
    let mut inner = Vec::new();
    let mut state = LeafState::new(interrupting, depth);
    let mut index = start;

    while let Some(line) = lines.get(index) {
        let stripped = match strip_blockquote(*line) {
            Some(stripped) => stripped,
            // Lazy continuation of a paragraph.
            None if state.continues_with(line) && !blocks_lazy_continuation(lines, index) => {
                Line { lazy: true, ..*line }
            }
            None => break,
        };
        state.feed(&stripped);
        inner.push(stripped);
        index += 1;
    }

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

#[derive(Clone)]
pub(super) struct ItemMarker {
    ordered: bool,
    kind: ListMarker,
    start: u32,
    /// Bytes from the start of the line to the end of the marker.
    after_marker: usize,
    /// Columns of whitespace between the marker and the content that belong to the marker.
    gap: usize,
    /// Columns that continuation lines must be indented by to belong to the item.
    width: usize,
    /// Whether the first line has no content after the marker.
    empty: bool,
}

impl ItemMarker {
    fn parse(line: &Line<'_>) -> Option<Self> {
        let Indent { columns, bytes: indent } = line.indent();
        if columns >= line.code_indent() {
            return None;
        }
        let rest = &line.text[indent..];
        let bytes = rest.as_bytes();
        if is_thematic_break(line, indent) {
            return None;
        }

        let (ordered, start, marker_len) = match *bytes.first()? {
            b'-' | b'+' | b'*' => (false, 0, 1),
            b'0'..=b'9' => {
                let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
                if digits > 9 || !matches!(bytes.get(digits), Some(b'.' | b')')) {
                    return None;
                }
                (true, rest[..digits].parse().ok()?, digits + 1)
            }
            _ => return None,
        };
        let kind = match bytes[marker_len - 1] {
            b'-' => ListMarker::Dash,
            b'+' => ListMarker::Plus,
            b'*' => ListMarker::Star,
            b'.' => ListMarker::Period,
            _ => ListMarker::Paren,
        };

        let after = &rest[marker_len..];
        let Indent {
            columns: spaces,
            bytes: blanks,
        } = line.skip(indent + marker_len).indent();
        if blanks == 0 && !after.is_empty() {
            return None;
        }

        let empty = blanks == after.len();
        // Five or more columns means the content is indented code, so only one belongs to the marker.
        let gap = if empty || spaces > line.code_indent() {
            1
        } else {
            spaces
        };

        Some(Self {
            ordered,
            kind,
            start,
            after_marker: indent + marker_len,
            gap,
            width: columns + marker_len + gap,
            empty,
        })
    }

    /// The line without the marker and the whitespace that belongs to it.
    fn content<'a>(&self, line: Line<'a>) -> Line<'a> {
        if self.empty {
            line.skip(line.text.len())
        } else {
            line.skip(self.after_marker).skip_columns(self.gap)
        }
    }

    fn same_list(&self, other: &Self) -> bool {
        self.ordered == other.ordered && self.kind == other.kind
    }

    /// Whether this marker may interrupt a paragraph.
    fn interrupts_paragraph(&self) -> bool {
        !self.empty && (!self.ordered || self.start == 1)
    }
}

fn list(
    lines: &[Line<'_>],
    start: usize,
    first: &ItemMarker,
    depth: usize,
    interrupting: Interrupt,
    blocks: &mut Vec<Block>,
) -> usize {
    let mut items = Vec::new();
    let mut spread = false;
    let mut index = start;
    let mut marker = first.clone();

    loop {
        let (item, end) = list_item(
            lines,
            index,
            &marker,
            depth,
            if items.is_empty() {
                interrupting
            } else {
                Interrupt::default()
            },
        );
        items.push(item);

        // Blank lines between items make the list loose.
        let next = (end..lines.len()).find(|&i| !lines[i].is_blank()).and_then(|i| {
            ItemMarker::parse(&lines[i])
                .filter(|m| m.same_list(first))
                .map(|m| (i, m))
        });
        match next {
            Some((next, next_marker)) => {
                spread |= next > end;
                index = next;
                marker = next_marker;
            }
            None => {
                index = end;
                break;
            }
        }
    }

    blocks.push(Block::List(ListBlock {
        ordered: first.ordered,
        start: first.ordered.then_some(first.start),
        marker: first.kind,
        spread,
        items,
    }));

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

/// Collects one item starting at `start`, returning it and the index of the first line after it.
fn list_item(
    lines: &[Line<'_>],
    start: usize,
    marker: &ItemMarker,
    depth: usize,
    interrupting: Interrupt,
) -> (Item, usize) {
    let first = marker.content(lines[start]);
    let mut inner = vec![first];
    let mut state = LeafState::new(interrupting, depth);
    state.feed(&first);
    let mut blanks = Vec::new();
    let mut last_blank_has_whitespace = false;
    let mut index = start + 1;

    while let Some(line) = lines.get(index) {
        if line.is_blank() {
            // An item that starts with a blank line cannot continue after another one.
            if first.is_blank() && inner.len() == 1 {
                break;
            }
            last_blank_has_whitespace = !line.text.is_empty();
            blanks.push(line.skip_columns(marker.width));
        } else if line.lazy {
            state.feed(line);
            inner.push(*line);
        } else if line.indent().columns >= marker.width {
            for blank in &blanks {
                state.feed(blank);
            }
            inner.append(&mut blanks);
            let stripped = line.skip_columns(marker.width);
            state.feed(&stripped);
            inner.push(stripped);
        } else if blanks.is_empty() && state.continues_with(line) && !blocks_lazy_continuation(lines, index) {
            let lazy = Line { lazy: true, ..*line };
            state.feed(&lazy);
            inner.push(lazy);
        } else {
            break;
        }
        index += 1;
    }

    let checkbox = inner
        .iter()
        .position(|line| !line.is_blank())
        .and_then(|index| task_checkbox(&inner[index], inner.get(index + 1)));

    // Trailing blank lines belong to whatever follows the item, but still extend its position.
    let reaches_end = index == lines.len();
    index -= blanks.len();

    // An unclosed fence in an item includes its line terminator when the next line is blank or starts
    // another container, but not when a plain paragraph or thematic break follows.
    let next = lines.get(index);
    let item_end = next.is_none_or(|line| {
        line.is_blank()
            || matches!(
                LineStart::of(line, line.indent()),
                Some(LineStart::Blockquote | LineStart::Item(_))
            )
    });
    if let Some(last) = inner.last_mut() {
        last.item_end = item_end;
    }

    let position = Position {
        start: lines[start].point(0),
        end: blanks
            .last()
            .or(inner.last())
            .map_or_else(|| lines[start].end(), Line::end),
    };
    let mut children = parse_blocks(&inner, depth + 1, interrupting);
    // The checkbox is the start of the text of the first paragraph.
    let checked = checkbox.filter(|_| remove_task_marker(&mut children));
    if let Some(blank) = blanks.last() {
        extend_last_items(
            &mut children,
            &blank.end(),
            last_blank_has_whitespace && reaches_end && blanks.len() == 1 && blank.eol.is_empty(),
        );
    }
    let item = Item {
        checked,
        children,
        position,
    };

    (item, index)
}

/// Recognises a GFM task marker (`[ ] ` or `[x] `) at the start of an item's content, returning
/// the checked state and the bytes to skip.
fn task_checkbox(line: &Line<'_>, next: Option<&Line<'_>>) -> Option<bool> {
    if !line.flavor.has_gfm() {
        return None;
    }
    let bytes = line.text.as_bytes();
    let checked = match bytes.get(..3)? {
        [b'[', b' ', b']'] => false,
        [b'[', b'x' | b'X', b']'] => true,
        _ => return None,
    };
    // Whitespace follows the marker, which a line ending is when the item goes on in the next line.
    match bytes.get(3) {
        Some(b' ' | b'\t') => Some(checked),
        None if next.is_some_and(|next| !next.is_blank()) => Some(checked),
        _ => None,
    }
}

/// Removes the checkbox from the text of the first paragraph of an item, and the paragraph when nothing
/// else is in it. Returns whether the item starts with one.
fn remove_task_marker(children: &mut Vec<Block>) -> bool {
    let Some(Block::Inline(InlineBlock {
        source,
        kind: InlineKind::Paragraph,
    })) = children.first_mut()
    else {
        return false;
    };
    if !matches!(source.text.as_bytes().get(..3), Some([b'[', b' ' | b'x' | b'X', b']'])) {
        return false;
    }
    source.remove_prefix_and_one(3);
    if source.text.is_empty() {
        children.remove(0);
    }
    true
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
        let offset = offset_in(line.text, text);
        let point = if index == 0 {
            line.content_point(offset)
        } else {
            line.point(offset)
        };
        (text, line.eol, point)
    }))
}

/// The start of a footnote definition: `[^label]:`.
pub(super) struct FootnoteMarker<'a> {
    label: &'a str,
    /// Bytes from the start of the line to the content.
    content: usize,
}

fn footnote_marker<'a>(line: &Line<'a>) -> Option<FootnoteMarker<'a>> {
    if !line.flavor.has_gfm() {
        return None;
    }
    let Indent { columns, bytes: indent } = line.indent();
    let rest = line.text[indent..].strip_prefix("[^")?;
    if columns >= line.code_indent() {
        return None;
    }
    // A backslash escapes the bracket that follows it, as well as another backslash.
    let bytes = rest.as_bytes();
    let mut close = 0;
    while close < bytes.len() && bytes[close] != b']' {
        if bytes[close] == b'[' {
            return None;
        }
        close += if bytes[close] == b'\\' && matches!(bytes.get(close + 1), Some(b'[' | b'\\' | b']')) {
            2
        } else {
            1
        };
    }
    if close >= bytes.len() {
        return None;
    }
    let label = &rest[..close];
    if label.is_empty() || label.bytes().any(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n')) {
        return None;
    }
    let after = rest[close + 1..].strip_prefix(':')?;
    let blanks = after.len() - after.trim_start_matches([' ', '\t']).len();
    Some(FootnoteMarker {
        label,
        content: indent + 2 + close + 2 + blanks,
    })
}

/// Collects a footnote definition, which continues on lines indented by four columns.
fn footnote(
    lines: &[Line<'_>],
    start: usize,
    marker: &FootnoteMarker<'_>,
    depth: usize,
    interrupting: Interrupt,
    blocks: &mut Vec<Block>,
) -> usize {
    let first = lines[start].skip(marker.content);
    let mut inner = vec![first];
    let mut state = LeafState::new(interrupting, depth);
    state.feed(&first);
    let mut blanks = Vec::new();
    let mut index = start + 1;

    while let Some(line) = lines.get(index) {
        if line.is_blank() {
            blanks.push(line.skip(line.text.len()));
        } else if line.indent().columns >= FOOTNOTE_INDENT {
            for blank in &blanks {
                state.feed(blank);
            }
            inner.append(&mut blanks);
            let stripped = line.skip_columns(FOOTNOTE_INDENT);
            state.feed(&stripped);
            inner.push(stripped);
        } else if blanks.is_empty() && state.continues_with(line) && !blocks_lazy_continuation(lines, index) {
            let lazy = Line { lazy: true, ..*line };
            state.feed(&lazy);
            inner.push(lazy);
        } else {
            break;
        }
        index += 1;
    }

    // Trailing blank lines belong to whatever follows, but still extend the position.
    index -= blanks.len();
    let end = blanks
        .last()
        .or(inner.last())
        .map_or_else(|| lines[start].end(), Line::end);
    let position = Position {
        start: lines[start].point(0),
        end,
    };
    blocks.push(Block::Footnote(FootnoteBlock {
        ident: inline::normalize(marker.label),
        children: parse_blocks(&inner, depth + 1, interrupting),
        position,
    }));

    index
}

/// Collects an HTML block of `kind` starting at `lines[start]`, which is indented less than four columns.
fn html_block(lines: &[Line<'_>], start: usize, kind: HtmlKind, blocks: &mut Vec<Block>) -> usize {
    let first = &lines[start];
    let mut last = start;
    let mut unterminated = false;

    match kind {
        HtmlKind::Basic | HtmlKind::Complete => {
            // Ends before a blank line, and lines cannot continue a paragraph lazily.
            for (index, line) in lines.iter().enumerate().skip(start + 1) {
                if line.is_blank() || line.lazy {
                    break;
                }
                last = index;
            }
        }
        _ => {
            let indent = first.indent().bytes;
            let from = indent + html_flow::first_line_offset(kind);
            if !html_flow::ends_in(kind, &first.text[from..]) {
                let mut ended = false;
                for (index, line) in lines.iter().enumerate().skip(start + 1) {
                    if line.lazy {
                        break;
                    }
                    last = index;
                    if html_flow::ends_in(kind, line.text) {
                        ended = true;
                        break;
                    }
                }
                // A block that never ends runs to the end of the document, line ending included.
                unterminated = !ended && lines[last].eof && !lines[last].eol.is_empty();
            }
        }
    }

    let parts = lines[start..=last]
        .iter()
        .map(|line| (line.text, line.eol))
        .collect::<Vec<_>>();
    let mut value = join_lines(&parts);
    let end = if unterminated {
        value.push_str(lines[last].eol);
        Point {
            line: lines[last].number + 1,
            column: 1,
        }
    } else {
        lines[last].end()
    };
    blocks.push(Block::Node(Node::Html(Html {
        value,
        position: Some(Position {
            start: first.point(0),
            end,
        }),
    })));

    last + 1
}

/// The kind of HTML block that starts at `rest`, in a flavor that has HTML.
fn html_start(line: &Line<'_>, rest: &str) -> Option<HtmlKind> {
    line.flavor.has_html().then(|| html_flow::start(rest)).flatten()
}
