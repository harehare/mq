pub mod token;

use smol_str::SmolStr;
use token::{StringSegment, Token, TokenKind};

use crate::error::syntax::SyntaxError;
use crate::module::ModuleId;
use crate::number::Number;
use crate::range::{Position, Range};

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub ignore_errors: bool,
    pub include_spaces: bool,
}

pub struct Lexer {
    options: Options,
}

impl Lexer {
    pub fn new(options: Options) -> Self {
        Self { options }
    }

    pub fn tokenize(&self, input: &str, module_id: ModuleId) -> Result<Vec<Token>, SyntaxError> {
        let mut cursor = Cursor::new(input, module_id);
        let mut tokens = Vec::with_capacity((input.len() / 5).max(16));

        if self.options.include_spaces {
            loop {
                match cursor.token_include_spaces() {
                    Some(token) => tokens.push(token),
                    None if self.options.ignore_errors && !cursor.rest().is_empty() => tokens.push(cursor.unknown()),
                    None => break,
                }
            }
        } else {
            loop {
                cursor.skip_whitespace_and_comments();
                match cursor.token() {
                    Some(token) => tokens.push(token),
                    None => break,
                }
            }
        }

        let eof = cursor.rest_range();
        let token = Token {
            range: eof,
            kind: TokenKind::Eof,
            module_id,
        };

        if token.range.start == token.range.end || self.options.ignore_errors {
            tokens.push(token);
            Ok(tokens)
        } else {
            Err(SyntaxError::UnexpectedToken(token))
        }
    }
}

/// Parses `input` as an interpolated string body (like between `s"` and `"`) without requiring
/// the surrounding quotes, e.g. for DAP logpoint messages.
#[cfg(feature = "debugger")]
pub(crate) fn parse_interpolation_segments(
    input: &str,
    module_id: ModuleId,
) -> Result<Vec<StringSegment>, SyntaxError> {
    let mut cursor = Cursor::new(input, module_id);
    let mut segments = Vec::with_capacity(4);

    while !cursor.rest().is_empty() {
        match cursor.string_segment() {
            Some(segment) => segments.push(segment),
            None => break,
        }
    }

    if cursor.rest().is_empty() {
        Ok(segments)
    } else {
        Err(SyntaxError::UnexpectedToken(Token {
            range: cursor.rest_range(),
            kind: TokenKind::Eof,
            module_id,
        }))
    }
}

/// Outcome of scanning a number: `Fatal` is a malformed exponent, which rejects the whole token
/// instead of letting the caller try other token kinds.
enum NumberScan {
    Match(usize, f64),
    NoMatch,
    Fatal,
}

fn keyword_kind(word: &str) -> Option<TokenKind> {
    Some(match word {
        "as" => TokenKind::As,
        "break" => TokenKind::Break,
        "catch" => TokenKind::Catch,
        "continue" => TokenKind::Continue,
        "def" => TokenKind::Def,
        "do" => TokenKind::Do,
        "elif" => TokenKind::Elif,
        "else" => TokenKind::Else,
        "end" => TokenKind::End,
        "fn" => TokenKind::Fn,
        "foreach" => TokenKind::Foreach,
        "if" => TokenKind::If,
        "import" => TokenKind::Import,
        "include" => TokenKind::Include,
        "let" => TokenKind::Let,
        "loop" => TokenKind::Loop,
        "match" => TokenKind::Match,
        "module" => TokenKind::Module,
        "nodes" => TokenKind::Nodes,
        "None" => TokenKind::None,
        "self" => TokenKind::Self_,
        "try" => TokenKind::Try,
        "unless" => TokenKind::Unless,
        "until" => TokenKind::Until,
        "var" => TokenKind::Var,
        "while" => TokenKind::While,
        "yield" => TokenKind::Yield,
        _ => return None,
    })
}

fn ident_kind(word: &str) -> TokenKind {
    match word {
        "true" => TokenKind::BoolLiteral(true),
        "false" => TokenKind::BoolLiteral(false),
        s => TokenKind::Ident(SmolStr::new(s)),
    }
}

/// Characters of `text` after the `\` of an escape that map to themselves in a string literal.
fn is_plain_string_escape(c: char) -> bool {
    matches!(
        c,
        '/' | '['
            | ']'
            | '('
            | ')'
            | '{'
            | '}'
            | '+'
            | '*'
            | '?'
            | '^'
            | '$'
            | '|'
            | '-'
            | '.'
            | 's'
            | 'S'
            | 'd'
            | 'D'
            | 'w'
            | 'W'
    )
}

/// Decodes the escape following a `\` (`rest` starts right after it) as `(char, bytes consumed)`.
///
/// `plain_string_escapes` also accepts the extra single-character escapes of `"..."` literals.
fn decode_escape(rest: &str, plain_string_escapes: bool) -> Option<(char, usize)> {
    let mut chars = rest.chars();
    let c = chars.next()?;
    let simple = match c {
        '\\' => Some('\\'),
        '"' => Some('"'),
        'r' => Some('\r'),
        'n' => Some('\n'),
        't' => Some('\t'),
        '{' | '}' => Some(c),
        c if plain_string_escapes && is_plain_string_escape(c) => Some(c),
        _ => None,
    };
    if let Some(ch) = simple {
        return Some((ch, 1));
    }

    let bytes = rest.as_bytes();
    let hex = |range: std::ops::Range<usize>| -> Option<u32> {
        let digits = bytes.get(range)?;
        if digits.iter().all(u8::is_ascii_hexdigit) {
            u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
        } else {
            None
        }
    };

    match c {
        'x' => Some((char::from_u32(hex(1..3)?)?, 3)),
        'u' if bytes.get(1) == Some(&b'{') => {
            let digits = bytes[2..].iter().take(6).take_while(|b| b.is_ascii_hexdigit()).count();
            if digits == 0 || bytes.get(2 + digits) != Some(&b'}') {
                return None;
            }
            Some((char::from_u32(hex(2..2 + digits)?)?, digits + 3))
        }
        'u' => Some((char::from_u32(hex(1..5)?)?, 5)),
        _ => None,
    }
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Walks the source once, tracking the line and the char-based column of the next byte.
struct Cursor<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: usize,
    module_id: ModuleId,
    /// Byte offset from which no `}` exists, so later `${` scans can stop early.
    no_closing_brace_from: usize,
}

/// Saved cursor state for backtracking.
#[derive(Clone, Copy)]
struct Mark {
    pos: usize,
    line: u32,
    col: usize,
}

