//! GFM tables.

use super::line::Line;
use super::tree::{InlineSource, TableItem};
use crate::node::{Point, Position, TableAlignKind};

/// One cell of a row, as byte offsets within the line.
struct Cell<'a> {
    /// Start of the cell, at its leading pipe when it has one.
    start: usize,
    end: usize,
    content: &'a str,
    content_offset: usize,
}

/// Splits a row into cells on unescaped pipes. Leading and trailing pipes do not make cells.
fn split_row<'a>(line: &Line<'a>) -> Vec<Cell<'a>> {
    let text = line.text;
    let indent = text.len() - text.trim_start_matches([' ', '\t']).len();
    let end = text.trim_end_matches([' ', '\t']).len().max(indent);
    let bytes = text.as_bytes();

    let mut pipes = Vec::new();
    let mut index = indent;
    while index < end {
        match bytes[index] {
            b'\\' => index += 1,
            b'|' => pipes.push(index),
            _ => {}
        }
        index += 1;
    }

    let leading = pipes.first() == Some(&indent);
    let trailing = pipes.last().is_some_and(|&pipe| pipe + 1 == end);

    // Segments between pipes: (start of the leading pipe or content, content start, content end).
    let mut segments = Vec::with_capacity(pipes.len() + 1);
    let mut from = indent;
    for &pipe in &pipes {
        segments.push((from, pipe));
        from = pipe + 1;
    }
    segments.push((from, end));

    let mut first = 0;
    let mut last = segments.len();
    if leading {
        first = 1;
    }
    if trailing && last - first > 1 {
        last -= 1;
    }

    (first..last)
        .map(|index| {
            let (from, to) = segments[index];
            let raw = &text[from..to];
            let content = raw.trim_matches([' ', '\t']);
            let content_offset = from + (raw.len() - raw.trim_start_matches([' ', '\t']).len());
            // The first cell starts at the start of the line, later ones at their leading pipe.
            let start = if index == first { 0 } else { pipes[index - 1] };
            // The last cell takes the whitespace at the end of the line along.
            let end = if index + 1 == last { text.len() } else { pipes[index] };
            Cell {
                start,
                end,
                content,
                content_offset,
            }
        })
        .collect()
}

/// Parses a delimiter cell such as `:-:` into its alignment.
fn align(content: &str) -> Option<TableAlignKind> {
    let left = content.starts_with(':');
    let right = content.len() > 1 && content.ends_with(':');
    let dashes = &content[usize::from(left)..content.len() - usize::from(right)];
    if dashes.is_empty() || !dashes.bytes().all(|b| b == b'-') {
        return None;
    }
    Some(match (left, right) {
        (true, true) => TableAlignKind::Center,
        (true, false) => TableAlignKind::Left,
        (false, true) => TableAlignKind::Right,
        (false, false) => TableAlignKind::None,
    })
}

/// The alignments of a table that starts with `lines[start]` as its header, if it is one.
fn header<'a>(
    lines: &[Line<'a>],
    start: usize,
    starts_block: fn(&Line<'_>) -> bool,
) -> Option<(Vec<Cell<'a>>, Vec<TableAlignKind>)> {
    let head = &lines[start];
    if head.mdx {
        return None;
    }
    let delimiter = lines.get(start + 1)?;
    // The delimiter row cannot start another block, such as a list item.
    if head.indent().0 >= 4 || delimiter.indent().0 >= 4 || delimiter.lazy || starts_block(delimiter) {
        return None;
    }
    let text = delimiter.text.trim_matches([' ', '\t']);
    // Without a pipe, a colon tells the delimiter row from the underline of a heading.
    if !text.contains(['|', ':'])
        || !text.contains('-')
        || !text.bytes().all(|b| matches!(b, b'|' | b':' | b'-' | b' ' | b'\t'))
    {
        return None;
    }

    // A lone pipe is both the leading and the trailing one of a row, which then has no cells.
    if head.text.trim_matches([' ', '\t']) == "|" {
        return None;
    }
    let cells = split_row(head);
    let aligns = split_row(delimiter)
        .iter()
        .map(|cell| align(cell.content))
        .collect::<Option<Vec<_>>>()?;
    (cells.len() == aligns.len()).then_some((cells, aligns))
}

/// Whether a table starts at `lines[start]`.
pub(super) fn starts_at(lines: &[Line<'_>], start: usize, starts_block: fn(&Line<'_>) -> bool) -> bool {
    header(lines, start, starts_block).is_some()
}

fn cell_item(line: &Line<'_>, row: usize, column: usize, cell: &Cell<'_>) -> TableItem {
    let source = (!cell.content.is_empty()).then(|| {
        let mut source = InlineSource::new(std::iter::once((cell.content, "", line.point(cell.content_offset))));
        source.table = true;
        source
    });
    TableItem::Cell {
        row,
        column,
        position: Position {
            start: line.point(cell.start),
            end: line.point(cell.end),
        },
        source,
    }
}

/// Parses a table starting at `lines[start]`, returning its nodes and the index after it.
/// `starts_block` tells which lines cannot be the delimiter row, `ends_row` which lines end the table
/// besides blank lines.
pub(super) fn parse(
    lines: &[Line<'_>],
    start: usize,
    starts_block: fn(&Line<'_>) -> bool,
    ends_row: fn(&Line<'_>) -> bool,
) -> Option<(Vec<TableItem>, usize)> {
    let (cells, aligns) = header(lines, start, starts_block)?;

    let mut items = Vec::new();
    for (column, cell) in cells.iter().enumerate() {
        items.push(cell_item(&lines[start], 0, column, cell));
    }
    let delimiter_line = lines[start + 1].number;
    items.push(TableItem::Align {
        align: aligns,
        position: Position {
            start: Point {
                line: delimiter_line,
                column: 1,
            },
            end: Point {
                line: delimiter_line,
                column: 1,
            },
        },
    });

    let mut index = start + 2;
    let mut row = 1;
    while let Some(line) = lines.get(index) {
        if line.is_blank() || line.indent().0 >= 4 || ends_row(line) {
            break;
        }
        for (column, cell) in split_row(line).iter().enumerate() {
            items.push(cell_item(line, row, column, cell));
        }
        row += 1;
        index += 1;
    }

    Some((items, index))
}

/// The number of cells in the row `line`.
pub(super) fn row_cells(line: &Line<'_>) -> usize {
    split_row(line).len()
}

/// The number of cells of `line` when it is a delimiter row such as `|:-|-:|`.
pub(super) fn delimiter_cells(line: &Line<'_>) -> Option<usize> {
    let text = line.text.trim_matches([' ', '\t']);
    // Without a pipe, a colon tells the delimiter row from the underline of a heading.
    if !text.contains(['|', ':'])
        || !text.contains('-')
        || !text.bytes().all(|b| matches!(b, b'|' | b':' | b'-' | b' ' | b'\t'))
    {
        return None;
    }
    let cells = split_row(line);
    cells
        .iter()
        .all(|cell| align(cell.content).is_some())
        .then_some(cells.len())
}
