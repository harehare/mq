//! HTML blocks: the seven kinds of `CommonMark`, ported from the rules of `markdown-rs`.

/// The kind of an HTML block, which decides how it ends.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    /// `<script`, `<pre`, `<style` or `<textarea`: ends at a line with the matching closing tag.
    Raw,
    /// `<!--`: ends at a line with `-->`.
    Comment,
    /// `<?`: ends at a line with `?>`.
    Instruction,
    /// `<!` and a letter: ends at a line with `>`.
    Declaration,
    /// `<![CDATA[`: ends at a line with `]]>`.
    Cdata,
    /// A known block tag name: ends at a blank line.
    Basic,
    /// A complete tag alone on its line: ends at a blank line. It cannot interrupt a paragraph.
    Complete,
}

const RAW_NAMES: [&str; 4] = ["pre", "script", "style", "textarea"];

const BLOCK_NAMES: [&str; 62] = [
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];

/// Recognizes the start of an HTML block in `rest`, the line without its indentation.
pub(super) fn start(rest: &str) -> Option<Kind> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }

    match bytes.get(1)? {
        b'!' => match bytes.get(2)? {
            b'-' if bytes.get(3) == Some(&b'-') => Some(Kind::Comment),
            b'[' if rest[3..].starts_with("CDATA[") => Some(Kind::Cdata),
            byte if byte.is_ascii_alphabetic() => Some(Kind::Declaration),
            _ => None,
        },
        b'?' => Some(Kind::Instruction),
        b'/' => tag(rest, 2, true),
        byte if byte.is_ascii_alphabetic() => tag(rest, 1, false),
        _ => None,
    }
}

/// Recognizes a block tag, a raw tag or a complete tag whose name starts at `from`.
fn tag(rest: &str, from: usize, closing: bool) -> Option<Kind> {
    let bytes = rest.as_bytes();
    if !bytes.get(from).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let length = bytes[from..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
        .count();
    let end = from + length;
    let terminator = bytes.get(end).copied();
    if !matches!(terminator, None | Some(b'\t' | b' ' | b'/' | b'>')) {
        return None;
    }
    let slash = terminator == Some(b'/');
    let name = rest[from..end].to_ascii_lowercase();

    if !slash && !closing && RAW_NAMES.contains(&name.as_str()) {
        Some(Kind::Raw)
    } else if BLOCK_NAMES.contains(&name.as_str()) {
        // A slash has to be the end of a self-closing tag.
        (!slash || bytes.get(end + 1) == Some(&b'>')).then_some(Kind::Basic)
    } else {
        complete_tag(&bytes[end..], closing).then_some(Kind::Complete)
    }
}

fn skip_blanks(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }
    index
}

/// Whether the rest of a tag after its name is complete and alone on the line.
fn complete_tag(bytes: &[u8], closing: bool) -> bool {
    let mut index = 0;

    if closing {
        index = skip_blanks(bytes, index);
        return bytes.get(index) == Some(&b'>') && skip_blanks(bytes, index + 1) == bytes.len();
    }

    loop {
        index = skip_blanks(bytes, index);
        match bytes.get(index) {
            Some(b'/') => {
                return bytes.get(index + 1) == Some(&b'>') && skip_blanks(bytes, index + 2) == bytes.len();
            }
            Some(b'0'..=b'9' | b':' | b'A'..=b'Z' | b'_' | b'a'..=b'z') => {
                index += 1;
                while matches!(
                    bytes.get(index),
                    Some(b'-' | b'.' | b'0'..=b'9' | b':' | b'A'..=b'Z' | b'_' | b'a'..=b'z')
                ) {
                    index += 1;
                }
                let after = skip_blanks(bytes, index);
                if bytes.get(after) == Some(&b'=') {
                    let Some(end) = attribute_value(bytes, skip_blanks(bytes, after + 1)) else {
                        return false;
                    };
                    index = end;
                }
            }
            Some(b'>') => return skip_blanks(bytes, index + 1) == bytes.len(),
            _ => return false,
        }
    }
}

/// The offset after the attribute value at `index`.
fn attribute_value(bytes: &[u8], index: usize) -> Option<usize> {
    match bytes.get(index)? {
        b'<' | b'=' | b'>' | b'`' => None,
        quote @ (b'"' | b'\'') => {
            let close = bytes[index + 1..].iter().position(|b| b == quote)?;
            let end = index + 1 + close + 1;
            matches!(bytes.get(end), Some(b'\t' | b' ' | b'/' | b'>')).then_some(end)
        }
        _ => {
            let length = bytes[index..]
                .iter()
                .take_while(|b| !matches!(b, b'\t' | b' ' | b'"' | b'\'' | b'/' | b'<' | b'=' | b'>' | b'`'))
                .count();
            Some(index + length)
        }
    }
}

/// Where in the first line the search for the end of a block starts, so that the opening marker is
/// only reused where `markdown-rs` reuses it (`<!-->` and `<?>` are complete).
pub(super) fn first_line_offset(kind: Kind) -> usize {
    match kind {
        Kind::Comment => 2,
        Kind::Instruction => 1,
        Kind::Declaration => 3,
        Kind::Cdata => 9,
        Kind::Raw | Kind::Basic | Kind::Complete => 1,
    }
}

/// Whether `text` holds what ends a block of `kind`. Blocks of the last two kinds end at blank lines.
pub(super) fn ends_in(kind: Kind, text: &str) -> bool {
    match kind {
        Kind::Comment => text.contains("-->"),
        Kind::Instruction => text.contains("?>"),
        Kind::Declaration => text.contains('>'),
        Kind::Cdata => text.contains("]]>"),
        Kind::Raw => has_raw_end(text),
        Kind::Basic | Kind::Complete => false,
    }
}

/// Whether `text` has a closing tag of a raw element, such as `</script>`.
fn has_raw_end(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while let Some(found) = text[index..].find("</") {
        let from = index + found + 2;
        let letters = bytes[from..].iter().take_while(|b| b.is_ascii_alphabetic()).count();
        if bytes.get(from + letters) == Some(&b'>')
            && RAW_NAMES.contains(&text[from..from + letters].to_ascii_lowercase().as_str())
        {
            return true;
        }
        index = from;
    }
    false
}
