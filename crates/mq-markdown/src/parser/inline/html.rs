//! Autolinks (`<https://example.com>`) and raw inline HTML.

/// An autolink found at `<`.
pub(super) struct Autolink {
    pub(super) url: String,
    /// Start and end of the link text, inside the angle brackets.
    pub(super) text: (usize, usize),
    /// Offset after the closing `>`.
    pub(super) end: usize,
}

/// Parses an autolink at `pos` (a `<`): an absolute URI or an email address.
pub(super) fn autolink(src: &str, pos: usize) -> Option<Autolink> {
    let bytes = src.as_bytes();
    let inner = pos + 1;

    let scheme_char = |b: &u8| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.');
    let scheme = bytes
        .get(inner)
        .is_some_and(u8::is_ascii_alphabetic)
        .then(|| {
            1 + bytes[inner + 1..]
                .iter()
                .take(31)
                .take_while(|b| scheme_char(b))
                .count()
        })
        .filter(|&length| length >= 2 && bytes.get(inner + length) == Some(&b':'));

    if let Some(length) = scheme {
        // Everything up to the closing `>` is the URL.
        let from = inner + length + 1;
        let close = from
            + bytes[from..]
                .iter()
                .position(|&b| b == b'>' || b <= 0x1F || matches!(b, b' ' | b'<' | 0x7F))?;
        return (bytes[close] == b'>').then(|| Autolink {
            url: src[inner..close].to_string(),
            text: (inner, close),
            end: close + 1,
        });
    }

    let end = email(bytes, inner)?;
    Some(Autolink {
        url: format!("mailto:{}", &src[inner..end]),
        text: (inner, end),
        end: end + 1,
    })
}

/// Whether `text` is an email address that an autolink (`<text>`) can hold.
pub(crate) fn is_autolink_email(text: &str) -> bool {
    let closed = format!("{text}>");
    email(closed.as_bytes(), 0) == Some(text.len())
}

/// Parses an email address for an autolink at `start`, returning the offset of the closing `>`.
/// The characters allowed differ slightly from `CommonMark`, following `markdown-rs`.
fn email(bytes: &[u8], start: usize) -> Option<usize> {
    let atext = |b: u8| matches!(b, b'#'..=b'\'' | b'*' | b'+' | b'-'..=b'9' | b'=' | b'?' | b'A'..=b'Z' | b'^'..=b'~');
    let mut index = start;
    while bytes.get(index).copied().is_some_and(atext) {
        index += 1;
    }
    if index == start || bytes.get(index) != Some(&b'@') {
        return None;
    }
    index += 1;

    // Labels of at most 63 letters, digits and hyphens, starting and ending with a letter or digit.
    loop {
        let label = index;
        if !bytes.get(index).is_some_and(u8::is_ascii_alphanumeric) {
            return None;
        }
        while bytes
            .get(index)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
        {
            index += 1;
        }
        if index - label > 63 || bytes[index - 1] == b'-' {
            return None;
        }
        match bytes.get(index) {
            Some(b'.') => index += 1,
            Some(b'>') => return Some(index),
            _ => return None,
        }
    }
}

/// Terminators that a search reached the end of the input without finding, so that later searches
/// for the same one fail without scanning again. Each is the first offset a search started from.
#[derive(Default)]
pub(super) struct Misses {
    comment: Option<usize>,
    instruction: Option<usize>,
    cdata: Option<usize>,
    declaration: Option<usize>,
}

/// Finds `terminator` in `src` from `from`, recording a failure in `miss`.
fn find_from(src: &str, from: usize, terminator: &str, miss: &mut Option<usize>) -> Option<usize> {
    if miss.is_some_and(|missed| from >= missed) {
        return None;
    }
    let found = src[from..].find(terminator).map(|index| from + index);
    if found.is_none() {
        *miss = Some(from);
    }
    found
}

/// Parses raw inline HTML at `pos` (a `<`), returning the offset after it.
pub(super) fn inline_html(src: &str, pos: usize, misses: &mut Misses) -> Option<usize> {
    let rest = &src[pos..];
    let bytes = rest.as_bytes();

    if rest.starts_with("<!--") {
        if rest.starts_with("<!-->") {
            return Some(pos + 5);
        }
        if rest.starts_with("<!--->") {
            return Some(pos + 6);
        }
        return find_from(src, pos + 4, "-->", &mut misses.comment).map(|index| index + 3);
    }
    if rest.starts_with("<?") {
        return find_from(src, pos + 2, "?>", &mut misses.instruction).map(|index| index + 2);
    }
    if rest.starts_with("<![CDATA[") {
        return find_from(src, pos + 9, "]]>", &mut misses.cdata).map(|index| index + 3);
    }
    if rest.starts_with("<!") && bytes.get(2).is_some_and(u8::is_ascii_alphabetic) {
        return find_from(src, pos, ">", &mut misses.declaration).map(|index| index + 1);
    }
    if rest.starts_with("</") {
        return closing_tag(bytes).map(|length| pos + length);
    }
    open_tag(bytes).map(|length| pos + length)
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// The length of a tag name at the start of `bytes`.
fn tag_name(bytes: &[u8]) -> usize {
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return 0;
    }
    bytes
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
        .count()
}

fn skip_spaces(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).copied().is_some_and(is_space) {
        index += 1;
    }
    index
}

/// The length of a closing tag such as `</a>` at the start of `bytes`.
fn closing_tag(bytes: &[u8]) -> Option<usize> {
    let name = tag_name(&bytes[2..]);
    if name == 0 {
        return None;
    }
    let index = skip_spaces(bytes, 2 + name);
    (bytes.get(index) == Some(&b'>')).then_some(index + 1)
}

/// The length of an open tag such as `<a href="x">` at the start of `bytes`.
fn open_tag(bytes: &[u8]) -> Option<usize> {
    let name = tag_name(&bytes[1..]);
    if name == 0 {
        return None;
    }
    let mut index = 1 + name;

    loop {
        let after_space = skip_spaces(bytes, index);
        let has_space = after_space > index;
        match bytes.get(after_space)? {
            b'>' => return Some(after_space + 1),
            b'/' => return (bytes.get(after_space + 1) == Some(&b'>')).then_some(after_space + 2),
            byte if has_space && (byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':')) => {
                index = attribute(bytes, after_space)?;
            }
            _ => return None,
        }
    }
}

/// The offset after the attribute (name and optional value) that starts at `start`.
fn attribute(bytes: &[u8], start: usize) -> Option<usize> {
    let name = bytes[start..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(**b, b'_' | b'.' | b':' | b'-'))
        .count();
    let end = start + name;

    let after_space = skip_spaces(bytes, end);
    if bytes.get(after_space) != Some(&b'=') {
        return Some(end);
    }
    let value = skip_spaces(bytes, after_space + 1);
    match bytes.get(value)? {
        quote @ (b'"' | b'\'') => {
            let close = bytes[value + 1..].iter().position(|b| b == quote)?;
            Some(value + 1 + close + 1)
        }
        _ => {
            let length = bytes[value..]
                .iter()
                .take_while(|&&b| !is_space(b) && !matches!(b, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`'))
                .count();
            (length > 0).then_some(value + length)
        }
    }
}
