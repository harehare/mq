//! Source lines and their positions.

use super::block::CODE_INDENT;
use super::flavor::Flavor;
use crate::node::Point;

/// The byte order mark.
pub(super) const BOM: char = '\u{feff}';

/// Leading whitespace: how many columns it spans, where tabs go to the next multiple of four, and how
/// many bytes it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Indent {
    pub(super) columns: usize,
    pub(super) bytes: usize,
}

#[derive(Clone, Copy)]
pub(super) struct Line<'a> {
    pub(super) number: u32,
    /// Byte offset of `text` within the original line, non-zero inside containers.
    pub(super) column: usize,
    pub(super) text: &'a str,
    /// Line terminator that follows `text`; empty on the last line without one.
    pub(super) eol: &'a str,
    /// Whether this is the last line of the document.
    pub(super) eof: bool,
    /// Whether this is the last line of a list item.
    pub(super) item_end: bool,
    /// Whether the line continues a paragraph without its container prefix.
    pub(super) lazy: bool,
    /// The tabs of the line, to compute columns without scanning the line again.
    pub(super) stops: &'a [TabStop],
    /// The whole line without its terminator, whatever a container removed from `text`, to turn byte
    /// offsets into columns.
    pub(super) whole: &'a str,
    /// Byte offset of the line in the document, to compare with `stops`.
    pub(super) base: usize,
    /// Columns left over from a tab that a container consumed only in part. They count as leading
    /// whitespace of `text`.
    pub(super) pad: usize,
    /// How many bytes of `text` are leading spaces and tabs, so that deeply indented lines in nested
    /// containers are not scanned again by every container.
    pub(super) ws: usize,
    /// The dialect of the document.
    pub(super) flavor: Flavor,
    /// For `*`, `-` and `_`: one past the offset in the line of the last byte that is neither that
    /// character, nor a space or tab, or 0 if there is none. A thematic break is a run of one of them
    /// and whitespace to the end of the line, so this tells at once, from any offset, that it is not.
    pub(super) others: [usize; 3],
}

impl<'a> Line<'a> {
    /// The indentation from which a line is code. Without indented code, indentation never matters.
    pub(super) fn code_indent(&self) -> usize {
        if self.flavor.has_indented_code() {
            CODE_INDENT
        } else {
            usize::MAX
        }
    }

    /// The zero-based visual column where `text` starts in the document.
    fn start_column(&self) -> usize {
        self.visual_column(self.column)
    }

    /// The zero-based visual column of the byte at `offset` in the line.
    fn visual_column(&self, offset: usize) -> usize {
        tab_adjusted_column(self.stops, self.base + offset, offset)
    }

    /// The leading whitespace.
    pub(super) fn indent(&self) -> Indent {
        let start = self.start_column();
        let end = self.visual_column(self.column + self.ws);
        Indent {
            columns: self.pad + end - start,
            bytes: self.ws,
        }
    }

    pub(super) fn is_blank(&self) -> bool {
        self.ws == self.text.len()
    }

    /// The position of the byte at `byte` in `text`. Columns are in UTF-16 code units, and a tab is one.
    pub(super) fn point(&self, byte: usize) -> Point {
        let offset = self.column + byte;
        Point {
            line: self.number,
            column: to_u32(offset - extra_bytes(&self.whole.as_bytes()[..offset.min(self.whole.len())]) + 1),
        }
    }

