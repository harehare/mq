//! Link reference definitions: `[label]: destination "title"` at the start of a paragraph.

use super::inline::{destination, normalize, remove_line_indent, title_at, unescape};
use super::scan::{eol_len, skip_blanks, skip_blanks_and_eol};
use super::tree::InlineSource;
use crate::node::{Definition, Node, Position, Title, Url};

/// Parses the label at `pos` (a `[`) followed by `:`, returning the raw label and the offset after.
fn label(text: &str, pos: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let mut index = pos + 1;
    while index < bytes.len() && index - pos <= 1000 {
        match bytes[index] {
            b'\\' => index += 1,
            b'[' => return None,
            b']' => {
                let label = &text[pos + 1..index];
                let has_content = label.bytes().any(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'));
                return (has_content && bytes.get(index + 1) == Some(&b':')).then_some((label, index + 2));
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// A definition and where it ends.
struct Parsed {
    label: String,
    ident: String,
    url: String,
    title: Option<String>,
    /// End of the last token of the definition.
    end: usize,
    /// Offset of the next line, after the line ending.
    next: usize,
}

fn parse_one(text: &str, pos: usize) -> Option<Parsed> {
    let bytes = text.as_bytes();
    let (raw_label, after_colon) = label(text, pos)?;

    let dest_start = skip_blanks_and_eol(bytes, after_colon);
    let (url, after_dest) = destination(text, dest_start)?;
    // A bare destination cannot be empty.
    if after_dest == dest_start {
        return None;
    }

    // A title on the same line or on the next one, with nothing after it on its line.
    let title = title_start(bytes, after_dest).and_then(|start| {
        let (title, after_title) = title_at(text, start)?;
        let after_blanks = skip_blanks(bytes, after_title);
        let eol = eol_len(bytes, after_blanks);
        (eol > 0 || after_blanks == bytes.len()).then_some((title, after_title, after_blanks + eol))
    });

    let (title, end, next) = match title {
        Some((title, end, next)) => (Some(title), end, next),
        None => {
            let after_blanks = skip_blanks(bytes, after_dest);
            let eol = eol_len(bytes, after_blanks);
            if eol == 0 && after_blanks != bytes.len() {
                return None;
            }
            (None, after_dest, after_blanks + eol)
        }
    };

    Some(Parsed {
        label: unescape(&remove_line_indent(raw_label)),
        ident: normalize(raw_label),
        url,
        title,
        end,
        next,
    })
}

/// The start of a title after the destination: whitespace is required before it.
fn title_start(bytes: &[u8], after_dest: usize) -> Option<usize> {
    let start = skip_blanks_and_eol(bytes, after_dest);
    (start > after_dest).then_some(start)
}

/// Parses the definitions at the start of a paragraph. `lines` holds the span of each line, from the
/// start of its indentation. Returns the definitions and how many lines they use.
pub(super) fn extract(source: &InlineSource, lines: &[Position]) -> (Vec<Node>, usize) {
    let text = source.text.as_str();
    let mut nodes = Vec::new();
    let mut pos = 0;
    let mut next_line = 0;

    while pos < text.len() && text.as_bytes()[pos] == b'[' {
        let Some(parsed) = parse_one(text, pos) else {
            break;
        };
        let first = source.line_index(pos);
        // The definition ends with its last line, trailing whitespace included.
        let last = source.line_index(parsed.end);
        nodes.push(Node::Definition(Definition {
            position: Some(Position {
                start: lines[first].start.clone(),
                end: lines[last].end.clone(),
            }),
            url: Url(parsed.url),
            title: parsed.title.map(Title),
            ident: parsed.ident,
            label: Some(parsed.label),
        }));
        next_line = parsed.next;
        pos = skip_blanks(text.as_bytes(), parsed.next);
    }

    // Every line that starts before the end of the definitions is used.
    let used = source.lines.partition_point(|line| line.offset < next_line);
    (nodes, used)
}

/// Whether `text`, the lines of a paragraph, is nothing but definitions.
pub(super) fn only_definitions(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut pos = skip_blanks_and_eol(bytes, 0);
    while pos < text.len() && bytes[pos] == b'[' {
        let Some(parsed) = parse_one(text, pos) else {
            return false;
        };
        pos = skip_blanks_and_eol(bytes, parsed.next);
    }
    pos >= text.len()
}
