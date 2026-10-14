//! GFM autolink literals: `www.example.com`, `https://example.com` and `user@example.com`.
//!
//! A link runs to the first whitespace or `<`, without the punctuation, character references and
//! unbalanced `)` at its end.

use super::punctuation::is_punctuation;
use super::{Item, Scanner, Value};
use crate::node::{Link, Node, Text, Url};
use crate::parser::scan::Span;

/// Whether `char` is a symbol of the emoji blocks, which a domain may contain.
fn is_emoji(char: char) -> bool {
    matches!(char, '\u{2600}'..='\u{27BF}' | '\u{1F300}'..='\u{1FAFF}')
}

/// Whether `char` can be in a domain besides the separators `.`, `_` and `-`.
fn is_domain_char(char: char) -> bool {
    is_emoji(char) || !(char.is_whitespace() || is_punctuation(char))
}

/// Where a `]` ends the link: at the end of the text, before whitespace, or before a `(` or `[`
/// that continues a Markdown link.
fn closes_label(bytes: &[u8], index: usize) -> bool {
    bytes[index] == b']' && matches!(bytes.get(index + 1), None | Some(b'\t' | b'\n' | b' ' | b'(' | b'['))
}

/// The end of the autolink whose domain starts at `start`, with the trailing punctuation left out.
fn autolink_end(src: &str, start: usize) -> usize {
    let bytes = src.as_bytes();
    let mut end = start;
    for (offset, char) in src[start..].char_indices() {
        if char.is_whitespace() || char == '<' || closes_label(bytes, start + offset) {
            break;
        }
        end = start + offset + char.len_utf8();
    }

    // The trailing punctuation starts where only punctuation is left up to the end.
    let mut trailing = end;
    while trailing > start {
        match bytes[trailing - 1] {
            b'!' | b'"' | b'\'' | b')' | b'*' | b',' | b'.' | b':' | b'?' | b']' | b'_' | b'~' => trailing -= 1,
            b';' => {
                // A character reference such as `&amp;` goes as a whole.
                let letters = bytes[start..trailing - 1]
                    .iter()
                    .rev()
                    .take_while(|b| b.is_ascii_alphabetic())
                    .count();
                let reference = trailing - 1 - letters;
                trailing = if letters > 0 && reference > start && bytes[reference - 1] == b'&' {
                    reference - 1
                } else {
                    trailing - 1
                };
            }
            _ => break,
        }
    }

    // A `)` that closes a `(` of the link is part of it.
    let open = bytes[start..trailing].iter().filter(|b| **b == b'(').count();
    let mut closed = bytes[start..trailing].iter().filter(|b| **b == b')').count();
    while trailing < end && bytes[trailing] == b')' && closed < open {
        closed += 1;
        trailing += 1;
    }
    trailing
}

/// Whether the domain at the start of `text` is valid: it has a character besides the separators,
/// and no `_` in its last two segments.
fn has_valid_domain(text: &str) -> bool {
    let length = text
        .find(|char: char| !(matches!(char, '.' | '_' | '-') || is_domain_char(char)))
        .unwrap_or(text.len());
    let domain = &text[..length];
    let underscore_free = domain.rsplit('.').take(2).all(|segment| !segment.contains('_'));
    underscore_free && domain.chars().any(is_domain_char)
}

/// Matches `http://` or `https://` followed by a domain and a path.
fn protocol(src: &str, pos: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    if pos > 0 && bytes[pos - 1].is_ascii_alphabetic() {
        return None;
    }
    let letters = bytes[pos..]
        .iter()
        .take(5)
        .take_while(|b| b.is_ascii_alphabetic())
        .count();
    let name = src[pos..pos + letters].to_ascii_lowercase();
    if !matches!(name.as_str(), "http" | "https") || !src[pos + letters..].starts_with("://") {
        return None;
    }
    let start = pos + letters + 3;
    let end = autolink_end(src, start);
    has_valid_domain(&src[start..end]).then_some(end)
}

/// Matches `www.` followed by a domain and a path.
fn www(src: &str, pos: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    if pos > 0
        && !matches!(
            bytes[pos - 1],
            b'\t' | b'\n' | b' ' | b'(' | b'*' | b'_' | b'[' | b']' | b'~'
        )
    {
        return None;
    }
    let prefix = bytes.get(pos..pos + 4)?;
    if !prefix[..3].eq_ignore_ascii_case(b"www") || prefix[3] != b'.' || pos + 4 >= bytes.len() {
        return None;
    }
    let end = autolink_end(src, pos);
    has_valid_domain(&src[pos..end]).then_some(end)
}

