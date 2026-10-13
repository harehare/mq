//! The block tree produced by block parsing, before inline content is parsed.
//!
//! Inline parsing needs to know every definition of the document, including those that come later, so
//! it runs in [`super::resolve`] once all blocks are known.

use super::error::MdxError;
use super::line::extra_bytes;
use super::mdx::TagKind;
use super::scan::eol_len;
use crate::node::{HeadingDepth, ListMarker, MdxAttributeContent, Node, Point, Position, TableAlignKind};

/// Where a line of an [`InlineSource`] starts: its offset in the text and its position in the document.
#[derive(Clone)]
pub(super) struct LineStart {
    pub(super) offset: usize,
    pub(super) point: Point,
}

/// Raw inline content of a paragraph, heading or table cell, with the source position of each line.
#[derive(Clone)]
pub(super) struct InlineSource {
    pub(super) text: String,
    /// Where each line starts, in `text` and in the document.
    pub(super) lines: Vec<LineStart>,
    /// Whether this is the content of a table cell, where `\|` in code stands for `|`.
    pub(super) table: bool,
    /// For each line, how many bytes of `text` before it are more than code units. Empty for ASCII.
    line_extra: Vec<usize>,
    /// An offset in `text` and how many bytes before it are more than code units: positions are mostly
    /// asked for in order, or near each other.
    cursor: std::cell::Cell<(usize, usize)>,
}

impl InlineSource {
    /// Joins `(text, eol, start)` lines, keeping the terminators between them.
    pub(super) fn new<'a>(lines: impl ExactSizeIterator<Item = (&'a str, &'a str, Point)>) -> Self {
        let count = lines.len();
        let mut text = String::new();
        let mut starts = Vec::with_capacity(count);
        for (index, (line, eol, start)) in lines.enumerate() {
            starts.push(LineStart {
                offset: text.len(),
                point: start,
            });
            text.push_str(line);
            if index + 1 < count {
                text.push_str(eol);
            }
        }
        let mut source = Self {
            text,
            lines: starts,
            table: false,
            line_extra: Vec::new(),
            cursor: std::cell::Cell::new((0, 0)),
        };
        source.index_lines();
        source
    }

    /// Finds how far each line is in code units from its offset in bytes.
    fn index_lines(&mut self) {
        self.line_extra.clear();
        self.cursor.set((0, 0));
        if self.text.is_ascii() {
            return;
        }
        let mut extra = 0;
        let mut from = 0;
        for line in &self.lines {
            extra += extra_bytes(&self.text.as_bytes()[from..line.offset]);
            from = line.offset;
            self.line_extra.push(extra);
        }
    }

    /// How many bytes of `text` before `offset` are more than UTF-16 code units.
    fn extra_before(&self, offset: usize) -> usize {
        let bytes = self.text.as_bytes();
        let (at, extra) = self.cursor.get();
        let extra = if offset >= at {
            extra + extra_bytes(&bytes[at..offset])
        } else {
            extra - extra_bytes(&bytes[offset..at])
        };
        self.cursor.set((offset, extra));
        extra
    }

    /// Removes the first `marker` bytes and the one space, tab or line ending that follows them, and the
    /// line ending after a space or a tab.
    pub(super) fn remove_prefix_and_one(&mut self, marker: usize) {
        let bytes = self.text.as_bytes();
        let one = if matches!(bytes.get(marker), Some(b' ' | b'\t')) {
            1 + eol_len(bytes, marker + 1)
        } else {
            eol_len(bytes, marker)
        };
        self.drain_to(marker + one);
    }

    /// Removes the first line with its line ending and returns it without the line ending.
    #[cfg(feature = "callout")]
    pub(super) fn take_first_line(&mut self) -> String {
        let Some(eol) = self.text.find(['\n', '\r']) else {
            self.lines.clear();
            return std::mem::take(&mut self.text);
        };
        let line = self.text[..eol].to_string();
        let end = eol + eol_len(self.text.as_bytes(), eol);
        self.drain_to(end);
        line
    }

    /// Removes the first `end` bytes, keeping the position of every line that is left.
    fn drain_to(&mut self, end: usize) {
        let new_point = self.point(end);
        self.text.replace_range(..end, "");

        let index = self.line_index(end);
        let first = LineStart {
            offset: 0,
            point: Point {
                line: self.lines[index].point.line,
                column: new_point.column,
            },
        };
        let rest = self.lines[index + 1..].iter().map(|line| LineStart {
            offset: line.offset - end,
            point: line.point.clone(),
        });
        self.lines = std::iter::once(first).chain(rest).collect();
        self.index_lines();
    }

    /// The index of the line that `offset` is on.
    pub(super) fn line_index(&self, offset: usize) -> usize {
        self.lines
            .partition_point(|line| line.offset <= offset)
            .saturating_sub(1)
    }

    /// The position where a node ends at `offset`. After a line ending that is the start of the next
    /// line in the document, not the start of its content inside containers.
    pub(super) fn end_point(&self, offset: usize) -> Point {
        let line = &self.lines[self.line_index(offset)];
        if line.offset == offset && offset > 0 {
            return Point {
                line: line.point.line,
                column: 1,
            };
        }
        self.point(offset)
    }

    /// The position of the byte at `offset` in `text`.
    pub(super) fn point(&self, offset: usize) -> Point {
        let index = self.line_index(offset);
        let LineStart { offset: start, point } = &self.lines[index];
        let extra = match self.line_extra.get(index) {
            Some(before) => self.extra_before(offset) - before,
            None => 0,
        };
        Point {
            line: point.line,
            column: point.column + (offset - start) - extra,
        }
    }
}

