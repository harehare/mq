//! HTML blocks, with the start and end conditions of the seven kinds that `CommonMark` defines.

use super::scan::skip_blanks;

/// The kind of an HTML block, which decides how it ends.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    /// `<script`, `<pre`, `<style` or `<textarea`: ends at a line with the matching closing tag.
    Raw(&'static str),
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
    let after = rest.strip_prefix('<')?;
    if after.starts_with("!--") {
        Some(Kind::Comment)
    } else if after.starts_with("![CDATA[") {
        Some(Kind::Cdata)
    } else if after.starts_with('?') {
        Some(Kind::Instruction)
    } else if after
        .strip_prefix('!')
        .is_some_and(|name| name.starts_with(|c: char| c.is_ascii_alphabetic()))
    {
        Some(Kind::Declaration)
    } else {
        match after.strip_prefix('/') {
            Some(name) => tag(name, true),
            None => tag(after, false),
        }
    }
}

/// Classifies the tag whose name starts `text`, which is what follows `<` or `</`.
fn tag(text: &str, closing: bool) -> Option<Kind> {
    let bytes = text.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let name_end = bytes
        .iter()
        .position(|b| !(b.is_ascii_alphanumeric() || *b == b'-'))
        .unwrap_or(bytes.len());
    let after = &bytes[name_end..];
    let self_closing = after.starts_with(b"/");
    if !matches!(after.first(), None | Some(b'\t' | b' ' | b'/' | b'>')) {
        return None;
    }
    let name = text[..name_end].to_ascii_lowercase();

    if !closing
        && !self_closing
        && let Some(raw) = RAW_NAMES.iter().find(|raw| **raw == name)
    {
        Some(Kind::Raw(raw))
    } else if BLOCK_NAMES.contains(&name.as_str()) {
        // A slash has to be the start of `/>`.
        (!self_closing || after.starts_with(b"/>")).then_some(Kind::Basic)
    } else {
        is_complete_tag(after, closing).then_some(Kind::Complete)
    }
}

/// Whether `rest`, what follows the name of a tag, closes the tag with nothing after it on the line.
fn is_complete_tag(rest: &[u8], closing: bool) -> bool {
    let mut cursor = Cursor { bytes: rest, index: 0 };
    cursor.skip_blanks();
    if !closing {
        loop {
            match cursor.attribute() {
                Attribute::Read => cursor.skip_blanks(),
                Attribute::Absent => break,
                Attribute::Invalid => return false,
            }
        }
        cursor.eat(b'/');
    }
    cursor.eat(b'>') && {
        cursor.skip_blanks();
        cursor.peek().is_none()
    }
}

enum Attribute {
    Read,
    /// There is no attribute here.
    Absent,
    /// An attribute whose value is malformed.
    Invalid,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn skip_blanks(&mut self) {
        self.index = skip_blanks(self.bytes, self.index);
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        self.index += usize::from(found);
        found
    }

    fn skip_while(&mut self, accept: impl Fn(u8) -> bool) {
        while self.peek().is_some_and(&accept) {
            self.index += 1;
        }
    }

    /// Reads an attribute name and its optional value.
    fn attribute(&mut self) -> Attribute {
        if !self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b':' || b == b'_')
        {
            return Attribute::Absent;
        }
        self.skip_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b':' | b'_'));
        // Blanks before an `=` belong to the value, otherwise they are left to the caller.
        let name_end = self.index;
        self.skip_blanks();
        if self.eat(b'=') {
            self.skip_blanks();
            return if self.value() {
                Attribute::Read
            } else {
                Attribute::Invalid
            };
        }
        self.index = name_end;
        Attribute::Read
    }

    fn value(&mut self) -> bool {
        match self.peek() {
            None | Some(b'<' | b'=' | b'>' | b'`') => false,
            Some(quote @ (b'"' | b'\'')) => {
                self.index += 1;
                self.skip_while(|b| b != quote);
                // The closing quote has to be followed by blanks, `/` or `>`.
                self.eat(quote) && matches!(self.peek(), Some(b'\t' | b' ' | b'/' | b'>'))
            }
            Some(_) => {
                self.skip_while(|b| !matches!(b, b'\t' | b' ' | b'"' | b'\'' | b'/' | b'<' | b'=' | b'>' | b'`'));
                true
            }
        }
    }
}

/// Where in the first line the search for the end of a block starts, so that the opening marker is
/// only reused where it can close the block (`<!-->` and `<?>` are complete).
pub(super) fn first_line_offset(kind: Kind) -> usize {
    match kind {
        Kind::Comment => 2,
        Kind::Instruction => 1,
        Kind::Declaration => 3,
        Kind::Cdata => 9,
        Kind::Raw(_) | Kind::Basic | Kind::Complete => 1,
    }
}

/// Whether `text` holds what ends a block of `kind`. Blocks of the last two kinds end at blank lines.
pub(super) fn ends_in(kind: Kind, text: &str) -> bool {
    match kind {
        Kind::Comment => text.contains("-->"),
        Kind::Instruction => text.contains("?>"),
        Kind::Declaration => text.contains('>'),
        Kind::Cdata => text.contains("]]>"),
        Kind::Raw(name) => has_raw_end(text, name),
        Kind::Basic | Kind::Complete => false,
    }
}

/// Whether `text` has the closing tag of the raw element `name`, such as `</script>`.
fn has_raw_end(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while let Some(found) = text[index..].find("</") {
        let from = index + found + 2;
        let letters = bytes[from..].iter().take_while(|b| b.is_ascii_alphabetic()).count();
        if bytes.get(from + letters) == Some(&b'>') && text[from..from + letters].eq_ignore_ascii_case(name) {
            return true;
        }
        index = from;
    }
    false
}