    /// The same line with the first `bytes` bytes removed.
    pub(super) fn skip(self, bytes: usize) -> Line<'a> {
        let text = &self.text[bytes..];
        Line {
            column: self.column + bytes,
            text,
            pad: 0,
            ws: self.ws.checked_sub(bytes).unwrap_or_else(|| leading_ws(text)),
            ..self
        }
    }

    /// The same line with `columns` columns of leading whitespace removed. What is left of a tab that
    /// is only consumed in part stays as padding.
    pub(super) fn skip_columns(self, columns: usize) -> Line<'a> {
        let mut remaining = columns;
        let consumed = remaining.min(self.pad);
        let mut pad = self.pad - consumed;
        remaining -= consumed;

        let mut absolute = self.start_column();
        let mut bytes = 0;
        for byte in self.text.bytes() {
            if remaining == 0 {
                break;
            }
            match byte {
                b' ' => {
                    remaining -= 1;
                    absolute += 1;
                }
                b'\t' => {
                    let width = (absolute / 4 + 1) * 4 - absolute;
                    absolute += width;
                    if width > remaining {
                        pad = width - remaining;
                        remaining = 0;
                    } else {
                        remaining -= width;
                    }
                }
                _ => break,
            }
            bytes += 1;
        }

        Line {
            column: self.column + bytes,
            text: &self.text[bytes..],
            pad,
            ws: self.ws - bytes,
            ..self
        }
    }

    pub(super) fn end(&self) -> Point {
        self.point(self.text.len())
    }
}

/// A tab in the document and the zero-based column right after it.
#[derive(Clone, Copy)]
pub(super) struct TabStop {
    at: usize,
    after: usize,
}

/// The column of the byte at `at` in the document: counted from the last tab before it, or `offset` if none.
/// `stops` is sorted by `at`.
fn tab_adjusted_column(stops: &[TabStop], at: usize, offset: usize) -> usize {
    let passed = stops.partition_point(|stop| stop.at < at);
    match passed.checked_sub(1).map(|index| stops[index]) {
        Some(stop) => stop.after + (at - stop.at - 1),
        None => offset,
    }
}

/// Every tab in `src` in order. A byte order mark at the start has no width.
pub(super) fn tab_stops(src: &str) -> Vec<TabStop> {
    let mut stops = Vec::new();
    // The byte offset and the column right after the last tab or line start.
    let (mut anchor, mut anchor_column) = (if src.starts_with(BOM) { BOM.len_utf8() } else { 0 }, 0);
    for (index, byte) in src.bytes().enumerate() {
        match byte {
            b'\n' | b'\r' => (anchor, anchor_column) = (index + 1, 0),
            b'\t' => {
                let column = anchor_column + (index - anchor);
                let after = (column / 4 + 1) * 4;
                stops.push(TabStop { at: index, after });
                (anchor, anchor_column) = (index + 1, after);
            }
            _ => {}
        }
    }
    stops
}

/// Converts a count to a [`Point`] coordinate, saturating on inputs past `u32::MAX`.
pub(super) fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// How many of `bytes`, which are whole characters, are more than UTF-16 code units: each continuation
/// byte is one, except that a character of four bytes is two code units.
pub(super) fn extra_bytes(bytes: &[u8]) -> usize {
    let (mut continuation, mut astral) = (0, 0);
    for &byte in bytes {
        continuation += usize::from(byte & 0xC0 == 0x80);
        astral += usize::from(byte >= 0xF0);
    }
    continuation - astral
}

/// The number of leading spaces and tabs in `text`.
pub(super) fn leading_ws(text: &str) -> usize {
    text.bytes().take_while(|byte| matches!(byte, b' ' | b'\t')).count()
}

/// The `Line::others` of `text`.
fn last_others(text: &str) -> [usize; 3] {
    let mut last = [0; 3];
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b' ' | b'\t' => {}
            b'*' => (last[1], last[2]) = (index + 1, index + 1),
            b'-' => (last[0], last[2]) = (index + 1, index + 1),
            b'_' => (last[0], last[1]) = (index + 1, index + 1),
            _ => last = [index + 1; 3],
        }
    }
    last
}

