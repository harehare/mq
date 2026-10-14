//! HTML blocks.

use super::{Block, Html, HtmlKind, Line, Node, Point, Position, html_flow, join_lines};

/// Collects an HTML block of `kind` starting at `lines[start]`, which is indented less than four columns.
pub(super) fn html_block(lines: &[Line<'_>], start: usize, kind: HtmlKind, blocks: &mut Vec<Block>) -> usize {
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
pub(super) fn html_start(line: &Line<'_>, rest: &str) -> Option<HtmlKind> {
    line.flavor.has_html().then(|| html_flow::start(rest)).flatten()
}
