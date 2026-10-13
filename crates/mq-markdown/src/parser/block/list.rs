//! Lists and their items.

use super::{
    Block, Collector, Indent, InlineBlock, InlineKind, Interrupt, Item, Line, LineStart, ListBlock, ListMarker,
    Position, extend_last_items, interrupts_paragraph, is_thematic_break, parse_blocks, setext_depth, table,
};

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
    pub(super) width: usize,
    /// Whether the first line has no content after the marker.
    pub(super) empty: bool,
}

impl ItemMarker {
    pub(super) fn parse(line: &Line<'_>) -> Option<Self> {
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
    pub(super) fn content<'a>(&self, line: Line<'a>) -> Line<'a> {
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
    pub(super) fn interrupts_paragraph(&self) -> bool {
        !self.empty && (!self.ordered || self.start == 1)
    }
}

pub(super) fn list(
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

/// Collects one item starting at `start`, returning it and the index of the first line after it.
pub(super) fn list_item(
    lines: &[Line<'_>],
    start: usize,
    marker: &ItemMarker,
    depth: usize,
    interrupting: Interrupt,
) -> (Item, usize) {
    let first = marker.content(lines[start]);
    let mut collector = Collector::new(depth, interrupting);
    collector.push(first);
    let mut last_blank_has_whitespace = false;
    let mut index = start + 1;

    while let Some(line) = lines.get(index) {
        if line.is_blank() {
            // An item that starts with a blank line cannot continue after another one.
            if first.is_blank() && collector.lines.len() == 1 {
                break;
            }
            last_blank_has_whitespace = !line.text.is_empty();
            collector.blank(line.skip_columns(marker.width));
        } else if line.lazy {
            collector.push(*line);
        } else if line.indent().columns >= marker.width {
            collector.push(line.skip_columns(marker.width));
        } else if !collector.lazy(lines, index) {
            break;
        }
        index += 1;
    }
    collector.keep_fence_blanks();
    let Collector {
        lines: mut inner,
        blanks,
        ..
    } = collector;

    let checkbox = inner
        .iter()
        .position(|line| !line.is_blank())
        .and_then(|index| task_checkbox(&inner[index..]));

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

    let content_end = inner.last().map_or_else(|| lines[start].end(), Line::end);
    let position = Position {
        start: lines[start].point(0),
        end: blanks.last().map_or_else(|| content_end.clone(), Line::end),
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
        content_end,
    };

    (item, index)
}

/// Recognises a GFM task marker (`[ ] ` or `[x] `) at the start of an item's content, returning
/// the checked state and the bytes to skip.
fn task_checkbox(lines: &[Line<'_>]) -> Option<bool> {
    let line = &lines[0];
    if !line.flavor.has_gfm() {
        return None;
    }
    let bytes = line.text.as_bytes();
    let checked = match bytes.get(..3)? {
        [b'[', b' ', b']'] => false,
        [b'[', b'x' | b'X', b']'] => true,
        _ => return None,
    };
    // Whitespace follows the marker, which a line ending is when the paragraph goes on in the next line.
    match bytes.get(3) {
        Some(b' ' | b'\t') => Some(checked),
        None if continues_paragraph(lines) => Some(checked),
        _ => None,
    }
}

/// Whether the paragraph that starts at `lines[0]` goes on in `lines[1]`, as opposed to a blank line,
/// another block, a table or a setext underline.
fn continues_paragraph(lines: &[Line<'_>]) -> bool {
    lines.get(1).is_some_and(|next| {
        !interrupts_paragraph(next) && setext_depth(next).is_none() && !table::starts_at(lines, 0, interrupts_paragraph)
    })
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