/// Handles `h` or `w` at the scanner position: a protocol or `www.` autolink.
pub(super) fn url(scanner: &mut Scanner<'_>) {
    let context = scanner.context;
    let src = context.src();
    let pos = scanner.pos;
    let is_www = matches!(src.as_bytes()[pos], b'w' | b'W');
    let end = if is_www { www(src, pos) } else { protocol(src, pos) };

    let Some(end) = end.filter(|&end| end > pos) else {
        scanner.pos += 1;
        return;
    };
    let text = &src[pos..end];
    let position = Some(context.position(pos, end));
    let node = Node::Link(Link {
        url: Url(if is_www {
            format!("http://{text}")
        } else {
            text.to_string()
        }),
        title: None,
        values: vec![Node::Text(Text {
            value: text.to_string(),
            position: position.clone(),
        })],
        position,
    });
    scanner.push_node(node, end);
}

/// The start of the local part of an email address that ends at `at`, not before `min`.
fn local_start(bytes: &[u8], min: usize, at: usize) -> Option<usize> {
    let mut index = at;
    while index > min && matches!(bytes[index - 1], b'+' | b'-' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'_' | b'a'..=b'z')
    {
        index -= 1;
    }
    (index != at).then_some(index)
}

/// The end of the domain of an email address that starts at `start`.
fn email_domain(bytes: &[u8], start: usize, xmpp: bool) -> Option<usize> {
    let mut index = start;
    let mut dot = false;

    while index < bytes.len() {
        match bytes[index] {
            b'-' | b'0'..=b'9' | b'A'..=b'Z' | b'_' | b'a'..=b'z' => {}
            b'/' if xmpp => {}
            b'.' if bytes.get(index + 1).is_some_and(u8::is_ascii_alphanumeric) => dot = true,
            _ => break,
        }
        index += 1;
    }

    (index > start && dot && matches!(bytes[index - 1], b'.' | b'A'..=b'Z' | b'a'..=b'z')).then_some(index)
}

/// Appends the plain text `start..end` to `out`, turning email addresses in it into links.
fn split_emails(src: &str, start: usize, end: usize, out: &mut Vec<Item>) {
    let bytes = src.as_bytes();
    let mut emitted = start;
    let mut min = start;
    let mut index = start;

    while index < end {
        if bytes[index] == b'@'
            && let Some(mut from) = local_start(bytes, min, index)
        {
            // `mailto:` and `xmpp:` are part of the link and are not prefixed again.
            let (mut prefixed, mut xmpp) = (true, false);
            if from > min && bytes[from - 1] == b':' {
                let mut word = from - 1;
                while word > min && bytes[word - 1].is_ascii_alphanumeric() {
                    word -= 1;
                }
                let name = src[word..from - 1].to_ascii_lowercase();
                if name == "xmpp" || name == "mailto" {
                    (from, prefixed, xmpp) = (word, false, name == "xmpp");
                }
            }
            if let Some(to) = email_domain(&bytes[..end], index + 1, xmpp).filter(|&to| bytes.get(to) != Some(&b'@')) {
                if emitted < from {
                    out.push(Item::Text {
                        start: emitted,
                        end: from,
                        value: Value::Slice(Span::new(emitted, from)),
                    });
                }
                out.push(Item::Email {
                    start: from,
                    end: to,
                    prefixed,
                });
                emitted = to;
                min = to;
                index = to;
                continue;
            }
        }
        index += 1;
    }

    if emitted < end {
        out.push(Item::Text {
            start: emitted,
            end,
            value: Value::Slice(Span::new(emitted, end)),
        });
    }
}

/// The source range of an item that is plain data: text that is not decoded, and delimiters and
/// brackets that were not used.
fn data_range(item: &Item) -> Option<Span> {
    match item {
        Item::Text {
            start,
            end,
            value: Value::Slice(span),
        } if span.start == *start && span.end == *end => Some(*span),
        Item::Delim(delim) => Some(Span::new(delim.start, delim.start + delim.count)),
        Item::Open(opener) => Some(Span::new(opener.start, opener.start + if opener.image { 2 } else { 1 })),
        _ => None,
    }
}

/// Links the email addresses found in runs of plain data. A line ending ends a run.
pub(super) fn link_emails(src: &str, items: Vec<Item>) -> Vec<Item> {
    let mut out = Vec::with_capacity(items.len());
    let mut group: Vec<Item> = Vec::new();
    let mut range = Span::new(0, 0);

    let finish = |group: &mut Vec<Item>, range: Span, out: &mut Vec<Item>| {
        if group.is_empty() {
            return;
        }
        if range.of(src).contains('@') {
            group.clear();
            split_emails(src, range.start, range.end, out);
        } else {
            out.append(group);
        }
    };

    for item in items {
        let data = data_range(&item).filter(|span| !matches!(src.as_bytes()[span.start], b'\n' | b'\r'));
        match data {
            Some(span) => {
                if group.is_empty() {
                    range.start = span.start;
                }
                range.end = span.end;
                group.push(item);
            }
            None => {
                finish(&mut group, range, &mut out);
                out.push(item);
            }
        }
    }
    finish(&mut group, range, &mut out);

    out
}
