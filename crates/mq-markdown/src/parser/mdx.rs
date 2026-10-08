//! MDX: JSX tags and expressions, ported from the rules of `markdown-rs` without a JavaScript parser,
//! so expressions are only checked for balanced braces.

use crate::node::{MdxAttributeContent, MdxAttributeValue, MdxJsxAttribute};
use smol_str::SmolStr;
use std::cell::OnceCell;
use std::collections::HashMap;

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
pub(super) struct Braces(OnceCell<HashMap<usize, usize>>);

impl Braces {
    /// The offset of the `}` that closes the `{` at `open`, if there is one.
    fn close(&self, src: &str, open: usize) -> Option<usize> {
        self.0
            .get_or_init(|| {
                let mut closes = HashMap::new();
                let mut opens = Vec::new();
                for (index, byte) in src.bytes().enumerate() {
                    match byte {
                        b'{' => opens.push(index),
                        b'}' => {
                            if let Some(open) = opens.pop() {
                                closes.insert(open, index);
                            }
                        }
                        _ => {}
                    }
                }
                closes
            })
            .get(&open)
            .copied()
    }
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
        if let Some(braces) = self.braces {
            let Some(end) = braces.close(self.src, open) else {
                self.index = self.src.len();
                return Err(Stop::More(Fallback::Error(UNCLOSED_EXPRESSION.into())));
            };
            self.index = end + 1;
            return Ok((start, end));
        }
        let mut depth = 0usize;
        loop {
            match self.peek() {
                None => {
                    return Err(Stop::More(Fallback::Error(UNCLOSED_EXPRESSION.into())));
                }
                Some('{') => depth += 1,
                Some('}') if depth == 0 => {
                    let end = self.index;
                    self.bump();
                    return Ok((start, end));
                }
                Some('}') => depth -= 1,
                Some(_) => {}
            }
            self.bump();
        }
    }
}

impl Cursor<'_> {
    /// The value of an expression: its content, without up to two whitespace characters at the start
    /// of each line after the first.
    fn expression_value(&self, start: usize, end: usize) -> SmolStr {
        let content = &self.src[start..end];
        if !content.contains(['\n', '\r']) {
            return SmolStr::new(content);
        }
        let mut value = String::with_capacity(content.len());
        // How many more whitespace characters may be dropped at the start of the current line.
        let mut droppable = 0;
        for char in content.chars() {
            match char {
                '\n' | '\r' => {
                    droppable = 2;
                    value.push(char);
                }
                ' ' | '\t' if droppable > 0 => droppable -= 1,
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
        // Unlike before the `=`, no whitespace is allowed after it.
        if self.peek().is_some_and(char::is_whitespace) {
            return Err(Stop::Nok);
        }
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