pub(super) enum Block {
    /// A finished leaf such as a code block or a thematic break.
    Node(Node),
    /// A fenced code or math block.
    Fenced(FencedBlock),
    Inline(InlineBlock),
    Quote(QuoteBlock),
    List(ListBlock),
    Footnote(FootnoteBlock),
    Table(Vec<TableItem>),
    /// A JSX tag on a line of its own in MDX. Tags are paired when the blocks become nodes.
    Jsx(JsxTag),
    /// Invalid MDX, reported when the blocks become nodes.
    Error(MdxError),
}

pub(super) struct FencedBlock {
    pub(super) node: Node,
    /// Whether it has a closing fence.
    pub(super) closed: bool,
    /// The number of lines of content, blank ones included.
    pub(super) lines: usize,
}

#[derive(Clone)]
pub(super) struct InlineBlock {
    pub(super) source: InlineSource,
    pub(super) kind: InlineKind,
}

#[derive(Clone)]
pub(super) enum InlineKind {
    Paragraph,
    Heading { depth: HeadingDepth, position: Position },
}

pub(super) struct QuoteBlock {
    pub(super) children: Vec<Block>,
    pub(super) position: Position,
}

pub(super) struct FootnoteBlock {
    /// The normalized label.
    pub(super) ident: String,
    pub(super) children: Vec<Block>,
    pub(super) position: Position,
}

pub(super) struct ListBlock {
    pub(super) ordered: bool,
    pub(super) start: Option<u32>,
    pub(super) marker: ListMarker,
    pub(super) spread: bool,
    pub(super) items: Vec<Item>,
}

pub(super) struct Item {
    pub(super) checked: Option<bool>,
    pub(super) children: Vec<Block>,
    pub(super) position: Position,
    /// Where the last line of the item ends, before the blank lines that `position` runs over.
    pub(super) content_end: Point,
}

pub(super) enum TableItem {
    Cell {
        row: usize,
        column: usize,
        position: Position,
        source: Option<InlineSource>,
    },
    Align {
        align: Vec<TableAlignKind>,
        position: Position,
    },
}

pub(super) struct JsxTag {
    pub(super) name: Option<String>,
    pub(super) attributes: Vec<MdxAttributeContent>,
    pub(super) kind: TagKind,
    pub(super) position: Position,
}
