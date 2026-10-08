//! MDX: JSX tags and expressions, ported from the rules of `markdown-rs` without a JavaScript parser,
//! so expressions are only checked for balanced braces, outside of strings, template literals and comments.

use crate::node::{MdxAttributeContent, MdxAttributeValue, MdxJsxAttribute};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::cell::OnceCell;

/// What the end of the input means for a construct it ends inside of.
pub(super) enum Fallback {
    Nok,
    Error(String),
}

/// The outcome of parsing at a position.
pub(super) enum Parsed<T> {
    Ok(T),
    /// Not this construct, so the text is something else.
    Nok,
    /// The input ended inside the construct. More input may complete it, and if there is none this
    /// is what the end of the input means for it.
    More(Fallback),
    /// The construct is invalid.
    Error(String),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum TagKind {
    Open,
    Close,
    SelfClosing,
}

/// A JSX tag: `<a b="c">`, `</a>` or `<a />`. A fragment has no name.
pub(super) struct Tag {
    pub(super) name: Option<String>,
    pub(super) attributes: Vec<MdxAttributeContent>,
    pub(super) kind: TagKind,
    /// Offset after the closing `>`.
    pub(super) end: usize,
}

/// Whether `char` can start a JavaScript identifier.
fn id_start(char: char) -> bool {
    unicode_id::UnicodeID::is_id_start(char) || matches!(char, '$' | '_')
}

/// Whether `char` can continue a JSX identifier, which may contain dashes.
fn id_continue(char: char) -> bool {
    unicode_id::UnicodeID::is_id_continue(char) || matches!(char, '\u{200c}' | '\u{200d}' | '-')
}

/// What a tag parser stops with: a failure of the construct, or of the input.
enum Stop {
    Nok,
    More(Fallback),
    Error(String),
}

type Step<T> = Result<T, Stop>;

const UNCLOSED_EXPRESSION: &str =
    "Unexpected end of file in expression, expected a corresponding closing brace for `{`";

/// Where each `{` of a source closes, found in one pass the first time one is asked for, so that
/// many unclosed braces do not each scan to the end of the input.
#[derive(Default)]
pub(super) struct Braces(OnceCell<FxHashMap<usize, usize>>);

impl Braces {
    /// The offset of the `}` that closes the `{` at `open`, if there is one.
    fn close(&self, src: &str, open: usize) -> Option<usize> {
        self.0.get_or_init(|| match_braces(src, 0, false)).get(&open).copied()
    }
}

enum Frame {
    /// A `{`, or a `${` of a template literal when it has no offset.
    Brace(Option<usize>),
    Template,
}

/// Matches the braces of the expressions in `src` from `from`, which are not checked as JavaScript but
/// skip over strings, template literals and comments. Text outside braces is not looked at. With
/// `single`, `from` is a `{` and the search ends once it is closed.
fn match_braces(src: &str, from: usize, single: bool) -> FxHashMap<usize, usize> {
    let bytes = src.as_bytes();
    let mut closes = FxHashMap::default();
    let mut stack = Vec::new();
    let mut index = from;

    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        match stack.last() {
            None => {
                if byte == b'{' {
                    stack.push(Frame::Brace(Some(index - 1)));
                }
            }
            Some(Frame::Template) => match byte {
                b'\\' => index += 1,
                b'`' => {
                    stack.pop();
                }
                b'$' if bytes.get(index) == Some(&b'{') => {
                    stack.push(Frame::Brace(None));
                    index += 1;
                }
                _ => {}
            },
            Some(Frame::Brace(_)) => match byte {
                b'{' => stack.push(Frame::Brace(Some(index - 1))),
                b'}' => {
                    if let Some(Frame::Brace(Some(open))) = stack.pop() {
                        closes.insert(open, index - 1);
                        if single && open == from {
                            break;
                        }
                    }
                }
                b'`' => stack.push(Frame::Template),
                b'\'' | b'"' => index = skip_string(bytes, index, byte).unwrap_or(index),
                b'/' => match bytes.get(index) {
                    Some(b'/') => {
                        index += bytes[index..].iter().take_while(|&&b| b != b'\n').count();
                    }
                    Some(b'*') => {
                        if let Some(end) = src[index + 1..].find("*/") {
                            index += 1 + end + 2;
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
        }
    }
    closes
}

/// The offset after the string that starts before `from` with the `quote`. A string that does not end on
/// its line is not one.
fn skip_string(bytes: &[u8], from: usize, quote: u8) -> Option<usize> {
    let mut index = from;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'\\' => index += 1,
            b'\n' | b'\r' => return None,
            _ if byte == quote => return Some(index + 1),
            _ => {}
        }
        index += 1;
    }
    None
}

struct Cursor<'a> {
    src: &'a str,
    index: usize,
    /// Set when the same source is searched for many expressions. Without it each one is scanned.
    braces: Option<&'a Braces>,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.index..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(char) = self.peek() {
            self.index += char.len_utf8();
        }
    }

    /// The input ends inside a construct: more input may follow, otherwise it is not one.
    fn end_of_input<T>(&self) -> Step<T> {
        Err(Stop::More(Fallback::Nok))
    }

    fn crash<T>(&self, message: String) -> Step<T> {
        Err(Stop::Error(message))
    }

    /// Skips whitespace, including line endings. Ends the tag at the end of the input.
    fn skip_whitespace(&mut self) -> Step<()> {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
        if self.peek().is_none() {
            return self.end_of_input();
        }
        Ok(())
    }

    fn name_part(&mut self) -> Step<&str> {
        let start = self.index;
        self.bump();
        while self.peek().is_some_and(id_continue) {
            self.bump();
        }
        Ok(&self.src[start..self.index])
    }

    fn describe(&self) -> String {
        match self.peek() {
            Some(char) => format!("character `{char}` (U+{:04X})", char as u32),
            None => "end of file".to_string(),
        }
    }

    /// The name of a tag, with its member (`.`) or namespace (`:`) parts, cleaned of whitespace.
    fn tag_name(&mut self) -> Step<String> {
        let mut name = self.name_part()?.to_string();
        self.skip_whitespace()?;
        match self.peek() {
            Some('.') => {
                while self.peek() == Some('.') {
                    self.bump();
                    self.skip_whitespace()?;
                    if !self.peek().is_some_and(id_start) {
                        return self.crash(format!("Unexpected {} before member name", self.describe()));
                    }
                    name.push('.');
                    name.push_str(self.name_part()?);
                    self.skip_whitespace()?;
                }
            }
            Some(':') => {
                self.bump();
                self.skip_whitespace()?;
                if !self.peek().is_some_and(id_start) {
                    return self.crash(format!("Unexpected {} before local name", self.describe()));
                }
                name.push(':');
                name.push_str(self.name_part()?);
                self.skip_whitespace()?;
            }
            _ => {}
        }
        Ok(name)
    }

    /// A `{...}` expression with balanced braces, returning the offsets of its content.
    fn expression(&mut self) -> Step<(usize, usize)> {
        let open = self.index;
        self.bump();
        let start = self.index;
        let end = match self.braces {
            Some(braces) => braces.close(self.src, open),
            None => match_braces(self.src, open, true).get(&open).copied(),
        };
        let Some(end) = end else {
            self.index = self.src.len();
            return Err(Stop::More(Fallback::Error(UNCLOSED_EXPRESSION.into())));
        };
        self.index = end + 1;
        Ok((start, end))
    }
}