/// Splits `src` into lines on `\n`, `\r\n` and `\r`. A trailing terminator does not add a line.
pub(super) fn split_lines<'a>(src: &'a str, flavor: Flavor, stops: &'a [TabStop]) -> Vec<Line<'a>> {
    let bytes = src.as_bytes();
    let mut lines = Vec::new();
    let (mut start, mut index) = (0, 0);
    let mut next_stop = 0;
    // The tabs between `start` and `end`.
    let mut take_stops = |end: usize| {
        let first = next_stop;
        while stops.get(next_stop).is_some_and(|stop| stop.at < end) {
            next_stop += 1;
        }
        &stops[first..next_stop]
    };

    while index < bytes.len() {
        let end = index;
        match bytes[index] {
            b'\n' => index += 1,
            b'\r' => index += if bytes.get(index + 1) == Some(&b'\n') { 2 } else { 1 },
            _ => {
                index += 1;
                continue;
            }
        }
        lines.push(Line {
            number: to_u32(lines.len() + 1),
            column: 0,
            text: &src[start..end],
            eol: &src[end..index],
            eof: index == bytes.len(),
            item_end: false,
            lazy: false,
            stops: take_stops(end),
            whole: &src[start..end],
            base: start,
            pad: 0,
            ws: leading_ws(&src[start..end]),
            flavor,
            others: last_others(&src[start..end]),
        });
        start = index;
    }

    if start < src.len() {
        lines.push(Line {
            number: to_u32(lines.len() + 1),
            column: 0,
            text: &src[start..],
            eol: "",
            eof: true,
            item_end: false,
            lazy: false,
            stops: take_stops(src.len()),
            whole: &src[start..],
            base: start,
            pad: 0,
            ws: leading_ws(&src[start..]),
            flavor,
            others: last_others(&src[start..]),
        });
    }

    lines
}

#[cfg(kani)]
mod proofs {
    use super::*;

    /// A well-formed UTF-8 buffer with two character boundaries `at <= offset`, as `Tree::extra_before` sees them.
    fn text_with_boundaries(bytes: &[u8; 6]) -> (usize, usize) {
        let text = std::str::from_utf8(bytes);
        kani::assume(text.is_ok());
        let text = text.unwrap_or_default();
        let at: usize = kani::any_where(|at: &usize| *at <= bytes.len() && text.is_char_boundary(*at));
        let offset: usize = kani::any_where(|o: &usize| *o >= at && *o <= bytes.len() && text.is_char_boundary(*o));
        (at, offset)
    }

    #[kani::proof]
    #[kani::unwind(8)]
    fn extra_bytes_does_not_underflow_on_whole_characters() {
        let bytes: [u8; 6] = kani::any();
        let (at, offset) = text_with_boundaries(&bytes);
        let _ = extra_bytes(&bytes[at..offset]);
    }

    /// The cursor in `Tree::extra_before` stores `extra_bytes(text[..at])`, so moving it back by
    /// `extra_bytes(text[offset..at])` never underflows.
    #[kani::proof]
    #[kani::unwind(8)]
    fn extra_bytes_is_additive_over_a_split() {
        let bytes: [u8; 6] = kani::any();
        let (at, offset) = text_with_boundaries(&bytes);
        kani::cover!(at > 0 && offset > at && extra_bytes(&bytes[at..offset]) > 0);
        assert_eq!(
            extra_bytes(&bytes[..offset]),
            extra_bytes(&bytes[..at]) + extra_bytes(&bytes[at..offset])
        );
    }

    /// `partition_point` finds the tabs before `at` only if `stops` is sorted, which `tab_stops` gives by
    /// walking `src` in order.
    #[kani::proof]
    #[kani::unwind(5)]
    fn tab_column_does_not_underflow() {
        let stops: [TabStop; 3] = [0, 1, 2].map(|_| TabStop {
            at: kani::any(),
            after: kani::any(),
        });
        kani::assume(stops[0].at < stops[1].at && stops[1].at < stops[2].at);
        // Bounded so that `after + (at - stop.at - 1)` stays within a document's size.
        kani::assume(stops.iter().all(|stop| stop.at < 1 << 32 && stop.after < 1 << 32));
        let (at, offset): (usize, usize) = (kani::any(), kani::any());
        kani::assume(at < 1 << 32 && offset < 1 << 32);
        let column = tab_adjusted_column(&stops, at, offset);
        if let Some(stop) = stops.iter().rev().find(|stop| stop.at < at) {
            assert_eq!(column, stop.after + (at - stop.at - 1));
        } else {
            assert_eq!(column, offset);
        }
    }
}
