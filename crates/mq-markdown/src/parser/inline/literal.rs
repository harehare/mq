//! GFM autolink literals: `www.example.com`, `https://example.com` and `user@example.com`.
//!
//! Ported from the state machines of `markdown-rs` (MIT License, Titus Wormer) so that the same
//! text is linked, including how trailing punctuation and unbalanced parentheses are excluded.

use super::punctuation::is_punctuation;
use super::{Item, Scanner, Value};
use crate::node::{Link, Node, Text, Url};
use crate::parser::scan::Span;

#[derive(PartialEq)]
enum Kind {
    Whitespace,
    Punctuation,
    Other,
}

/// Classifies the character at byte `index`; the end of the text counts as whitespace.
fn kind_at(src: &str, index: usize) -> Kind {
    let Some(char) = src.get(index..).and_then(|rest| rest.chars().next()) else {
        return Kind::Whitespace;
    };
    if char.is_whitespace() {
        Kind::Whitespace
    } else if is_punctuation(char) {
        Kind::Punctuation
    } else {
        Kind::Other
    }
}

/// Whether the character at byte `index` is a symbol of the emoji blocks, which a domain may contain.
fn is_emoji(src: &str, index: usize) -> bool {
    src.get(index..)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|char| matches!(char, '\u{2600}'..='\u{27BF}' | '\u{1F300}'..='\u{1FAFF}'))
}

/// Whether the text from `index` is only trailing punctuation up to whitespace, the end or `<`.
fn trail(src: &str, mut index: usize) -> bool {
    let bytes = src.as_bytes();
    loop {
        match bytes.get(index) {
            Some(b'!' | b'"' | b'\'' | b')' | b'*' | b',' | b'.' | b':' | b';' | b'?' | b'_' | b'~') => index += 1,
            Some(b'&') => {
                index += 1;
                let letters = bytes[index..].iter().take_while(|b| b.is_ascii_alphabetic()).count();
                if letters == 0 || bytes.get(index + letters) != Some(&b';') {
                    return false;
                }
                index += letters + 1;
            }
            Some(b'<') => return true,
            Some(b']') => {
                index += 1;
                if matches!(bytes.get(index), None | Some(b'\t' | b'\n' | b' ' | b'(' | b'[')) {
                    return true;
                }
            }
            _ => return kind_at(src, index) == Kind::Whitespace,
        }
    }
}

/// Matches the domain that starts at `index`, returning the offset after it.
fn domain(src: &str, mut index: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let (mut seen, mut marker, mut marker_before) = (false, 0u8, 0u8);

    loop {
        match bytes.get(index) {
            Some(b'.' | b'_') => {
                if trail(src, index) {
                    break;
                }
                if bytes[index] == b'_' {
                    marker = b'_';
                } else {
                    marker_before = marker;
                    marker = 0;
                }
                index += 1;
            }
            Some(b'-' | 0x80..=0xBF) => index += 1,
            _ if kind_at(src, index) == Kind::Other || is_emoji(src, index) => {
                seen = true;
                index += 1;
            }
            _ => break,
        }
    }

    // Underscores are not allowed in the last two segments.
    (marker_before != b'_' && marker != b'_' && seen).then_some(index)
}

/// Matches the path that starts at `index`, returning the offset after it.
fn path(src: &str, mut index: usize) -> usize {
    let bytes = src.as_bytes();
    let (mut open, mut closed) = (0usize, 0usize);

    loop {
        match bytes.get(index) {
            None => return index,
            Some(0x80..=0xBF) => index += 1,
            Some(b'(') => {
                open += 1;
                index += 1;
            }
            Some(
                punctuation @ (b'!' | b'"' | b'&' | b'\'' | b')' | b'*' | b',' | b'.' | b':' | b';' | b'<' | b'?'
                | b']' | b'_' | b'~'),
            ) => {
                let unbalanced = *punctuation == b')' && closed < open;
                if trail(src, index) && !unbalanced {
                    return index;
                }
                if *punctuation == b')' {
                    closed += 1;
                }
                index += 1;
            }
            _ if kind_at(src, index) == Kind::Whitespace => return index,
            _ => index += 1,
        }
    }
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
    let end = domain(src, pos + letters + 3)?;
    Some(path(src, end))
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
    let end = domain(src, pos)?;
    Some(path(src, end))
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
