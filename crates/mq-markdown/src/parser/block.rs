//! Block structure parsing: resolves each line into leaf blocks.
//!
//! Containers (blockquote, list) are resolved by stripping their prefix from each line and parsing
//! the remaining lines again, up to [`MAX_DEPTH`] levels. The remaining leaf blocks (HTML, table,
//! definition, frontmatter, math) are not handled yet, so their lines currently fall into paragraphs.

use super::code::{Fence, fenced_code, indented_code};
use super::definition;
use super::html_flow::{self, Kind as HtmlKind};
use super::inline;
use super::line::{Line, split_lines};
use super::mdx_flow::{
    FlowOutcome, absorbed_until, blocks_lazy_continuation, looks_like_mdx_flow, mdx_flow, probe_mdx_flow,
};
use super::table;
use super::tree::{Block, FootnoteBlock, InlineBlock, InlineKind, InlineSource, Item, ListBlock, QuoteBlock};
use crate::node::{HorizontalRule, Html, Node, Point, Position, Toml, Yaml};

/// Lines indented by this many columns or more are code, not other blocks.
pub(super) const CODE_INDENT: usize = 4;

/// Containers nested deeper than this are parsed as plain paragraph text, which bounds recursion.
const MAX_DEPTH: usize = 128;

/// Which list markers end a paragraph on its second line although they could not interrupt it.
#[derive(Clone, Copy, PartialEq)]
enum Release {
    None,
    /// Markers with content.
    Markers,
    /// Every marker.
    AllMarkers,
}

/// How far a paragraph of dashes right after a setext heading extends, in markdown-rs.
#[derive(Clone, Copy, PartialEq)]
enum Dashes {
    /// Not such a paragraph.
    No,
    /// Up to the next indented code line, at the top level.
    UntilCode,
    /// A single line, inside containers.
    OneLine,
}

/// What the first line of a container cannot start because the container interrupted a paragraph.
#[derive(Clone, Copy, Default)]
struct Interrupt {
    /// Indented code cannot interrupt a paragraph.
    code: bool,
    /// Empty items and ordered items that do not start at 1 cannot interrupt a paragraph.
    list: bool,
}

