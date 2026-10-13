//! Footnote definitions.

use super::{Block, Collector, FootnoteBlock, Indent, Interrupt, Line, Position, inline, parse_blocks};

/// The indentation of the lines that continue a footnote definition.
pub(super) const FOOTNOTE_INDENT: usize = 4;

/// The start of a footnote definition: `[^label]:`.
pub(super) struct FootnoteMarker<'a> {
    label: &'a str,
    /// Bytes from the start of the line to the content.
    pub(super) content: usize,
}

pub(super) fn footnote_marker<'a>(line: &Line<'a>) -> Option<FootnoteMarker<'a>> {
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
pub(super) fn footnote(
    lines: &[Line<'_>],
    start: usize,
    marker: &FootnoteMarker<'_>,
    depth: usize,
    interrupting: Interrupt,
    blocks: &mut Vec<Block>,
) -> usize {
    let mut collector = Collector::new(depth, interrupting);
    collector.push(lines[start].skip(marker.content));
    let mut index = start + 1;

    while let Some(line) = lines.get(index) {
        if line.is_blank() {
            collector.blank(line.skip(line.text.len()));
        } else if line.indent().columns >= FOOTNOTE_INDENT {
            collector.push(line.skip_columns(FOOTNOTE_INDENT));
        } else if !collector.lazy(lines, index) {
            break;
        }
        index += 1;
    }
    let Collector {
        lines: inner, blanks, ..
    } = collector;

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
