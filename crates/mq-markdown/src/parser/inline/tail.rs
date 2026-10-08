//! The parts that follow the text of a link: destinations, titles and reference labels.

use super::Context;
use super::entity;
use super::link::normalize;

/// The destination and title of an inline link, and the offset after its closing parenthesis.
pub(super) struct Tail {
    pub(super) url: String,
    pub(super) title: Option<String>,
    pub(super) end: usize,
}

/// Resolves a reference whose text is `text` and whose closing bracket is followed by `after`.
/// Returns the label, the offset after the reference, and whether the label is the text itself.
pub(super) fn reference(context: &Context<'_>, text: &str, after: usize) -> Option<(String, usize, bool)> {
    let src = context.src();
    // A label has at most 999 characters, which also keeps the normalizing of long text away.
    let defined =
        |label: &str| label.chars().nth(999).is_none() && context.references.definitions.contains(&normalize(label));

    if src[after..].starts_with('[') {
        match parse_label(src, after) {
            // A full reference: `[text][label]`.
            Some((end, label)) if !label.is_empty() => {
                return defined(label).then(|| (label.to_string(), end, false));
            }
            // A collapsed reference: `[text][]`.
            Some((end, _)) => {
                return (has_content(text) && defined(text)).then(|| (text.to_string(), end, true));
            }
            None => {}
        }
    }

    // A shortcut reference: `[text]`.
    (has_content(text) && defined(text)).then(|| (text.to_string(), after, true))
}

fn has_content(label: &str) -> bool {
    label.bytes().any(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
}

/// Parses a link label at `pos` (the `[`), returning the offset after the `]` and the raw label.
/// An empty label is returned for `[]`.
fn parse_label(src: &str, pos: usize) -> Option<(usize, &str)> {
    let bytes = src.as_bytes();
    let mut index = pos + 1;
    while index < bytes.len() && index - pos <= 1000 {
        match bytes[index] {
            b'\\' => index += 1,
            b'[' => return None,
            b']' => {
                let label = &src[pos + 1..index];
                return (label.is_empty() || has_content(label)).then_some((index + 1, label));
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Skips whitespace including at most one line ending.
fn skip_space(bytes: &[u8], mut index: usize) -> usize {
    let mut line_endings = 0;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b' ' | b'\t' => index += 1,
            b'\n' | b'\r' => {
                line_endings += 1;
                if line_endings > 1 {
                    break;
                }
                index += super::eol_len(&bytes[index..]);
            }
            _ => break,
        }
    }
    index
}

/// Parses `(destination "title")` at `pos`.
pub(super) fn inline_tail(src: &str, pos: usize) -> Option<Tail> {
    let bytes = src.as_bytes();
    if bytes.get(pos) != Some(&b'(') {
        return None;
    }

    let index = skip_space(bytes, pos + 1);
    let (url, after_url) = destination(src, index)?;

    let after_space = skip_space(bytes, after_url);
    if bytes.get(after_space) == Some(&b')') {
        return Some(Tail {
            url,
            title: None,
            end: after_space + 1,
        });
    }
    // A title needs whitespace before it.
    if after_space == after_url {
        return None;
    }
    let (title, after_title) = title_at(src, after_space)?;

    let closing = skip_space(bytes, after_title);
    (bytes.get(closing) == Some(&b')')).then_some(Tail {
        url,
        title: Some(title),
        end: closing + 1,
    })
}

/// Parses a link destination at `pos`, returning it decoded and the offset after it.
pub(in crate::parser) fn destination(src: &str, pos: usize) -> Option<(String, usize)> {
    let bytes = src.as_bytes();

    if bytes.get(pos) == Some(&b'<') {
        let mut index = pos + 1;
        while let Some(&byte) = bytes.get(index) {
            match byte {
                b'>' => return Some((entity::unescape(&src[pos + 1..index]), index + 1)),
                b'<' | b'\n' | b'\r' => return None,
                b'\\' => index += 1,
                _ => {}
            }
            index += 1;
        }
        return None;
    }

    let mut index = pos;
    let mut depth = 0usize;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'\\' if bytes.get(index + 1).is_some_and(u8::is_ascii_punctuation) => index += 1,
            b'(' => {
                depth += 1;
                if depth > 32 {
                    return None;
                }
            }
            b')' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            b' ' | b'\t' | b'\n' | b'\r' => break,
            // markdown-rs lets a NUL through, though it is a control character.
            byte if byte.is_ascii_control() && byte != 0 => break,
            _ => {}
        }
        index += 1;
    }
    (depth == 0).then(|| (entity::unescape(&src[pos..index]), index))
}

/// Parses a link title at `pos`, returning it decoded and the offset after its closing delimiter.
pub(in crate::parser) fn title_at(src: &str, pos: usize) -> Option<(String, usize)> {
    let bytes = src.as_bytes();
    let close = match bytes.get(pos)? {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };

    let mut index = pos + 1;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            byte if byte == close => {
                // The lines of a paragraph lose their leading whitespace, also inside a title.
                let title = entity::remove_line_indent(&src[pos + 1..index]);
                return Some((entity::unescape(&title), index + 1));
            }
            b'(' if close == b')' => return None,
            b'\\' => index += 1,
            b'\n' | b'\r' => {
                // A title cannot contain a blank line.
                let length = super::eol_len(&bytes[index..]);
                let mut next = index + length;
                while matches!(bytes.get(next), Some(b' ' | b'\t')) {
                    next += 1;
                }
                if matches!(bytes.get(next), Some(b'\n' | b'\r')) {
                    return None;
                }
                index += length - 1;
            }
            _ => {}
        }
        index += 1;
    }
    None
}