impl Cursor<'_> {
    /// The value of an expression: its content, without up to two columns of whitespace at the start
    /// of each line after the first.
    fn expression_value(&self, start: usize, end: usize) -> SmolStr {
        let content = &self.src[start..end];
        if !content.contains(['\n', '\r']) {
            return SmolStr::new(content);
        }
        let mut value = String::with_capacity(content.len());
        // How many more columns of whitespace may be dropped at the start of the current line, and the
        // column that the line is at.
        let mut droppable = 0usize;
        let mut column = 0usize;
        for char in content.chars() {
            match char {
                '\n' | '\r' => {
                    droppable = 2;
                    column = 0;
                    value.push(char);
                }
                ' ' if droppable > 0 => {
                    droppable -= 1;
                    column += 1;
                }
                // What is left of a tab that is dropped in part stays as spaces.
                '\t' if droppable > 0 => {
                    let width = 4 - column % 4;
                    value.extend(std::iter::repeat_n(' ', width.saturating_sub(droppable)));
                    droppable = droppable.saturating_sub(width);
                    column += width;
                }
                _ => {
                    droppable = 0;
                    value.push(char);
                }
            }
        }
        SmolStr::new(value)
    }

    /// Checks the character after a name: whitespace or one of `allowed`.
    fn end_of_name(&self, allowed: &[char], what: &str) -> Step<()> {
        match self.peek() {
            None => self.end_of_input(),
            Some(char) if char.is_whitespace() || allowed.contains(&char) => Ok(()),
            Some(_) => self.crash(format!("Unexpected {} in {what}", self.describe())),
        }
    }

    fn attribute(&mut self) -> Step<MdxAttributeContent> {
        let mut name = self.name_part()?.to_string();
        self.end_of_name(&['/', ':', '=', '>', '{'], "attribute name")?;
        self.skip_whitespace()?;

        if self.peek() == Some(':') {
            self.bump();
            self.skip_whitespace()?;
            if !self.peek().is_some_and(id_start) {
                return self.crash(format!("Unexpected {} before local attribute name", self.describe()));
            }
            name.push(':');
            name.push_str(self.name_part()?);
            self.end_of_name(&['/', '=', '>', '{'], "local attribute name")?;
            self.skip_whitespace()?;
        }

        let name = SmolStr::new(name);
        if self.peek() != Some('=') {
            if !matches!(self.peek(), Some('/' | '>' | '{')) && !self.peek().is_some_and(id_start) {
                return self.crash(format!("Unexpected {} after attribute name", self.describe()));
            }
            return Ok(MdxAttributeContent::Property(MdxJsxAttribute { name, value: None }));
        }

        self.bump();
        self.skip_whitespace()?;
        let value = match self.peek() {
            Some(quote @ ('"' | '\'')) => {
                self.bump();
                let start = self.index;
                let Some(length) = self.src[start..].find(quote) else {
                    return Err(Stop::More(Fallback::Error(format!(
                        "Unexpected end of file in attribute value, expected a corresponding closing quote `{quote}`"
                    ))));
                };
                self.index = start + length + 1;
                MdxAttributeValue::Literal(SmolStr::new(super::inline::decode_references(
                    &self.src[start..start + length],
                )))
            }
            Some('{') => {
                let (start, end) = self.expression()?;
                MdxAttributeValue::Expression(self.expression_value(start, end))
            }
            _ => {
                return self.crash(format!("Unexpected {} before attribute value", self.describe()));
            }
        };
        Ok(MdxAttributeContent::Property(MdxJsxAttribute {
            name,
            value: Some(value),
        }))
    }

    fn tag(&mut self) -> Step<Tag> {
        // `<` is followed by a name, not by whitespace.
        match self.peek() {
            Some(' ' | '\t' | '\n' | '\r') => return Err(Stop::Nok),
            None => return self.end_of_input(),
            Some(_) => {}
        }
        self.skip_whitespace()?;

        let mut kind = TagKind::Open;
        if self.peek() == Some('/') {
            self.bump();
            self.skip_whitespace()?;
            kind = TagKind::Close;
        }

        let name = match self.peek() {
            Some('>') => None,
            Some(char) if id_start(char) => Some(self.tag_name()?),
            _ => return self.crash(format!("Unexpected {} before name", self.describe())),
        };
        if name.is_some() {
            self.end_of_tag_name()?;
        }

        let mut attributes = Vec::new();
        // Errors about a closing tag are reported once the tag is known to end.
        let mut misplaced = None;
        loop {
            match self.peek() {
                Some('/') => {
                    self.bump();
                    self.skip_whitespace()?;
                    if self.peek() != Some('>') {
                        return self.crash(format!("Unexpected {} after self-closing slash", self.describe()));
                    }
                    if kind == TagKind::Close {
                        return self.crash(
                            "Unexpected self-closing slash `/` in closing tag, expected the end of the tag".into(),
                        );
                    }
                    kind = TagKind::SelfClosing;
                }
                Some('>') => {
                    self.bump();
                    break;
                }
                Some(char) if char == '{' || id_start(char) => {
                    if kind == TagKind::Close {
                        misplaced = Some("Unexpected attribute in closing tag, expected the end of the tag");
                    }
                    let attribute = if char == '{' {
                        let (start, end) = self.expression()?;
                        MdxAttributeContent::Expression(self.expression_value(start, end))
                    } else {
                        self.attribute()?
                    };
                    attributes.push(attribute);
                    self.skip_whitespace()?;
                }
                None => return self.end_of_input(),
                Some(_) => return self.crash(format!("Unexpected {} before attribute name", self.describe())),
            }
        }

        if let Some(message) = misplaced {
            return self.crash(message.into());
        }

        Ok(Tag {
            name,
            attributes,
            kind,
            end: self.index,
        })
    }

    /// After the name of a tag comes a slash, the end, or an attribute.
    fn end_of_tag_name(&self) -> Step<()> {
        match self.peek() {
            None => self.end_of_input(),
            Some('/' | '>' | '{') => Ok(()),
            // An attribute needs whitespace before it.
            Some(char) if id_start(char) && self.src[..self.index].ends_with(char::is_whitespace) => Ok(()),
            Some(_) => self.crash(format!("Unexpected {} after name", self.describe())),
        }
    }
}

fn parsed<T>(step: Step<T>) -> Parsed<T> {
    match step {
        Ok(value) => Parsed::Ok(value),
        Err(Stop::Nok) => Parsed::Nok,
        Err(Stop::More(fallback)) => Parsed::More(fallback),
        Err(Stop::Error(message)) => Parsed::Error(message),
    }
}

/// Parses the JSX tag that starts at `pos`, a `<`.
pub(super) fn tag(src: &str, pos: usize, braces: Option<&Braces>) -> Parsed<Tag> {
    let mut cursor = Cursor {
        src,
        index: pos + 1,
        braces,
    };
    parsed(cursor.tag())
}

/// Parses the expression that starts at `pos`, a `{`, returning the offset after it and its value.
pub(super) fn expression(src: &str, pos: usize, braces: Option<&Braces>) -> Parsed<(usize, SmolStr)> {
    let mut cursor = Cursor {
        src,
        index: pos,
        braces,
    };
    parsed(
        cursor
            .expression()
            .map(|(start, end)| (end + 1, cursor.expression_value(start, end))),
    )
}
