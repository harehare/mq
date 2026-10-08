//! Fenced and indented code, and math fences.

use super::block::CODE_INDENT;
use super::inline;
use super::line::Line;
use super::tree::{Block, FencedBlock};
use crate::node::{Code, Math, Node, Point, Position};

/// An opening code fence, or math fence.
pub(super) struct Fence<'a> {
    marker: u8,
    length: usize,
    /// Info string after the fence, trimmed.
    info: &'a str,
}

impl<'a> Fence<'a> {
    /// Parses `rest` (indent already removed) as an opening fence.
    pub(super) fn open(rest: &'a str, math: bool) -> Option<Self> {
        let marker = *rest
            .as_bytes()
            .first()
            .filter(|b| matches!(b, b'`' | b'~') || (math && **b == b'$'))?;
        let length = rest.bytes().take_while(|&b| b == marker).count();
        let info = rest[length..].trim_matches([' ', '\t']);
        // Math fences are two dollars or more, and the info string cannot hold the marker of math or
        // backtick fences.
        let long_enough = length >= if marker == b'$' { 2 } else { 3 };
        let info_ok = !(matches!(marker, b'`' | b'$') && info.as_bytes().contains(&marker));
        (long_enough && info_ok).then_some(Self { marker, length, info })
    }

    /// Whether `rest` (indent already removed) closes this fence.
    pub(super) fn is_closed_by(&self, rest: &str) -> bool {
        let length = rest.bytes().take_while(|&b| b == self.marker).count();
        length >= self.length && rest[length..].trim_matches([' ', '\t']).is_empty()
    }
}

pub(super) fn fenced_code(
    lines: &[Line<'_>],
    start: usize,
    indent: usize,
    fence: &Fence<'_>,
    own_end: bool,
    blocks: &mut Vec<Block>,
) -> usize {
    let (lang, meta) = match fence.info.split_once([' ', '\t']) {
        Some((lang, meta)) => (Some(lang), Some(meta.trim_start_matches([' ', '\t']))),
        None => (Some(fence.info).filter(|s| !s.is_empty()), None),
    };

    let mut body = Vec::new();
    let mut index = start + 1;
    let mut end = None;

    while let Some(line) = lines.get(index) {
        let (columns, line_indent) = line.indent();
        if columns < line.code_indent() && fence.is_closed_by(&line.text[line_indent..]) {
            end = Some(line.end());
            index += 1;
            break;
        }
        // Remove up to the opening fence's indentation from each content line. What is left of a tab
        // that is only consumed in part stays as spaces.
        let content = line.skip_columns(lines[start].pad + indent);
        body.push((content.pad, content.text, line.eol));
        index += 1;
    }

    // An unclosed fence runs to the end of its container. When that is the end of the document the
    // final line terminator is part of it, so the end is the start of the following line.
    // Right after a container, a fence without content ends with its own line instead.
    let closed = end.is_some();
    let end = end.unwrap_or_else(|| match lines.last() {
        Some(last) if !last.eol.is_empty() && reaches_line_end(last, body.is_empty(), own_end) => Point {
            line: last.number + 1,
            column: 1,
        },
        Some(last) => last.end(),
        None => lines[start].end(),
    });

    let position = Some(Position {
        start: lines[start].content_point(indent),
        end,
    });
    let mut value = String::with_capacity(body.iter().map(|(pad, text, eol)| pad + text.len() + eol.len()).sum());
    for (index, (pad, text, eol)) in body.iter().enumerate() {
        value.extend(std::iter::repeat_n(' ', *pad));
        value.push_str(text);
        if index + 1 < body.len() {
            value.push_str(eol);
        }
    }
    let node = if fence.marker == b'$' {
        Node::Math(Math { value, position })
    } else {
        Node::Code(Code {
            value,
            lang: lang.map(inline::unescape),
            meta: meta.filter(|m| !m.is_empty()).map(inline::unescape),
            fence: true,
            position,
        })
    };
    blocks.push(Block::Fenced(FencedBlock {
        node,
        closed,
        lines: body.len(),
    }));

    index
}

/// Whether an unclosed fence whose last line is `last` includes that line's terminator.
fn reaches_line_end(last: &Line<'_>, empty: bool, own_end: bool) -> bool {
    if empty {
        last.eof && !own_end
    } else {
        last.eof || last.item_end
    }
}

pub(super) fn indented_code(lines: &[Line<'_>], start: usize, blocks: &mut Vec<Block>) -> usize {
    let mut last_code = start;
    let mut index = start;

    while let Some(line) = lines.get(index) {
        // Whitespace-only lines are code when indented enough, otherwise they may just separate chunks.
        if line.indent().0 >= CODE_INDENT {
            last_code = index;
        } else if !line.is_blank() {
            break;
        }
        index += 1;
    }

    let mut parts = lines[start..=last_code]
        .iter()
        .map(|line| {
            let content = line.skip_columns(CODE_INDENT);
            (content.pad, content.text, line.eol)
        })
        .collect::<Vec<_>>();
    // Trailing lines of whitespace only extend the position, not the value.
    while parts.len() > 1
        && parts
            .last()
            .is_some_and(|(_, text, _)| text.trim_matches([' ', '\t']).is_empty())
    {
        parts.pop();
    }
    // What is left of a tab that the indent consumes in part is code, as spaces.
    let mut value = String::with_capacity(parts.iter().map(|(pad, text, eol)| pad + text.len() + eol.len()).sum());
    for (index, (pad, text, eol)) in parts.iter().enumerate() {
        value.extend(std::iter::repeat_n(' ', *pad));
        value.push_str(text);
        if index + 1 < parts.len() {
            value.push_str(eol);
        }
    }

    let position = Position {
        start: lines[start].point(0),
        end: lines[last_code].end(),
    };
    // Mirrors the `mdast` conversion: a value whose line count differs from the span is reported as fenced.
    let fence = value.lines().count() != position.end.line - position.start.line + 1;

    blocks.push(Block::Node(Node::Code(Code {
        value,
        lang: None,
        meta: None,
        fence,
        position: Some(position),
    })));

    last_code + 1
}