pub(super) fn parse(src: &str, mdx: bool) -> Vec<Block> {
    let lines = split_lines(src, mdx);
    match frontmatter(&lines).filter(|_| !mdx) {
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
    let mut after_setext = false;
    // Set after a container, and whether a blank line or an empty list item preceded the next block.
    let mut after_container = false;
    let mut after_list = false;
    let mut blank_between = false;

    while index < lines.len() {
        let interrupting = std::mem::take(&mut after_paragraph);
        let follows_setext = std::mem::take(&mut after_setext);
        let line = &lines[index];
        if line.is_blank() {
            blank_between = true;
            index += 1;
            continue;
        }
        let closed_container = std::mem::take(&mut after_container);
        let closed_list = std::mem::take(&mut after_list);
        let separated = std::mem::take(&mut blank_between);

        let (columns, indent) = line.indent();
        let rest = &line.text[indent..];

        if columns >= line.code_indent() && !interrupting.code {
            index = indented_code(lines, index, closed_list || closed_container && !separated, &mut blocks);
        } else if columns >= line.code_indent() {
            // Indented code cannot interrupt a paragraph.
            let (next, interrupt, setext) = paragraph(lines, index, Release::None, Dashes::No, &mut blocks);
            index = next;
            after_paragraph = interrupt;
            after_setext = setext;
        } else if let Some(fence) = Fence::open(rest, !line.mdx) {
            // The end of an unclosed fence without content is quirky right after a container.
            let own_end = closed_container
                && !(separated
                    && matches!(blocks.last(), Some(Block::List(l)) if l.items.last().is_some_and(|i| i.children.is_empty())));
            index = fenced_code(lines, index, indent, &fence, own_end, &mut blocks);
        } else if let Some(depth) = atx_depth(rest) {
            blocks.push(atx_heading(line, indent, depth));
            index += 1;
        } else if is_thematic_break(rest) && !(follows_setext && is_dash_run(rest)) {
            blocks.push(Block::Node(Node::HorizontalRule(HorizontalRule {
                position: Some(Position {
                    start: line.point(0),
                    end: line.end(),
                }),
            })));
            index += 1;
        } else if containers && blockquote_marker(line).is_some() {
            index = blockquote(lines, index, depth, interrupting, &mut blocks);
            after_container = true;
        } else if let Some(marker) =
            ListMarker::parse(line).filter(|marker| containers && (!interrupting.list || marker.interrupts_paragraph()))
        {
            index = list(lines, index, &marker, depth, interrupting, &mut blocks);
            after_container = true;
            after_list = true;
        } else if let Some(kind) = html_start(line, rest) {
            index = html_block(lines, index, kind, &mut blocks);
        } else if let Some(marker) = footnote_marker(line).filter(|_| containers) {
            index = footnote(lines, index, &marker, depth, interrupting, &mut blocks);
            after_container = true;
        } else if let Some((items, next)) = table::parse(lines, index, interrupts_paragraph) {
            blocks.push(Block::Table(items));
            index = next;
        } else if line.mdx
            && matches!(rest.as_bytes().first(), Some(b'<' | b'{'))
            && let FlowOutcome::Flow(next) = mdx_flow(lines, index, &mut blocks)
        {
            index = next;
            // Flow content that interrupted a paragraph keeps the restrictions that came with it.
            after_paragraph = interrupting;
        } else {
            let release = match (closed_container, separated) {
                (false, _) => Release::None,
                (true, false) => Release::AllMarkers,
                (true, true) => Release::Markers,
            };
            let (next, interrupt, setext) = paragraph(
                lines,
                index,
                release,
                match (follows_setext && is_dash_run(rest), depth) {
                    (false, _) => Dashes::No,
                    (true, 0) => Dashes::UntilCode,
                    (true, _) => Dashes::OneLine,
                },
                &mut blocks,
            );
            index = next;
            after_paragraph = interrupt;
            after_setext = setext;
        }
    }

    blocks
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
    /// Whether the last line was a setext underline. markdown-rs still lets unindented text continue.
    after_setext: bool,
    /// Restrictions on the next fed line, the first line of a container that interrupted a paragraph.
    restricted: Interrupt,
}

impl<'a> LeafState<'a> {
    fn new(restricted: Interrupt) -> Self {
        Self {
            fence: None,
            html: None,
            table: false,
            header: None,
            paragraph: false,
            paragraph_containers: 0,
            after_setext: false,
            restricted,
        }
    }

    /// Whether `line` without its container prefix continues the paragraph of the collected lines.
    fn continues_with(&self, line: &Line<'_>) -> bool {
        (self.paragraph || (self.after_setext && line.indent().0 < line.code_indent())) && is_lazy_continuation(line)
    }

    fn feed(&mut self, line: &Line<'a>) {
        let mut line = *line;
        let restricted = std::mem::take(&mut self.restricted);
        let follows_setext = std::mem::take(&mut self.after_setext);
        let mut containers = 0;
        // Whether a paragraph was open when the line started, for markers deeper on the same line.
        let open = self.paragraph;
        loop {
            let (columns, indent) = line.indent();
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
            if let Some(fence) = Fence::open(rest, !line.mdx) {
                self.fence = Some(fence);
                self.paragraph = false;
                return;
            }
            if self.paragraph && containers == self.paragraph_containers && setext_depth(&line).is_some() {
                self.paragraph = false;
                self.after_setext = true;
                return;
            }
            // Dashes right after a setext heading start a paragraph, not a thematic break.
            if follows_setext && is_dash_run(rest) {
                self.paragraph = true;
                self.paragraph_containers = containers;
                return;
            }
            if atx_depth(rest).is_some() || is_thematic_break(rest) {
                self.paragraph = false;
                return;
            }
            if let Some(kind) = html_start(&line, rest).filter(|&kind| kind != HtmlKind::Complete || !self.paragraph) {
                let closed = match kind {
                    HtmlKind::Basic | HtmlKind::Complete => false,
                    _ => html_flow::ends_in(kind, &rest[html_flow::first_line_offset(kind)..]),
                };
                self.html = (!closed).then_some(kind);
                self.paragraph = false;
                return;
            }
            if let Some(stripped) = strip_blockquote(line) {
                line = stripped;
                containers += 1;
                continue;
            }
            if let Some(marker) = ListMarker::parse(&line).filter(|m| {
                // A marker in or inside the container of the open paragraph has to be able to interrupt it.
                let continues_paragraph = open && containers >= self.paragraph_containers;
                !(restricted.list || continues_paragraph) || m.interrupts_paragraph()
            }) {
                line = marker.content(line);
                containers += 1;
                // The rest of the line starts a new item.
                self.paragraph = false;
                continue;
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

/// Bytes to skip to get past a blockquote marker (`>` and one optional space), if `line` has one.
fn blockquote_marker(line: &Line<'_>) -> Option<usize> {
    let (columns, indent) = line.indent();
    let rest = &line.text[indent..];
    if columns >= line.code_indent() || !rest.starts_with('>') {
        return None;
    }
    let optional_space = usize::from(rest[1..].starts_with([' ', '\t']));
    Some(indent + 1 + optional_space)
}

/// The line without its blockquote marker and the one space or column after it, if it has a marker.
fn strip_blockquote(line: Line<'_>) -> Option<Line<'_>> {
    let (columns, indent) = line.indent();
    if columns >= line.code_indent() || !line.text[indent..].starts_with('>') {
        return None;
    }
    let after = line.skip(indent + 1);
    Some(match after.text.as_bytes().first() {
        Some(b' ') => after.skip(1),
        // A tab is consumed by one column only.
        Some(b'\t') => after.skip_columns(1),
        _ => after,
    })
}

fn blockquote(
    lines: &[Line<'_>],
    start: usize,
    depth: usize,
    interrupting: Interrupt,
    blocks: &mut Vec<Block>,
) -> usize {
    let mut inner = Vec::new();
    let mut state = LeafState::new(interrupting);
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

struct ListMarker {
    ordered: bool,
    /// `-`, `+` or `*` for bullets, `.` or `)` for ordered lists.
    delimiter: u8,
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

impl ListMarker {
    fn parse(line: &Line<'_>) -> Option<Self> {
        let (columns, indent) = line.indent();
        if columns >= line.code_indent() {
            return None;
        }
        let rest = &line.text[indent..];
        let bytes = rest.as_bytes();
        if is_thematic_break(rest) {
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
        let delimiter = bytes[marker_len - 1];

        let after = &rest[marker_len..];
        let (spaces, blanks) = line.skip(indent + marker_len).indent();
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
            delimiter,
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
        self.ordered == other.ordered && self.delimiter == other.delimiter
    }

    /// Whether this marker may interrupt a paragraph.
    fn interrupts_paragraph(&self) -> bool {
        !self.empty && (!self.ordered || self.start == 1)
    }
}

fn list(
    lines: &[Line<'_>],
    start: usize,
    first: &ListMarker,
    depth: usize,
    interrupting: Interrupt,
    blocks: &mut Vec<Block>,
) -> usize {
    let mut items = Vec::new();
    let mut spread = false;
    let mut index = start;

    loop {
        let marker = ListMarker::parse(&lines[index]).unwrap_or_else(|| unreachable!("checked by the caller"));
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
        let next = (end..lines.len()).find(|&i| !lines[i].is_blank());
        match next.filter(|&i| ListMarker::parse(&lines[i]).is_some_and(|m| m.same_list(first))) {
            Some(next) => {
                spread |= next > end || items.last().is_some_and(ends_in_empty_item);
                index = next;
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
        spread,
        items,
    }));

    index
}

/// The end of the last node that `block` produces.
fn last_end(block: &Block) -> Option<Point> {
    match block {
        Block::Node(node) => node.position().map(|position| position.end),
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
        Block::Table(_) | Block::Jsx(_) | Block::Error(_) => false,
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

/// Whether the item ends with a nested list whose last item is empty. markdown-rs counts that
/// like a blank line between the item and the next one.
fn ends_in_empty_item(item: &Item) -> bool {
    match item.children.last() {
        Some(Block::List(list)) => list
            .items
            .last()
            .is_some_and(|last| last.children.is_empty() || ends_in_empty_item(last)),
        _ => false,
    }
}

/// Collects one item starting at `start`, returning it and the index of the first line after it.
fn list_item(
    lines: &[Line<'_>],
    start: usize,
    marker: &ListMarker,
    depth: usize,
    interrupting: Interrupt,
) -> (Item, usize) {
    let mut first = marker.content(lines[start]);
    first.own_chunk = first.indent().0 >= first.code_indent();
    let mut inner = vec![first];
    let mut state = LeafState::new(interrupting);
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
            blanks.push(line.skip(line.text.len()));
        } else if line.lazy {
            state.feed(line);
            inner.push(*line);
        } else if line.indent().0 >= marker.width {
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

    // The checkbox is on the first content line, which is the next one when the marker line is empty.
    let target = usize::from(first.is_blank());
    let checked = inner
        .get(target)
        .filter(|_| !ends_with_setext_underline(&inner[target + 1..]))
        .and_then(task_checkbox)
        .map(|(checked, bytes)| {
            inner[target] = inner[target].skip(bytes);
            checked
        });

    // Trailing blank lines belong to whatever follows the item, but still extend its position.
    let reaches_end = index == lines.len();
    index -= blanks.len();

    // An unclosed fence in an item includes its line terminator when the next line is blank or starts
    // another container, but not when a plain paragraph or thematic break follows.
    let next = lines.get(index);
    let item_end = next
        .is_none_or(|line| line.is_blank() || blockquote_marker(line).is_some() || ListMarker::parse(line).is_some());
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

/// Whether the paragraph that continues on `lines` is turned into a setext heading.
fn ends_with_setext_underline(lines: &[Line<'_>]) -> bool {
    for line in lines {
        if setext_depth(line).is_some() {
            return true;
        }
        if interrupts_paragraph(line) {
            return false;
        }
    }
    false
}

/// Recognises a GFM task marker (`[ ] ` or `[x] `) at the start of an item's content, returning
/// the checked state and the bytes to skip.
fn task_checkbox(line: &Line<'_>) -> Option<(bool, usize)> {
    if line.mdx {
        return None;
    }
    let bytes = line.text.as_bytes();
    let checked = match bytes.get(..4)? {
        [b'[', b' ', b']', b' ' | b'\t'] => false,
        [b'[', b'x' | b'X', b']', b' ' | b'\t'] => true,
        _ => return None,
    };
    (!line.text[4..].trim_matches([' ', '\t']).is_empty()).then_some((checked, 4))
}

/// Returns the heading depth when `rest` (indent already removed) is an ATX heading line.
fn atx_depth(rest: &str) -> Option<u8> {
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    let delimited = matches!(rest[hashes..].chars().next(), None | Some(' ' | '\t'));
    ((1..=6).contains(&hashes) && delimited).then_some(hashes as u8)
}

fn atx_heading(line: &Line<'_>, indent: usize, depth: u8) -> Block {
    let rest = &line.text[indent + depth as usize..];
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
    // markdown-rs drops further `#` sequences (and the whitespace between them) at the start of the text.
    let content = {
        let mut content = content;
        while content.starts_with('#') {
            content = content.trim_start_matches('#').trim_start_matches([' ', '\t']);
        }
        content
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

/// Whether `rest` is only dashes, like `---`, which markdown-rs does not read as a thematic break
/// right after a setext heading.
fn is_dash_run(rest: &str) -> bool {
    let content = rest.trim_end_matches([' ', '\t']);
    content.len() >= 3 && content.bytes().all(|b| b == b'-')
}

fn is_thematic_break(rest: &str) -> bool {
    let Some(marker) = rest.chars().next().filter(|c| matches!(c, '*' | '-' | '_')) else {
        return false;
    };
    let mut count = 0;
    for c in rest.chars() {
        if c == marker {
            count += 1;
        } else if !matches!(c, ' ' | '\t') {
            return false;
        }
    }
    count >= 3
}

/// Returns the setext heading depth when `line` is a setext underline.
fn setext_depth(line: &Line<'_>) -> Option<u8> {
    let (columns, indent) = line.indent();
    let content = line.text[indent..].trim_end_matches([' ', '\t']);
    let marker = content.chars().next()?;
    if line.lazy || columns >= line.code_indent() || !content.chars().all(|c| c == marker) {
        return None;
    }
    match marker {
        '=' => Some(1),
        '-' => Some(2),
        _ => None,
    }
}

/// Whether a line without its container prefix can continue an open paragraph of that container.
/// Any list marker ends the paragraph here, even one that could not interrupt it elsewhere.
fn is_lazy_continuation(line: &Line<'_>) -> bool {
    !interrupts_paragraph(line) && ListMarker::parse(line).is_none() && !starts_construct(line)
}

/// Whether the line is only the start of a construct that a next line could still complete: one or two
/// underscores (a thematic break), one or two backticks or tildes (a fence), a dollar, or an angle
/// bracket. markdown-rs does not continue paragraphs lazily with such a line.
fn starts_construct(line: &Line<'_>) -> bool {
    // Only the end of the document leaves such a start undecided.
    if !line.eof || !line.eol.is_empty() {
        return false;
    }
    let text = line.text.trim_matches([' ', '\t']);
    match text.as_bytes() {
        [b'<'] => !line.mdx,
        [b'$'] => true,
        [first @ (b'`' | b'~'), rest @ ..] => rest.len() < 2 && rest.iter().all(|b| b == first),
        _ => {
            // Underscores with spaces between, like `_ _`.
            let markers = text.bytes().filter(|&b| b == b'_').count();
            (1..3).contains(&markers) && text.bytes().all(|b| matches!(b, b'_' | b' ' | b'\t'))
        }
    }
}

/// Whether `line` ends the paragraph that is being collected.
fn interrupts_paragraph(line: &Line<'_>) -> bool {
    if line.is_blank() {
        return true;
    }
    // Already accepted as a continuation by an enclosing container.
    if line.lazy {
        return false;
    }
    let (columns, indent) = line.indent();
    let rest = &line.text[indent..];
    columns < line.code_indent()
        && (Fence::open(rest, !line.mdx).is_some()
            || atx_depth(rest).is_some()
            || is_thematic_break(rest)
            || blockquote_marker(line).is_some()
            || footnote_marker(line).is_some()
            || html_start(line, rest).is_some_and(|kind| kind != HtmlKind::Complete)
            || ListMarker::parse(line).is_some_and(|marker| marker.interrupts_paragraph()))
}

/// Returns the index after the block and what it restricts on the first line of a following container.
///
/// A paragraph of `dashes` right after a setext heading is cut short as described by [`Dashes`]. A paragraph with a `release` has its second line ended by list markers that could not interrupt it.
fn paragraph(
    lines: &[Line<'_>],
    start: usize,
    release: Release,
    dashes: Dashes,
    blocks: &mut Vec<Block>,
) -> (usize, Interrupt, bool) {
    let mut index = start + 1;
    // Lines up to this one are taken by flow content that turned out to be text.
    let mut absorbed = match probe_mdx_flow(lines, start) {
        Some(FlowOutcome::Nok(Some(last))) => absorbed_until(lines, start, last),
        _ => start,
    };
    let mut setext = None;
    // Whether a marker that could not interrupt the paragraph ended it anyway.
    let mut released = false;

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
        let releases = index == start + 1
            && ListMarker::parse(line).is_some_and(|marker| match release {
                Release::None => false,
                Release::Markers => !marker.empty,
                Release::AllMarkers => true,
            });
        if releases {
            released = true;
            break;
        }
        if let Some(depth) = setext_depth(line) {
            setext = Some((depth, index));
            break;
        }
        if dashes == Dashes::OneLine
            || interrupts_paragraph(line)
            || (dashes == Dashes::UntilCode && line.indent().0 >= line.code_indent())
        {
            break;
        }
        index += 1;
    }

    // Definitions at the start of the paragraph are blocks of their own.
    let mut first = start;
    if lines[start].text.trim_start_matches([' ', '\t']).starts_with('[') {
        let source = paragraph_source(&lines[start..index]);
        let starts = lines[start..index]
            .iter()
            .map(|line| (line.point(0), line.end()))
            .collect::<Vec<_>>();
        let (definitions, used) = definition::extract(&source, &starts);
        blocks.extend(definitions.into_iter().map(Block::Node));
        first += used;
    }
    if first == index {
        // Nothing is left of the paragraph, so a setext underline starts a paragraph of text instead.
        return match setext {
            Some((_, underline)) => (underline, Interrupt::default(), true),
            None => (index, Interrupt::default(), false),
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
            (underline + 1, Interrupt::default(), true)
        }
        None => {
            blocks.push(Block::Inline(InlineBlock {
                source,
                kind: InlineKind::Paragraph,
            }));
            // A one-line paragraph right after a container does not restrict what follows it.
            (
                index,
                Interrupt {
                    code: dashes == Dashes::No,
                    list: !(dashes != Dashes::No || released || release == Release::AllMarkers && index == start + 1),
                },
                false,
            )
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

/// The start of a footnote definition: `[^label]:`.
struct FootnoteMarker<'a> {
    label: &'a str,
    /// Bytes from the start of the line to the content.
    content: usize,
}

fn footnote_marker<'a>(line: &Line<'a>) -> Option<FootnoteMarker<'a>> {
    if line.mdx {
        return None;
    }
    let (columns, indent) = line.indent();
    let rest = line.text[indent..].strip_prefix("[^")?;
    if columns >= CODE_INDENT {
        return None;
    }
    let close = rest.find(']')?;
    let label = &rest[..close];
    if label.is_empty() || label.bytes().any(|b| matches!(b, b' ' | b'\t' | b'[' | b'\r' | b'\n')) {
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
    let mut state = LeafState::new(interrupting);
    state.feed(&first);
    let mut blanks = Vec::new();
    let mut index = start + 1;

    while let Some(line) = lines.get(index) {
        if line.is_blank() {
            if first.is_blank() && inner.len() == 1 {
                break;
            }
            blanks.push(line.skip(line.text.len()));
        } else if line.indent().0 >= CODE_INDENT {
            for blank in &blanks {
                state.feed(blank);
            }
            inner.append(&mut blanks);
            let stripped = line.skip_columns(CODE_INDENT);
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
    let position = Position {
        start: lines[start].point(0),
        end: blanks
            .last()
            .or(inner.last())
            .map_or_else(|| lines[start].end(), Line::end),
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
            let (_, indent) = first.indent();
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

/// The kind of HTML block that starts at `rest`; MDX has none.
fn html_start(line: &Line<'_>, rest: &str) -> Option<HtmlKind> {
    if line.mdx { None } else { html_flow::start(rest) }
}