impl<'a> Cursor<'a> {
    fn new(src: &'a str, module_id: ModuleId) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            module_id,
            no_closing_brace_from: usize::MAX,
        }
    }

    #[inline(always)]
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    #[inline(always)]
    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    #[inline(always)]
    fn position(&self) -> Position {
        Position {
            line: self.line,
            column: self.col,
        }
    }

    fn mark(&self) -> Mark {
        Mark {
            pos: self.pos,
            line: self.line,
            col: self.col,
        }
    }

    fn reset(&mut self, mark: Mark) {
        self.pos = mark.pos;
        self.line = mark.line;
        self.col = mark.col;
    }

    /// Moves to the absolute byte offset `end`, updating the line and column.
    fn advance(&mut self, end: usize) {
        for &b in &self.bytes[self.pos..end] {
            if b == b'\n' {
                self.line += 1;
                self.col = 1;
            } else if b & 0xC0 != 0x80 {
                self.col += 1;
            }
        }
        self.pos = end;
    }

    /// Moves over `len` ASCII bytes that contain no newline.
    #[inline(always)]
    fn advance_ascii(&mut self, len: usize) {
        self.pos += len;
        self.col += len;
    }

    /// Range of the unconsumed input, as reported for the end-of-input token.
    fn rest_range(&self) -> Range {
        let start = self.position();
        let fragment = self.rest();
        let fragment = if !fragment.starts_with(' ') && fragment.ends_with(' ') {
            fragment.trim()
        } else {
            fragment
        };
        Range {
            start,
            end: Position {
                line: start.line,
                column: start.column + fragment.chars().count(),
            },
        }
    }

    /// Builds a token that spans from the cursor to the byte offset `end` on its first line.
    fn spanned(&mut self, end: usize, kind: TokenKind) -> Token {
        let start = self.position();
        let columns = self.src[self.pos..end].chars().count();
        self.advance(end);
        Token {
            range: Range {
                start,
                end: Position {
                    line: start.line,
                    column: start.column + columns,
                },
            },
            kind,
            module_id: self.module_id,
        }
    }

    /// Builds a token for `len` ASCII bytes with no newline.
    #[inline(always)]
    fn simple(&mut self, len: usize, kind: TokenKind) -> Token {
        let start = self.position();
        self.advance_ascii(len);
        Token {
            range: Range {
                start,
                end: Position {
                    line: start.line,
                    column: start.column + len,
                },
            },
            kind,
            module_id: self.module_id,
        }
    }

    /// Builds a token that spans from `start` to the cursor, which may cross lines.
    fn token_from(&self, start: Position, kind: TokenKind) -> Token {
        Token {
            range: Range {
                start,
                end: self.position(),
            },
            kind,
            module_id: self.module_id,
        }
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    /// Skips spaces, tabs, line breaks and `#` comments in one pass.
    fn skip_whitespace_and_comments(&mut self) {
        let (mut i, mut line, mut col) = (self.pos, self.line, self.col);

        loop {
            match self.bytes.get(i) {
                Some(b' ' | b'\t' | b'\r') => {
                    i += 1;
                    col += 1;
                }
                Some(b'\n') => {
                    i += 1;
                    line += 1;
                    col = 1;
                }
                Some(b'#') => {
                    let len = Self::comment_len(&self.bytes[i..]);
                    col += self.src[i..i + len].chars().count();
                    i += len;
                }
                _ => break,
            }
        }

        self.pos = i;
        self.line = line;
        self.col = col;
    }

    /// Length of the comment text that starts at `bytes[0]`, up to the line break.
    #[inline(always)]
    fn comment_len(bytes: &[u8]) -> usize {
        memchr::memchr2(b'\n', b'\r', bytes).unwrap_or(bytes.len())
    }

    /// Like `advance`, for text without a newline but possibly non-ASCII.
    fn advance_text(&mut self, end: usize) {
        self.col += self.src[self.pos..end].chars().count();
        self.pos = end;
    }

    fn token_include_spaces(&mut self) -> Option<Token> {
        match self.peek(0)? {
            b'\n' => Some(self.spanned(self.pos + 1, TokenKind::NewLine)),
            b'\r' => {
                if self.peek(1) == Some(b'\n') {
                    Some(self.spanned(self.pos + 2, TokenKind::NewLine))
                } else {
                    None
                }
            }
            b' ' => {
                let len = self.bytes[self.pos..].iter().take_while(|&&b| b == b' ').count();
                Some(self.simple(len, TokenKind::Whitespace(len)))
            }
            b'\t' => {
                let len = self.bytes[self.pos..].iter().take_while(|&&b| b == b'\t').count();
                Some(self.simple(len, TokenKind::Tab(len)))
            }
            b'#' => Some(self.inline_comment()),
            _ => self.token(),
        }
    }

    fn inline_comment(&mut self) -> Token {
        self.advance_ascii(1);
        let start = self.position();
        let len = Self::comment_len(&self.bytes[self.pos..]);
        let text = self.src[self.pos..self.pos + len].to_string();
        self.advance_text(self.pos + len);
        self.token_from(start, TokenKind::Comment(text))
    }

    /// Consumes up to the next whitespace or delimiter as a single `Unknown` token.
    fn unknown(&mut self) -> Token {
        let first = self.rest().chars().next().map_or(0, char::len_utf8);
        let tail = self.src[self.pos + first..]
            .find(|c: char| c.is_whitespace() || ",()[]{}|;".contains(c))
            .unwrap_or(self.src.len() - self.pos - first);
        let end = self.pos + first + tail;
        let text = self.src[self.pos..end].to_string();
        self.spanned(end, TokenKind::Unknown(text))
    }

    fn token(&mut self) -> Option<Token> {
        let c = self.peek(0)?;
        match c {
            b'$' => self.env(),
            b'"' => self.string_or_empty(),
            b'0'..=b'9' => match self.number_scan() {
                NumberScan::Match(len, value) => self.number(len, value),
                NumberScan::NoMatch | NumberScan::Fatal => None,
            },
            b'-' => match self.number_scan() {
                NumberScan::Match(len, value) => self.number(len, value),
                NumberScan::Fatal => None,
                NumberScan::NoMatch => Some(match self.peek(1) {
                    Some(b'>') => self.simple(2, TokenKind::Arrow),
                    Some(b'=') => self.simple(2, TokenKind::MinusEqual),
                    _ => self.simple(1, TokenKind::Minus),
                }),
            },
            b'.' => match self.number_scan() {
                NumberScan::Match(len, value) => self.number(len, value),
                NumberScan::Fatal => None,
                NumberScan::NoMatch => {
                    if self.starts_with("...") {
                        Some(self.simple(3, TokenKind::DotDotDot))
                    } else if self.starts_with("..") {
                        Some(self.simple(2, TokenKind::DoubleDot))
                    } else {
                        Some(self.selector())
                    }
                }
            },
            b'b' => {
                if let Some(token) = self.byte_string() {
                    Some(token)
                } else {
                    Some(self.ident_or_keyword())
                }
            }
            b's' => {
                if let Some(token) = self.interpolated_string() {
                    Some(token)
                } else {
                    Some(self.ident_or_keyword())
                }
            }
            b'(' => Some(self.simple(1, TokenKind::LParen)),
            b')' => Some(self.simple(1, TokenKind::RParen)),
            b'{' => Some(self.simple(1, TokenKind::LBrace)),
            b'}' => Some(self.simple(1, TokenKind::RBrace)),
            b'[' => Some(self.simple(1, TokenKind::LBracket)),
            b']' => Some(self.simple(1, TokenKind::RBracket)),
            b',' => Some(self.simple(1, TokenKind::Comma)),
            b';' => Some(self.simple(1, TokenKind::SemiColon)),
            b':' => Some(self.one_or_two(b':', TokenKind::DoubleColon, TokenKind::Colon)),
            b'?' => Some(self.one_or_two(b'?', TokenKind::Coalesce, TokenKind::Question)),
            b'|' => Some(match self.peek(1) {
                Some(b'=') => self.simple(2, TokenKind::PipeEqual),
                Some(b'|') => self.simple(2, TokenKind::Or),
                _ => self.simple(1, TokenKind::Pipe),
            }),
            b'!' => Some(match self.peek(1) {
                Some(b'=') => self.simple(2, TokenKind::NeEq),
                Some(b'~') => self.simple(2, TokenKind::NotTildeEqual),
                _ => self.simple(1, TokenKind::Not),
            }),
            b'<' => Some(match self.peek(1) {
                Some(b'<') => self.simple(2, TokenKind::LeftShift),
                Some(b'=') => self.simple(2, TokenKind::Lte),
                _ => self.simple(1, TokenKind::Lt),
            }),
            b'>' => Some(match self.peek(1) {
                Some(b'>') => self.simple(2, TokenKind::RightShift),
                Some(b'=') => self.simple(2, TokenKind::Gte),
                _ => self.simple(1, TokenKind::Gt),
            }),
            b'=' => Some(match self.peek(1) {
                Some(b'=') => self.simple(2, TokenKind::EqEq),
                Some(b'~') => self.simple(2, TokenKind::TildeEqual),
                _ => self.simple(1, TokenKind::Equal),
            }),
            b'+' => match self.number_scan() {
                NumberScan::Match(len, value) => self.number(len, value),
                NumberScan::Fatal => None,
                NumberScan::NoMatch => Some(self.one_or_two(b'=', TokenKind::PlusEqual, TokenKind::Plus)),
            },
            b'*' => Some(self.one_or_two(b'=', TokenKind::StarEqual, TokenKind::Asterisk)),
            b'/' => Some(match (self.peek(1), self.peek(2)) {
                (Some(b'='), _) => self.simple(2, TokenKind::SlashEqual),
                (Some(b'/'), Some(b'=')) => self.simple(3, TokenKind::DoubleSlashEqual),
                _ => self.simple(1, TokenKind::Slash),
            }),
            b'%' => Some(self.one_or_two(b'=', TokenKind::PercentEqual, TokenKind::Percent)),
            b'&' => {
                if self.starts_with("&&") {
                    Some(self.simple(2, TokenKind::And))
                } else {
                    None
                }
            }
            b'@' => Some(self.simple(1, TokenKind::Convert)),
            c if c.is_ascii_alphabetic() || c == b'_' => Some(self.ident_or_keyword()),
            _ => None,
        }
    }

    /// Emits `two` if the byte after the current one is `second`, else `one`.
    #[inline(always)]
    fn one_or_two(&mut self, second: u8, two: TokenKind, one: TokenKind) -> Token {
        if self.peek(1) == Some(second) {
            self.simple(2, two)
        } else {
            self.simple(1, one)
        }
    }

    fn env(&mut self) -> Option<Token> {
        let name_len = self.bytes[self.pos + 1..]
            .iter()
            .take_while(|&&b| is_word_byte(b))
            .count();
        if name_len == 0 {
            return None;
        }
        self.advance_ascii(1);
        let name = SmolStr::new(&self.src[self.pos..self.pos + name_len]);
        Some(self.simple(name_len, TokenKind::Env(name)))
    }

    fn number_scan(&self) -> NumberScan {
        let b = &self.bytes[self.pos..];
        let digits = |from: usize| b[from.min(b.len())..].iter().take_while(|b| b.is_ascii_digit()).count();
        let mut i = 0;

        if b.get(i) == Some(&b'-') {
            i += 1;
        }
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }

        let int_digits = digits(i);
        if int_digits > 0 {
            i += int_digits;
            if b.get(i) == Some(&b'.') {
                let frac = digits(i + 1);
                if frac > 0 {
                    i += 1 + frac;
                }
            }
        } else if b.get(i) == Some(&b'.') && digits(i + 1) > 0 {
            i += 1 + digits(i + 1);
        } else {
            return NumberScan::NoMatch;
        }

        if matches!(b.get(i), Some(b'e' | b'E')) {
            let mut j = i + 1;
            if matches!(b.get(j), Some(b'+' | b'-')) {
                j += 1;
            }
            let exp = digits(j);
            if exp == 0 {
                return NumberScan::Fatal;
            }
            i = j + exp;
        }

        match self.src[self.pos..self.pos + i].parse::<f64>() {
            Ok(value) => NumberScan::Match(i, value),
            Err(_) => NumberScan::NoMatch,
        }
    }

    fn number(&mut self, len: usize, value: f64) -> Option<Token> {
        Some(self.simple(len, TokenKind::NumberLiteral(Number::new(value))))
    }

    fn selector(&mut self) -> Token {
        let rest = self.rest();
        let bytes = rest.as_bytes();
        let mut len = 1;

        match bytes.get(1) {
            Some(b'>' | b'^') => len = 2,
            Some(b'"') => {
                if let Some(quoted) = Self::quoted_selector_len(rest) {
                    len = quoted;
                }
            }
            _ => {
                len += bytes[1..]
                    .iter()
                    .take_while(|&&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'*'))
                    .count();
            }
        }

        let text = SmolStr::new(&rest[..len]);
        self.spanned(self.pos + len, TokenKind::Selector(text))
    }

    /// Length of a quoted selector such as `."key"` at the start of `rest`, or `None` if unterminated.
    fn quoted_selector_len(rest: &str) -> Option<usize> {
        let mut chars = rest[2..].char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' => {
                    chars.next()?;
                }
                '"' => return Some(2 + i + 1),
                _ => {}
            }
        }
        None
    }

    fn ident_or_keyword(&mut self) -> Token {
        let rest = self.rest();
        let bytes = rest.as_bytes();
        let base_len = bytes.iter().take_while(|&&b| is_word_byte(b)).count();
        let base = &rest[..base_len];
        let next = rest[base_len..].chars().next();
        let at_word_boundary = next.map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true);

        if at_word_boundary && let Some(kind) = keyword_kind(base) {
            return self.simple(base_len, kind);
        }

        if matches!(next, Some('-' | '*')) {
            let full_len = bytes
                .iter()
                .take_while(|&&b| is_word_byte(b) || matches!(b, b'-' | b'*'))
                .count();
            return self.simple(full_len, ident_kind(&rest[..full_len]));
        }

        self.simple(base_len, ident_kind(base))
    }

    fn string_or_empty(&mut self) -> Option<Token> {
        if self.starts_with("\"\"") {
            return Some(self.simple(2, TokenKind::StringLiteral(String::new())));
        }

        let mark = self.mark();
        let start = self.position();
        self.advance_ascii(1);
        let Some((text, end)) = self.escaped_text(|c| c != '"' && c != '\\', true) else {
            self.reset(mark);
            return None;
        };
        if self.bytes.get(end) != Some(&b'"') {
            self.reset(mark);
            return None;
        }
        self.advance(end + 1);
        Some(self.token_from(start, TokenKind::StringLiteral(text)))
    }

    /// Reads text made of `normal` characters and `\` escapes starting at the cursor and returns it
    /// with the byte offset where it stops. A first character that is neither is an error.
    fn escaped_text(&self, normal: impl Fn(char) -> bool, plain_string_escapes: bool) -> Option<(String, usize)> {
        let rest = self.rest();
        let mut text = String::new();
        let mut index = 0;

        while index < rest.len() {
            let c = rest[index..].chars().next()?;
            if normal(c) {
                text.push(c);
                index += c.len_utf8();
            } else if c == '\\' {
                let next = index + 1;
                if next >= rest.len() {
                    return None;
                }
                let (decoded, used) = decode_escape(&rest[next..], plain_string_escapes)?;
                text.push(decoded);
                index = next + used;
            } else if index == 0 {
                return None;
            } else {
                break;
            }
        }

        Some((text, self.pos + index))
    }

    fn byte_string(&mut self) -> Option<Token> {
        if !self.starts_with("b\"") {
            return None;
        }

        let rest = self.rest();
        let bytes = rest.as_bytes();
        let mut value = Vec::new();
        let mut i = 2;

        loop {
            match *bytes.get(i)? {
                b'"' => break,
                b'\\' => {
                    let (byte, used) = match *bytes.get(i + 1)? {
                        b'x' => {
                            let digits = bytes.get(i + 2..i + 4)?;
                            if !digits.iter().all(u8::is_ascii_hexdigit) {
                                return None;
                            }
                            (u8::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()?, 4)
                        }
                        b'\\' => (b'\\', 2),
                        b'"' => (b'"', 2),
                        b'n' => (b'\n', 2),
                        b'r' => (b'\r', 2),
                        b't' => (b'\t', 2),
                        b'0' => (0, 2),
                        _ => return None,
                    };
                    value.push(byte);
                    i += used;
                }
                b if b.is_ascii() => {
                    value.push(b);
                    i += 1;
                }
                _ => return None,
            }
        }

        let start = self.position();
        self.advance(self.pos + i + 1);
        Some(self.token_from(start, TokenKind::BytesLiteral(value)))
    }

    fn interpolated_string(&mut self) -> Option<Token> {
        if !self.starts_with("s\"") {
            return None;
        }

        let mark = self.mark();
        let start = self.position();
        self.advance_ascii(2);

        let mut segments = Vec::with_capacity(4);
        match self.string_segment() {
            Some(segment) => segments.push(segment),
            None => {
                self.reset(mark);
                return None;
            }
        }
        while !self.rest().is_empty() {
            match self.string_segment() {
                Some(segment) => segments.push(segment),
                None => break,
            }
        }

        if self.peek(0) != Some(b'"') {
            self.reset(mark);
            return None;
        }
        self.advance_ascii(1);
        Some(self.token_from(start, TokenKind::InterpolatedString(segments)))
    }

    /// Offset of the first `}` after the `${` at the cursor, relative to the text after `${`.
    fn find_closing_brace(&mut self) -> Option<usize> {
        let from = self.pos + 2;
        if from >= self.no_closing_brace_from {
            return None;
        }
        let close = memchr::memchr(b'}', &self.bytes[from..]);
        if close.is_none() {
            self.no_closing_brace_from = from;
        }
        close
    }

    /// Reads one segment of an interpolated string, leaving the cursor unchanged on failure.
    fn string_segment(&mut self) -> Option<StringSegment> {
        let start = self.position();

        if self.starts_with("${")
            && let Some(close) = self.find_closing_brace()
        {
            let expr = SmolStr::new(&self.rest()[2..2 + close]);
            self.advance(self.pos + 2 + close + 1);
            return Some(StringSegment::Expr(
                expr,
                Range {
                    start,
                    end: self.position(),
                },
            ));
        }

        if let Some((text, end)) = self.escaped_text(|c| !matches!(c, '"' | '\\' | '$' | '{'), false) {
            self.advance(end);
            return Some(StringSegment::Text(
                text,
                Range {
                    start,
                    end: self.position(),
                },
            ));
        }

        if self.starts_with("$$") {
            self.advance_ascii(2);
            return Some(StringSegment::Text(
                "$".to_string(),
                Range {
                    start,
                    end: self.position(),
                },
            ));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use crate::range::Position;

    use super::*;
    use proptest::proptest;
    use rstest::rstest;

    #[rstest]
    #[case("and(contains(\"test\"))",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 13} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 20} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 20}, end: Position {line: 1, column: 21} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 22} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("and(contains(\"test\")) | or(endswith(\"test\"))",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 13} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 20} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 20}, end: Position {line: 1, column: 21} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 23}, end: Position {line: 1, column: 24} }, kind: TokenKind::Pipe, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 25}, end: Position {line: 1, column: 27} }, kind: TokenKind::Ident(SmolStr::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 27}, end: Position {line: 1, column: 28} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 28}, end: Position {line: 1, column: 36} }, kind: TokenKind::Ident(SmolStr::new("endswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 36}, end: Position {line: 1, column: 37} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 37}, end: Position {line: 1, column: 43} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 43}, end: Position {line: 1, column: 44} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 44}, end: Position {line: 1, column: 45} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 45}, end: Position {line: 1, column: 45} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("eq(length(), 10)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(SmolStr::new("eq")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 10} }, kind: TokenKind::Ident(SmolStr::new("length")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 11} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 16} }, kind: TokenKind::NumberLiteral(10.into()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 16}, end: Position {line: 1, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 17}, end: Position {line: 1, column: 17} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("or(.h1, .**)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(SmolStr::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 7} }, kind: TokenKind::Selector(SmolStr::new(".h1")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 8} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 12} }, kind: TokenKind::Selector(SmolStr::new(".**")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 13} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("or(.[][], .[])",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(SmolStr::new("or")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Selector(SmolStr::new(".")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 7} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 8} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 8}, end: Position {line: 1, column: 9} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::Selector(SmolStr::new(".")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::LBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 14} }, kind: TokenKind::RBracket, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("startswith(\"\\u{0061}\")",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 11} }, kind: TokenKind::Ident(SmolStr::new("startswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 12} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 22} }, kind: TokenKind::StringLiteral("a".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 23} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 23}, end: Position {line: 1, column: 23} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("endswith($ENV)",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(SmolStr::new("endswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 14} }, kind: TokenKind::Env(SmolStr::new("ENV")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("def check(arg1, arg2): startswith(\"\\u{0061}\")",
        Options::default(),
        Ok(vec![
          Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Def, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 10} }, kind: TokenKind::Ident(SmolStr::new("check")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 11} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 15} }, kind: TokenKind::Ident(SmolStr::new("arg1")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 16} }, kind: TokenKind::Comma, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 17}, end: Position {line: 1, column: 21} }, kind: TokenKind::Ident(SmolStr::new("arg2")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 23} }, kind: TokenKind::Colon, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 24}, end: Position {line: 1, column: 34} }, kind: TokenKind::Ident(SmolStr::new("startswith")), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 34}, end: Position {line: 1, column: 35} }, kind: TokenKind::LParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 35}, end: Position {line: 1, column: 45} }, kind: TokenKind::StringLiteral("a".to_string()), module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 45}, end: Position {line: 1, column: 46} }, kind: TokenKind::RParen, module_id: 1.into()},
          Token{range: Range { start: Position {line: 1, column: 46}, end: Position {line: 1, column: 46} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("\"test",
          Options::default(),
          Err(SyntaxError::UnexpectedToken(Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} }, kind: TokenKind::Eof, module_id: 1.into()})))]
    #[case::new_line("and(\ncontains(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 1}, end: Position {line: 2, column: 9} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 9}, end: Position {line: 2, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 10}, end: Position {line: 2, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 16}, end: Position {line: 2, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 17}, end: Position {line: 2, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 18}, end: Position {line: 2, column: 18} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("and(\ncontains(\"test\")) | or(\nendswith(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 1}, end: Position {line: 2, column: 9} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 9}, end: Position {line: 2, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 10}, end: Position {line: 2, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 16}, end: Position {line: 2, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 17}, end: Position {line: 2, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 18}, end: Position {line: 2, column: 19} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 19}, end: Position {line: 2, column: 20} }, kind: TokenKind::Pipe, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 20}, end: Position {line: 2, column: 21} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 21}, end: Position {line: 2, column: 23} }, kind: TokenKind::Ident(SmolStr::new("or")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 23}, end: Position {line: 2, column: 24} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 2, column: 24}, end: Position {line: 2, column: 25} }, kind: TokenKind::NewLine, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 1}, end: Position {line: 3, column: 9} }, kind: TokenKind::Ident(SmolStr::new("endswith")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 9}, end: Position {line: 3, column: 10} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 10}, end: Position {line: 3, column: 16} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 16}, end: Position {line: 3, column: 17} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 17}, end: Position {line: 3, column: 18} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 3, column: 18}, end: Position {line: 3, column: 18} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::tab("and(\tcontains(\"test\"))",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("and")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Tab(1), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(SmolStr::new("contains")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 21} }, kind: TokenKind::StringLiteral("test".to_string()), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 21}, end: Position {line: 1, column: 22} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 22}, end: Position {line: 1, column: 23} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 23}, end: Position {line: 1, column: 23} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::interpolated_string("s\"test${val1}test\n\"",
            Options{include_spaces: true, ignore_errors: true},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 2, column: 2} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("test".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 7} }),
                            StringSegment::Expr("val1".to_string().into(), Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }),
                            StringSegment::Text("test\n".to_string(), Range { start: Position {line: 1, column: 14}, end: Position {line: 2, column: 1 }})
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 2, column: 2}, end: Position {line: 2, column: 2} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::error("\"test",
            Options{include_spaces: false, ignore_errors: false},
            Err(SyntaxError::UnexpectedToken(Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} }, kind: TokenKind::Eof, module_id: 1.into()})))]
    #[case::error("s\"$$${test}$$\"",
            Options{include_spaces: false, ignore_errors: false},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 15} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("$".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 5} }),
                            StringSegment::Expr("test".to_string().into(), Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 12} }),
                            StringSegment::Text("$".to_string(), Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 14 }})
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::function_declaration("fn(): program;",
            Options::default(),
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Fn, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Colon, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(SmolStr::new("program")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 15} }, kind: TokenKind::SemiColon, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 15} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::end_keyword("end",
            Options::default(),
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::End, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::function_declaration_with_end("fn(): program end",
            Options::default(),
            Ok(vec![
              Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Fn, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::LParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::RParen, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Colon, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 14} }, kind: TokenKind::Ident(SmolStr::new("program")), module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 15}, end: Position {line: 1, column: 18} }, kind: TokenKind::End, module_id: 1.into()},
              Token{range: Range { start: Position {line: 1, column: 18}, end: Position {line: 1, column: 18} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq1("==",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::EqEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 3} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq2("=",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 2} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq3("===",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::EqEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq4("== =",
              Options{include_spaces: true, ignore_errors: false},
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::EqEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::eq_eq5("== =",
              Options{include_spaces: false, ignore_errors: false}, // Default options ignore spaces between tokens
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::EqEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::ne_eq1("!=",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::NeEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 3} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::ne_eq2("!==",
              Options::default(),
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::NeEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::ne_eq3("!= =",
              Options{include_spaces: true, ignore_errors: false},
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::NeEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Whitespace(1), module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::ne_eq4("!= =",
              Options{include_spaces: false, ignore_errors: false}, // Default options ignore spaces between tokens
              Ok(vec![
                  Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::NeEq, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Equal, module_id: 1.into()},
                  Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("{}",
            Options::default(),
            Ok(vec![
                Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::LBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 3} }, kind: TokenKind::RBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 3} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case(" { } ",
            Options::default(),
            Ok(vec![
                Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 3} }, kind: TokenKind::LBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::RBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 6}, end: Position {line: 1, column: 6} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case("{key: value}", // Adjusted to match LBrace/RBrace being {{ and }}
            Options::default(),
            Ok(vec![
                Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::LBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 2}, end: Position {line: 1, column: 5} }, kind: TokenKind::Ident(SmolStr::new("key")), module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 6} }, kind: TokenKind::Colon, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 12} }, kind: TokenKind::Ident(SmolStr::new("value")), module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 12}, end: Position {line: 1, column: 13} }, kind: TokenKind::RBrace, module_id: 1.into()},
                Token{range: Range { start: Position {line: 1, column: 13}, end: Position {line: 1, column: 13} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::selector_with_dot_h_text(".h.text",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Selector(SmolStr::new(".h")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 8 } },
                        kind: TokenKind::Selector(SmolStr::new(".text")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 8 }, end: Position { line: 1, column: 8 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::selector_with_dot_h_level(".h.level",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Selector(SmolStr::new(".h")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 9 } },
                        kind: TokenKind::Selector(SmolStr::new(".level")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 9 }, end: Position { line: 1, column: 9 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::selector_blockquote_alias(".>",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Selector(SmolStr::new(".>")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::selector_footnote_alias(".^",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Selector(SmolStr::new(".^")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 3 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::selector_blockquote_in_expression("select(.>)",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 7 } },
                        kind: TokenKind::Ident(SmolStr::new("select")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 7 }, end: Position { line: 1, column: 8 } },
                        kind: TokenKind::LParen,
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 8 }, end: Position { line: 1, column: 10 } },
                        kind: TokenKind::Selector(SmolStr::new(".>")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 10 }, end: Position { line: 1, column: 11 } },
                        kind: TokenKind::RParen,
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 11 }, end: Position { line: 1, column: 11 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::hex_escape_sequence("print(\"\\x1b[2J\\x1b[H\")",
            Options::default(),
            Ok(vec![
                    Token {
                        range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 6 } },
                        kind: TokenKind::Ident(SmolStr::new("print")),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 6 }, end: Position { line: 1, column: 7 } },
                        kind: TokenKind::LParen,
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 7 }, end: Position { line: 1, column: 22 } },
                        kind: TokenKind::StringLiteral("\x1b[2J\x1b[H".to_string()),
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 22 }, end: Position { line: 1, column: 23 } },
                        kind: TokenKind::RParen,
                        module_id: 1.into(),
                    },
                    Token {
                        range: Range { start: Position { line: 1, column: 23 }, end: Position { line: 1, column: 23 } },
                        kind: TokenKind::Eof,
                        module_id: 1.into(),
                    }
                ])
            )]
    #[case::keyword_boundary_def("definition",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 11} }, kind: TokenKind::Ident(SmolStr::new("definition")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 11} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_end("ending",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 7} }, kind: TokenKind::Ident(SmolStr::new("ending")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 7} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_if("ifconfig",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(SmolStr::new("ifconfig")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 9} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_yield("yielding",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(SmolStr::new("yielding")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 9} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_proper_def("def ",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Def, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_proper_end("end ",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::End, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_proper_yield("yield ",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 6} }, kind: TokenKind::Yield, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 7} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    // Non-ASCII alphanumeric after an ASCII keyword base must block the keyword match.
    // "defä" must not lex as keyword Def; the ASCII portion becomes Ident("def") instead.
    #[case::keyword_boundary_non_ascii_def("defä",
        Options{ignore_errors: true, include_spaces: false},
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("def")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::keyword_boundary_non_ascii_if("ifé",
        Options{ignore_errors: true, include_spaces: false},
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Ident(SmolStr::new("if")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::number_regex("\"^(-?(?:0|[1-9]\\\\d*)(?:\\\\.\\\\d+)?(?:[eE][+-]?\\\\d+)?)\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 53 } },
                kind: TokenKind::StringLiteral("^(-?(?:0|[1-9]\\d*)(?:\\.\\d+)?(?:[eE][+-]?\\d+)?)".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 53 }, end: Position { line: 1, column: 53 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::regex_with_brackets("\"[a-zA-Z0-9]+\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 15 } },
                kind: TokenKind::StringLiteral("[a-zA-Z0-9]+".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 15 }, end: Position { line: 1, column: 15 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::regex_with_escaped_chars("\"\\\\[\\\\(\\\\)\\\\{\\\\}\\\\+\\\\*\\\\?\\\\^\\\\$\\\\|\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 36 } },
                kind: TokenKind::StringLiteral("\\[\\(\\)\\{\\}\\+\\*\\?\\^\\$\\|".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 36 }, end: Position { line: 1, column: 36 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::regex_character_classes("\"\\s\\S\\d\\D\\w\\W\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 15 } },
                kind: TokenKind::StringLiteral("sSdDwW".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 15 }, end: Position { line: 1, column: 15 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::regex_mixed_with_character_classes("\"[a-z]\\d+\\s*\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 14 } },
                kind: TokenKind::StringLiteral("[a-z]d+s*".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 14 }, end: Position { line: 1, column: 14 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::pipe_with_comment("| \"test\" # comment",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 2 } },
                kind: TokenKind::Pipe,
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::StringLiteral("test".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 19 }, end: Position { line: 1, column: 19 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::comment_with_pipe_character("# comment with | pipe",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 22 }, end: Position { line: 1, column: 22 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::comment_with_empty_line("#\n# test",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 2, column: 7 }, end: Position { line: 2, column: 7 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::comment_hash_only("#",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 2 }, end: Position { line: 1, column: 2 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::interpolated_string_with_escaped_braces("s\"test\\{escaped\\}\"",
            Options{include_spaces: false, ignore_errors: false},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 19} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("test{escaped}".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 18} })
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 1, column: 19}, end: Position {line: 1, column: 19} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::interpolated_string_mixed_escape_and_expr("s\"\\{${var}\\}\"",
            Options{include_spaces: false, ignore_errors: false},
            Ok(vec![Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 14} },
                          kind: TokenKind::InterpolatedString(vec![
                            StringSegment::Text("{".to_string(), Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 5} }),
                            StringSegment::Expr("var".to_string().into(), Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 11} }),
                            StringSegment::Text("}".to_string(), Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 13} })
                          ]), module_id: 1.into()},
                   Token{range: Range { start: Position {line: 1, column: 14}, end: Position {line: 1, column: 14} }, kind: TokenKind::Eof, module_id: 1.into()}]
                ))]
    #[case::unicode4_hiragana("\"\\u3041\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::StringLiteral("ぁ".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 9 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::unicode4_katakana("\"\\u30A1\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::StringLiteral("ァ".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 9 }, end: Position { line: 1, column: 9 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::unicode4_in_regex_char_class("\"[\\u3041-\\u3096]+\"",
        Options::default(),
        Ok(vec![
            Token {
                range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 19 } },
                kind: TokenKind::StringLiteral("[ぁ-ゖ]+".to_string()),
                module_id: 1.into(),
            },
            Token {
                range: Range { start: Position { line: 1, column: 19 }, end: Position { line: 1, column: 19 } },
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }
        ])
    )]
    #[case::unterminated_string_reports_position("\"unterminated",
        Options::default(),
        Err(SyntaxError::UnexpectedToken(Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 14} }, kind: TokenKind::Eof, module_id: 1.into()})))]
    #[case::arrow("->",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 3} }, kind: TokenKind::Arrow, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 3} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::arrow_in_expression("map(->(x):upcase;)",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 4} }, kind: TokenKind::Ident(SmolStr::new("map")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 5} }, kind: TokenKind::LParen, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 5}, end: Position {line: 1, column: 7} }, kind: TokenKind::Arrow, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 7}, end: Position {line: 1, column: 8} }, kind: TokenKind::LParen, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 8}, end: Position {line: 1, column: 9} }, kind: TokenKind::Ident(SmolStr::new("x")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 9}, end: Position {line: 1, column: 10} }, kind: TokenKind::RParen, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 10}, end: Position {line: 1, column: 11} }, kind: TokenKind::Colon, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 11}, end: Position {line: 1, column: 17} }, kind: TokenKind::Ident(SmolStr::new("upcase")), module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 17}, end: Position {line: 1, column: 18} }, kind: TokenKind::SemiColon, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 18}, end: Position {line: 1, column: 19} }, kind: TokenKind::RParen, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 19}, end: Position {line: 1, column: 19} }, kind: TokenKind::Eof, module_id: 1.into()}]))]
    #[case::arrow_not_minus("- >",
        Options::default(),
        Ok(vec![
            Token{range: Range { start: Position {line: 1, column: 1}, end: Position {line: 1, column: 2} }, kind: TokenKind::Minus, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 3}, end: Position {line: 1, column: 4} }, kind: TokenKind::Gt, module_id: 1.into()},
            Token{range: Range { start: Position {line: 1, column: 4}, end: Position {line: 1, column: 4} }, kind: TokenKind::Eof, module_id: 1.into()}]))]

    fn test_parse(#[case] input: &str, #[case] options: Options, #[case] expected: Result<Vec<Token>, SyntaxError>) {
        assert_eq!(Lexer::new(options).tokenize(input, 1.into()), expected);
    }

    #[rstest]
    #[case::basic(r#"b"abc""#,
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 7 } },
                kind: TokenKind::BytesLiteral(vec![97, 98, 99]), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 7 }, end: Position { line: 1, column: 7 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    #[case::hex_escape(r#"b"\xf0\x9f""#,
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 12 } },
                kind: TokenKind::BytesLiteral(vec![0xf0, 0x9f]), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 12 }, end: Position { line: 1, column: 12 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    #[case::standard_escapes(r#"b"\n\r\t\\""#,
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 12 } },
                kind: TokenKind::BytesLiteral(vec![b'\n', b'\r', b'\t', b'\\']), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 12 }, end: Position { line: 1, column: 12 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    #[case::empty(r#"b"""#,
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 4 } },
                kind: TokenKind::BytesLiteral(vec![]), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 4 }, end: Position { line: 1, column: 4 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    // Non-ASCII inside b"..." fails to parse as a byte literal.
    // The tokenizer falls back: `b` becomes Ident and `"é"` becomes StringLiteral.
    // Higher-level parsing then rejects the invalid expression.
    #[case::non_ascii_not_a_byte_literal(
        "b\"\u{00e9}\"",
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 2 } },
                kind: TokenKind::Ident(SmolStr::new("b")), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 2 }, end: Position { line: 1, column: 5 } },
                kind: TokenKind::StringLiteral("\u{00e9}".to_string()), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 5 }, end: Position { line: 1, column: 5 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    #[case::b_ident_without_quote("b foo",
        Options::default(),
        Ok(vec![
            Token { range: Range { start: Position { line: 1, column: 1 }, end: Position { line: 1, column: 2 } },
                kind: TokenKind::Ident(SmolStr::new("b")), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 3 }, end: Position { line: 1, column: 6 } },
                kind: TokenKind::Ident(SmolStr::new("foo")), module_id: 1.into() },
            Token { range: Range { start: Position { line: 1, column: 6 }, end: Position { line: 1, column: 6 } },
                kind: TokenKind::Eof, module_id: 1.into() },
        ])
    )]
    fn test_byte_string_literal(
        #[case] input: &str,
        #[case] options: Options,
        #[case] expected: Result<Vec<Token>, SyntaxError>,
    ) {
        assert_eq!(Lexer::new(options).tokenize(input, 1.into()), expected);
    }

    const FILES: &[(&str, &str)] = &[
        ("builtin.mq", include_str!("../builtin.mq")),
        ("builtin_tests.mq", include_str!("../builtin_tests.mq")),
        ("modules/cbor.mq", include_str!("../modules/cbor.mq")),
        ("modules/csv.mq", include_str!("../modules/csv.mq")),
        ("modules/csv_test.mq", include_str!("../modules/csv_test.mq")),
        ("modules/fuzzy.mq", include_str!("../modules/fuzzy.mq")),
        ("modules/fuzzy_test.mq", include_str!("../modules/fuzzy_test.mq")),
        ("modules/gron.mq", include_str!("../modules/gron.mq")),
        ("modules/gron_test.mq", include_str!("../modules/gron_test.mq")),
        ("modules/html.mq", include_str!("../modules/html.mq")),
        ("modules/html_test.mq", include_str!("../modules/html_test.mq")),
        ("modules/json.mq", include_str!("../modules/json.mq")),
        ("modules/json_test.mq", include_str!("../modules/json_test.mq")),
        ("modules/md.mq", include_str!("../modules/md.mq")),
        ("modules/md_test.mq", include_str!("../modules/md_test.mq")),
        ("modules/section.mq", include_str!("../modules/section.mq")),
        ("modules/section_test.mq", include_str!("../modules/section_test.mq")),
        ("modules/semver.mq", include_str!("../modules/semver.mq")),
        ("modules/semver_test.mq", include_str!("../modules/semver_test.mq")),
        ("modules/table.mq", include_str!("../modules/table.mq")),
        ("modules/table_test.mq", include_str!("../modules/table_test.mq")),
        ("modules/test.mq", include_str!("../modules/test.mq")),
        ("modules/toml.mq", include_str!("../modules/toml.mq")),
        ("modules/toml_test.mq", include_str!("../modules/toml_test.mq")),
        ("modules/toon.mq", include_str!("../modules/toon.mq")),
        ("modules/toon_test.mq", include_str!("../modules/toon_test.mq")),
        ("modules/xml.mq", include_str!("../modules/xml.mq")),
        ("modules/xml_test.mq", include_str!("../modules/xml_test.mq")),
        ("modules/yaml.mq", include_str!("../modules/yaml.mq")),
        ("modules/yaml_test.mq", include_str!("../modules/yaml_test.mq")),
    ];

    #[test]
    fn parses_real_mq_files() {
        for (name, source) in FILES {
            let token_arena = crate::Shared::new(crate::SharedCell::new(crate::Arena::new(256)));
            crate::parse(source, token_arena).unwrap_or_else(|e| panic!("{name} failed to parse: {e}"));
        }
    }

    /// Pieces that stress every token kind and its failure paths when glued together.
    const FRAGMENTS: &[&str] = &[
        "def",
        "end",
        "if",
        "elif",
        "else",
        "while",
        "foreach",
        "let",
        "var",
        "fn",
        "do",
        "None",
        "self",
        "nodes",
        "true",
        "false",
        "foo",
        "_bar",
        "a-b",
        "a*",
        "b",
        "s",
        "x1",
        "é",
        "あ",
        "$",
        "$ENV",
        "$a_1",
        "$$",
        "${",
        "}",
        "{",
        "(",
        ")",
        "[",
        "]",
        ",",
        ";",
        ":",
        "::",
        "|",
        "||",
        "|=",
        "&",
        "&&",
        "?",
        "??",
        "!",
        "!=",
        "!~",
        "=",
        "==",
        "=~",
        "<",
        "<<",
        "<=",
        ">",
        ">>",
        ">=",
        "+",
        "+=",
        "-",
        "-=",
        "->",
        "*",
        "*=",
        "/",
        "/=",
        "//=",
        "%",
        "%=",
        "@",
        ".",
        "..",
        "...",
        ".h1",
        ".[]",
        ".>",
        ".^",
        ".\"k\"",
        ".\"k",
        "0",
        "12",
        "-3",
        "+4",
        "1.5",
        ".5",
        "1e",
        "1e5",
        "1E+5",
        "1e-",
        "--5",
        "-+5",
        "5.",
        "\"",
        "\"\"",
        "\"a\"",
        "\"a\\n\"",
        "\"\\q\"",
        "\"\\u{41}\"",
        "\"\\u{110000}\"",
        "\"\\x41\"",
        "\"\\",
        "\\",
        "s\"",
        "s\"a\"",
        "s\"${x}\"",
        "s\"$$\"",
        "s\"\\{\"",
        "s\"a$b\"",
        "b\"",
        "b\"a\"",
        "b\"\\x41\"",
        "b\"\\q\"",
        "b\"é\"",
        "#",
        "# c",
        " ",
        "  ",
        "\t",
        "\n",
        "\r\n",
        "\r",
        "\u{b}",
        "\u{a0}",
        "§",
        "🎉",
        "`",
        "~",
        "^",
        "'",
    ];

    fn fragments_strategy() -> impl proptest::strategy::Strategy<Value = String> {
        use proptest::prelude::*;
        proptest::collection::vec(
            (0..FRAGMENTS.len(), prop_oneof![Just(""), Just(" "), Just("\n")]),
            0..24,
        )
        .prop_map(|parts| {
            parts
                .into_iter()
                .map(|(i, sep)| format!("{}{}", FRAGMENTS[i], sep))
                .collect::<String>()
        })
    }

    /// Lexes `source` with every option combination and checks the structural invariants.
    fn assert_lexes_consistently(source: &str) {
        for (ignore_errors, include_spaces) in [(false, false), (false, true), (true, false), (true, true)] {
            let options = Options {
                ignore_errors,
                include_spaces,
            };
            let Ok(tokens) = Lexer::new(options).tokenize(source, 1.into()) else {
                continue;
            };

            assert_eq!(tokens.last().map(|t| &t.kind), Some(&TokenKind::Eof), "{source:?}");
            let starts: Vec<_> = tokens
                .iter()
                .map(|t| (t.range.start.line, t.range.start.column))
                .collect();
            assert!(
                starts.windows(2).all(|w| w[0] <= w[1]),
                "token starts are not ordered for {source:?}: {starts:?}"
            );
        }
    }

    proptest! {
        #[test]
        fn lexes_token_soup_consistently(s in fragments_strategy()) {
            assert_lexes_consistently(&s);
        }

        #[test]
        fn lexes_arbitrary_unicode_consistently(s in "\\PC{0,80}") {
            assert_lexes_consistently(&s);
        }

        #[test]
        fn lexes_arbitrary_chars_consistently(s in proptest::collection::vec(proptest::prelude::any::<char>(), 0..60)) {
            assert_lexes_consistently(&s.into_iter().collect::<String>());
        }
    }

    /// Lexes `input` and returns the token kinds, with the ranges inside string segments cleared.
    fn kinds(input: &str, include_spaces: bool) -> Result<Vec<TokenKind>, SyntaxError> {
        Lexer::new(Options {
            ignore_errors: false,
            include_spaces,
        })
        .tokenize(input, 1.into())
        .map(|tokens| {
            tokens
                .into_iter()
                .map(|t| match t.kind {
                    TokenKind::InterpolatedString(segments) => TokenKind::InterpolatedString(
                        segments
                            .into_iter()
                            .map(|segment| match segment {
                                StringSegment::Text(text, _) => StringSegment::Text(text, Range::default()),
                                StringSegment::Expr(expr, _) => StringSegment::Expr(expr, Range::default()),
                            })
                            .collect(),
                    ),
                    kind => kind,
                })
                .collect()
        })
    }

    fn num(value: f64) -> TokenKind {
        TokenKind::NumberLiteral(Number::new(value))
    }

    fn ident(name: &str) -> TokenKind {
        TokenKind::Ident(name.into())
    }

    fn text(value: &str) -> StringSegment {
        StringSegment::Text(value.to_string(), Range::default())
    }

    #[rstest]
    #[case::numbers(
        "1 2.5 .5 -3 +4 1e3 1E+3 2.5e-2",
        vec![num(1.0), num(2.5), num(0.5), num(-3.0), num(4.0), num(1000.0), num(1000.0), num(0.025)]
    )]
    #[case::dot_without_digit_after_integer("1.e3", vec![num(1.0), TokenKind::Selector(".e3".into())])]
    #[case::double_sign_is_not_a_number("--5", vec![TokenKind::Minus, num(-5.0)])]
    #[case::sign_then_plus("-+5", vec![TokenKind::Minus, num(5.0)])]
    #[case::minus_before_identifier("a-b -b", vec![ident("a-b"), TokenKind::Minus, ident("b")])]
    #[case::string_escapes(r#""a\nb\t\"\\""#, vec![TokenKind::StringLiteral("a\nb\t\"\\".into())])]
    #[case::regex_escapes_keep_the_letter(r#""\s\d\/\.""#, vec![TokenKind::StringLiteral("sd/.".into())])]
    #[case::unicode_and_hex_escapes(r#""\u{41}B\x43""#, vec![TokenKind::StringLiteral("ABC".into())])]
    #[case::hex_escape_is_latin1(r#""\xe9""#, vec![TokenKind::StringLiteral("é".into())])]
    #[case::empty_strings(r#""" """#, vec![TokenKind::StringLiteral(String::new()), TokenKind::StringLiteral(String::new())])]
    #[case::multiline_string("\"a\nb\"", vec![TokenKind::StringLiteral("a\nb".into())])]
    #[case::interpolation(
        r#"s"a${b}c$$d\{""#,
        vec![TokenKind::InterpolatedString(vec![
            text("a"),
            StringSegment::Expr("b".into(), Range::default()),
            text("c"),
            text("$"),
            text("d{"),
        ])]
    )]
    #[case::interpolation_expr_keeps_everything_up_to_the_brace(
        r#"s"${ "a" + 1 }""#,
        vec![TokenKind::InterpolatedString(vec![StringSegment::Expr(" \"a\" + 1 ".into(), Range::default())])]
    )]
    #[case::interpolation_without_closing_brace_falls_back(
        r#"s"${x""#,
        vec![ident("s"), TokenKind::StringLiteral("${x".into())]
    )]
    #[case::unmatched_brace_in_interpolated_text_falls_back(
        r#"s"a{b""#,
        vec![ident("s"), TokenKind::StringLiteral("a{b".into())]
    )]
    #[case::carriage_return_is_whitespace_when_skipping("a\rb", vec![ident("a"), ident("b")])]
    #[case::repeated_interpolation_without_closing_brace_falls_back(
        r#"s"${x" s"${y""#,
        vec![ident("s"), TokenKind::StringLiteral("${x".into()), ident("s"), TokenKind::StringLiteral("${y".into())]
    )]
    #[case::interpolation_before_an_unclosed_one(
        r#"s"${a}" s"${b""#,
        vec![
            TokenKind::InterpolatedString(vec![StringSegment::Expr("a".into(), Range::default())]),
            ident("s"),
            TokenKind::StringLiteral("${b".into()),
        ]
    )]
    #[case::empty_interpolated_string_falls_back(r#"s"""#, vec![ident("s"), TokenKind::StringLiteral(String::new())])]
    #[case::bytes(r#"b"A\x42\n\0\\\"""#, vec![TokenKind::BytesLiteral(vec![0x41, 0x42, b'\n', 0, b'\\', b'"'])])]
    #[case::bytes_with_non_ascii_falls_back("b\"é\"", vec![ident("b"), TokenKind::StringLiteral("é".into())])]
    #[case::selectors(
        r#". .h1 .> .^ .* .a-b ."a b""#,
        vec![
            TokenKind::Selector(".".into()),
            TokenKind::Selector(".h1".into()),
            TokenKind::Selector(".>".into()),
            TokenKind::Selector(".^".into()),
            TokenKind::Selector(".*".into()),
            TokenKind::Selector(".a-b".into()),
            TokenKind::Selector(r#"."a b""#.into()),
        ]
    )]
    #[case::selector_with_escaped_quote(r#"."a\"b""#, vec![TokenKind::Selector(r#"."a\"b""#.into())])]
    #[case::ranges_and_spread(".. ... ..5", vec![TokenKind::DoubleDot, TokenKind::DotDotDot, TokenKind::DoubleDot, num(5.0)])]
    #[case::keywords_at_word_boundary(
        "end endx end-x a*b _x True true false",
        vec![
            TokenKind::End,
            ident("endx"),
            TokenKind::End,
            TokenKind::Minus,
            ident("x"),
            ident("a*b"),
            ident("_x"),
            ident("True"),
            TokenKind::BoolLiteral(true),
            TokenKind::BoolLiteral(false),
        ]
    )]
    #[case::env("$HOME $a_1", vec![TokenKind::Env("HOME".into()), TokenKind::Env("a_1".into())])]
    #[case::operators(
        "== =~ = != !~ ! << <= < >> >= > && || | |= ?? ? :: : -> - -= + += * *= / /= // //= % %= @",
        vec![
            TokenKind::EqEq, TokenKind::TildeEqual, TokenKind::Equal, TokenKind::NeEq, TokenKind::NotTildeEqual,
            TokenKind::Not, TokenKind::LeftShift, TokenKind::Lte, TokenKind::Lt, TokenKind::RightShift, TokenKind::Gte,
            TokenKind::Gt, TokenKind::And, TokenKind::Or, TokenKind::Pipe, TokenKind::PipeEqual, TokenKind::Coalesce,
            TokenKind::Question, TokenKind::DoubleColon, TokenKind::Colon, TokenKind::Arrow, TokenKind::Minus,
            TokenKind::MinusEqual, TokenKind::Plus, TokenKind::PlusEqual, TokenKind::Asterisk, TokenKind::StarEqual,
            TokenKind::Slash, TokenKind::SlashEqual, TokenKind::Slash, TokenKind::Slash, TokenKind::DoubleSlashEqual,
            TokenKind::Percent, TokenKind::PercentEqual, TokenKind::Convert,
        ]
    )]
    #[case::comments_are_skipped("a # c\n# d\nb", vec![ident("a"), ident("b")])]
    fn test_token_kinds(#[case] input: &str, #[case] expected: Vec<TokenKind>) {
        let mut expected = expected;
        expected.push(TokenKind::Eof);
        assert_eq!(kinds(input, false), Ok(expected));
    }

    #[rstest]
    #[case::spaces_tabs_comment_and_crlf(
        "a  \tb # c\r\nd",
        vec![
            ident("a"),
            TokenKind::Whitespace(2),
            TokenKind::Tab(1),
            ident("b"),
            TokenKind::Whitespace(1),
            TokenKind::Comment(" c".into()),
            TokenKind::NewLine,
            ident("d"),
        ]
    )]
    #[case::empty_comment("#\nx", vec![TokenKind::Comment(String::new()), TokenKind::NewLine, ident("x")])]
    fn test_token_kinds_with_spaces(#[case] input: &str, #[case] expected: Vec<TokenKind>) {
        let mut expected = expected;
        expected.push(TokenKind::Eof);
        assert_eq!(kinds(input, true), Ok(expected));
    }

    #[test]
    fn test_lone_carriage_return_is_an_error_when_keeping_spaces() {
        assert!(matches!(kinds("a\rb", true), Err(SyntaxError::UnexpectedToken(_))));
    }

    #[rstest]
    #[case::malformed_exponent("1e")]
    #[case::exponent_after_digits("5 1else")]
    #[case::exponent_sign_without_digits("1e+")]
    #[case::unterminated_string("\"abc")]
    #[case::trailing_backslash("\"abc\\")]
    #[case::unknown_escape(r#""\q""#)]
    #[case::unicode_escape_out_of_range(r#""\u{110000}""#)]
    #[case::unicode_escape_surrogate(r#""\ud800""#)]
    #[case::too_many_unicode_digits(r#""\u{1234567}""#)]
    #[case::short_hex_escape(r#""\x4""#)]
    #[case::bad_byte_escape(r#"b"\q""#)]
    #[case::unterminated_bytes(r#"b"abc"#)]
    #[case::lone_ampersand("a & b")]
    #[case::non_ascii_identifier("é")]
    #[case::non_ascii_after_identifier("foo é")]
    #[case::bare_dollar("$ a")]
    #[case::bad_interpolation_escape(r#"s"\q""#)]
    fn test_lex_errors(#[case] input: &str) {
        for include_spaces in [false, true] {
            assert!(
                matches!(kinds(input, include_spaces), Err(SyntaxError::UnexpectedToken(_))),
                "{input:?} (include_spaces={include_spaces}) should fail"
            );
        }
    }

    fn range(start: (u32, usize), end: (u32, usize)) -> Range {
        Range {
            start: Position {
                line: start.0,
                column: start.1,
            },
            end: Position {
                line: end.0,
                column: end.1,
            },
        }
    }

    fn token_ranges(input: &str, include_spaces: bool) -> Vec<Range> {
        Lexer::new(Options {
            ignore_errors: false,
            include_spaces,
        })
        .tokenize(input, 1.into())
        .unwrap()
        .into_iter()
        .map(|t| t.range)
        .collect()
    }

    #[rstest]
    #[case::env_range_excludes_the_dollar("$ab", false, vec![range((1, 2), (1, 4)), range((1, 4), (1, 4))])]
    #[case::newline_range_stays_on_its_line("a\r\nb", true, vec![range((1, 1), (1, 2)), range((1, 2), (1, 4)), range((2, 1), (2, 2)), range((2, 2), (2, 2))])]
    #[case::comment_range_excludes_the_hash("x#hi", true, vec![range((1, 1), (1, 2)), range((1, 3), (1, 5)), range((1, 5), (1, 5))])]
    #[case::multiline_string_ends_on_its_last_line("\"a\nb\" x", false, vec![range((1, 1), (2, 3)), range((2, 4), (2, 5)), range((2, 5), (2, 5))])]
    #[case::columns_restart_after_a_newline("ab\n  cd", false, vec![range((1, 1), (1, 3)), range((2, 3), (2, 5)), range((2, 5), (2, 5))])]
    #[case::columns_count_chars("\"あい\" x", false, vec![range((1, 1), (1, 5)), range((1, 6), (1, 7)), range((1, 7), (1, 7))])]
    #[case::eof_after_trailing_comment("x # é", false, vec![range((1, 1), (1, 2)), range((1, 6), (1, 6))])]
    fn test_token_ranges(#[case] input: &str, #[case] include_spaces: bool, #[case] expected: Vec<Range>) {
        assert_eq!(token_ranges(input, include_spaces), expected);
    }

    #[rstest]
    #[case::at_the_failing_token("a 1e", range((1, 3), (1, 5)))]
    #[case::covers_the_rest_of_the_input("a & b c", range((1, 3), (1, 8)))]
    #[case::trailing_space_is_trimmed_from_the_range("\"abc ", range((1, 1), (1, 5)))]
    fn test_error_range(#[case] input: &str, #[case] expected: Range) {
        match kinds_error(input) {
            SyntaxError::UnexpectedToken(token) => {
                assert_eq!(token.kind, TokenKind::Eof);
                assert_eq!(token.range, expected);
            }
            other => panic!("unexpected error {other:?}"),
        }
    }

    fn kinds_error(input: &str) -> SyntaxError {
        Lexer::new(Options::default()).tokenize(input, 1.into()).unwrap_err()
    }

    fn ident_range(code: &str, name: &str) -> Range {
        Lexer::new(Options::default())
            .tokenize(code, 1.into())
            .unwrap()
            .into_iter()
            .find(|t| matches!(&t.kind, TokenKind::Ident(n) if n == name))
            .unwrap()
            .range
    }

    #[rstest]
    #[case::ascii("\"ab\" | x", 1, 8)]
    #[case::multibyte_string("\"あい\" | x", 1, 8)]
    #[case::emoji_string("\"🎉🎉🎉\" | x", 1, 9)]
    #[case::multibyte_on_previous_line("\"あいう\"\n| x", 2, 3)]
    #[case::several_multibyte_tokens("\"あ\" + \"い\" | x", 1, 13)]
    fn test_column_counts_chars_not_bytes(#[case] code: &str, #[case] line: u32, #[case] column: usize) {
        let range = ident_range(code, "x");
        assert_eq!(range.start, Position { line, column });
        assert_eq!(
            range.end,
            Position {
                line,
                column: column + 1
            }
        );
    }

    proptest! {
        #[test]
        fn prop_column_after_string_literal_counts_chars(s in "[^\"\\\\\\n\\r$]{0,20}") {
            let code = format!("\"{s}\" | x");
            let range = ident_range(&code, "x");
            proptest::prop_assert_eq!(range.start.column, s.chars().count() + 6);
        }
    }

    #[rstest]
    #[case::unterminated_string("foo(\"abc", vec![TokenKind::Ident("foo".into()), TokenKind::LParen, TokenKind::Unknown("\"abc".into()), TokenKind::Eof])]
    #[case::stops_at_delimiter("a § , b", vec![TokenKind::Ident("a".into()), TokenKind::Whitespace(1), TokenKind::Unknown("§".into()), TokenKind::Whitespace(1), TokenKind::Comma, TokenKind::Whitespace(1), TokenKind::Ident("b".into()), TokenKind::Eof])]
    fn test_ignore_errors_emits_unknown_token(#[case] input: &str, #[case] expected: Vec<TokenKind>) {
        let tokens = Lexer::new(Options {
            ignore_errors: true,
            include_spaces: true,
        })
        .tokenize(input, 1.into())
        .unwrap();

        assert_eq!(tokens.into_iter().map(|t| t.kind).collect::<Vec<_>>(), expected);
    }
}
