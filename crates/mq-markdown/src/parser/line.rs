//! Source lines and their positions.

use crate::node::Point;

#[derive(Clone, Copy)]
pub(super) struct Line<'a> {
    pub(super) number: usize,
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
    /// The whole line as in the document, to compute columns when it contains tabs.
    pub(super) origin: &'a str,
    /// Whether `origin` contains a tab.
    pub(super) tabs: bool,
    /// Columns left over from a tab that a container consumed only in part. They count as leading
    /// whitespace of `text`.
    pub(super) pad: usize,
    /// Whether the document is MDX, which has no indented code, HTML, autolinks or GFM.
    pub(super) mdx: bool,
}

impl<'a> Line<'a> {
    /// The indentation from which a line is code. Without indented code, indentation never matters.
    pub(super) fn code_indent(&self) -> usize {
        if self.mdx { usize::MAX } else { 4 }
    }

    /// The zero-based visual column where `text` starts in the document.
    fn start_column(&self) -> usize {
        if self.tabs {
            visual_column(self.origin[..self.column].chars(), 0)
        } else {
            self.column
        }
    }

    /// Column count and byte length of the leading whitespace.
    pub(super) fn indent(&self) -> (usize, usize) {
        let mut columns = self.pad;
        let mut absolute = self.start_column();
        for (index, byte) in self.text.bytes().enumerate() {
            match byte {
                b' ' => {
                    columns += 1;
                    absolute += 1;
                }
                b'\t' => {
                    let width = (absolute / 4 + 1) * 4 - absolute;
                    columns += width;
                    absolute += width;
                }
                _ => return (columns, index),
            }
        }
        (columns, self.text.len())
    }

    pub(super) fn is_blank(&self) -> bool {
        self.text.bytes().all(|b| matches!(b, b' ' | b'\t'))
    }

    /// The position of the byte at `byte` in `text`. Columns are in bytes, except that a tab advances
    /// to the next multiple of four, like `markdown-rs` counts them.
    pub(super) fn point(&self, byte: usize) -> Point {
        let offset = self.column + byte;
        let mut column = if self.tabs {
            visual_column(self.origin[..offset].chars(), 0) + 1
        } else {
            offset + 1
        };
        // Whitespace at the start of a line inside a tab starts before the end of that tab, content
        // starts where that tab ends.
        if byte == 0 {
            column -= self.pad;
        }
        Point {
            line: self.number,
            column,
        }
    }

    /// Like [`Line::point`] for content that `markdown-rs` places where the rest of a tab ends, instead
    /// of before it: paragraphs and fences.
    pub(super) fn content_point(&self, byte: usize) -> Point {
        let mut point = self.point(byte);
        if byte == 0 {
            point.column += self.pad;
        }
        point
    }

    /// The same line with the first `bytes` bytes removed.
    pub(super) fn skip(self, bytes: usize) -> Line<'a> {
        Line {
            column: self.column + bytes,
            text: &self.text[bytes..],
            pad: 0,
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
            ..self
        }
    }

    pub(super) fn end(&self) -> Point {
        self.point(self.text.len())
    }
}

/// The zero-based column reached after `chars`, starting at column `start`, where tabs go to the next
/// multiple of four and other characters advance by their length in bytes.
pub(super) fn visual_column(chars: impl Iterator<Item = char>, start: usize) -> usize {
    chars.fold(start, |column, char| {
        if char == '\t' {
            (column / 4 + 1) * 4
        } else {
            column + char.len_utf8()
        }
    })
}

/// Splits `src` into lines on `\n`, `\r\n` and `\r`. A trailing terminator does not add a line.
pub(super) fn split_lines(src: &str, mdx: bool) -> Vec<Line<'_>> {
    let bytes = src.as_bytes();
    let mut lines = Vec::new();
    let (mut start, mut index) = (0, 0);

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
            number: lines.len() + 1,
            column: 0,
            text: &src[start..end],
            eol: &src[end..index],
            eof: index == bytes.len(),
            item_end: false,
            lazy: false,
            origin: &src[start..end],
            tabs: src[start..end].contains('\t'),
            pad: 0,
            mdx,
        });
        start = index;
    }

    if start < src.len() {
        lines.push(Line {
            number: lines.len() + 1,
            column: 0,
            text: &src[start..],
            eol: "",
            eof: true,
            item_end: false,
            lazy: false,
            origin: &src[start..],
            tabs: src[start..].contains('\t'),
            pad: 0,
            mdx,
        });
    }

    lines
}
