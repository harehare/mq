use std::fmt::{self, Display, Formatter};

use itertools::Itertools;
use smol_str::SmolStr;

use crate::{ArenaId, module::ModuleId, number::Number, range::Range};
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};

/// A piece of an interpolated string.
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialOrd, PartialEq, Ord, Eq)]
pub enum StringSegment {
    /// Literal text and its range.
    Text(String, Range),
    /// The source of a `${...}` expression and its range.
    Expr(SmolStr, Range),
}

impl Display for StringSegment {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            StringSegment::Text(text, _) => write!(f, "{}", text),
            StringSegment::Expr(expr, _) => write!(f, "${{{}}}", expr),
        }
    }
}

fn default_module_id() -> ModuleId {
    ArenaId::new(0)
}

/// A lexed token with its source range.
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct Token {
    /// Source range of the token.
    pub range: Range,
    /// The kind of the token.
    pub kind: TokenKind,
    /// The module the token came from.
    #[cfg_attr(
        feature = "ast-json",
        serde(skip_serializing, skip_deserializing, default = "default_module_id")
    )]
    pub module_id: ModuleId,
}

/// Represents the kind of a token in the mq language.
///
/// TokenKind variants are sorted alphabetically for maintainability.
#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub enum TokenKind {
    /// `&&`
    And,
    /// `->`
    Arrow,
    /// `as`
    As,
    /// `@`
    Convert,
    /// `*`
    Asterisk,
    /// `true` or `false`.
    BoolLiteral(bool),
    /// A bytes literal.
    BytesLiteral(Vec<u8>),
    /// `break`
    Break,
    /// `catch`
    Catch,
    /// `??`
    Coalesce,
    /// `:`
    Colon,
    /// `::`
    DoubleColon,
    /// `//=`
    DoubleSlashEqual,
    /// `...`
    DotDotDot,
    /// `,`
    Comma,
    /// A comment, without the leading `#`.
    Comment(String),
    /// `continue`
    Continue,
    /// `def`
    Def,
    /// `do`
    Do,
    /// `elif`
    Elif,
    /// `else`
    Else,
    /// `end`
    End,
    /// An environment variable reference such as `$HOME`.
    Env(SmolStr),
    /// End of input.
    Eof,
    /// `=`
    Equal,
    /// `==`
    EqEq,
    /// `fn`
    Fn,
    /// `foreach`
    Foreach,
    /// `>`
    Gt,
    /// `>=`
    Gte,
    /// An identifier.
    Ident(SmolStr),
    /// `if`
    If,
    /// `include`
    Include,
    /// A string with `${...}` segments.
    InterpolatedString(Vec<StringSegment>),
    /// `import`
    Import,
    /// `{`
    LBrace,
    /// `[`
    LBracket,
    /// `let`
    Let,
    /// `<<`
    LeftShift,
    /// `loop`
    Loop,
    /// `<`
    Lt,
    /// `<=`
    Lte,
    /// `match`
    Match,
    /// `module`
    Module,
    /// `-`
    Minus,
    /// `-=`
    MinusEqual,
    /// `!=`
    NeEq,
    /// A line break.
    NewLine,
    /// `nodes`
    Nodes,
    /// `None`
    None,
    /// `!`
    Not,
    /// A number literal.
    NumberLiteral(Number),
    /// `||`
    Or,
    /// `%`
    Percent,
    /// `%=`
    PercentEqual,
    /// `|`
    Pipe,
    /// `|=`
    PipeEqual,
    /// `+`
    Plus,
    /// `+=`
    PlusEqual,
    /// `?`
    Question,
    /// `}`
    RBrace,
    /// `..`
    DoubleDot,
    /// `]`
    RBracket,
    /// `>>`
    RightShift,
    /// `)`
    RParen,
    /// A selector such as `.h1`.
    Selector(SmolStr),
    /// `self`
    Self_,
    /// `;`
    SemiColon,
    /// `/`
    Slash,
    /// `/=`
    SlashEqual,
    /// A string literal.
    StringLiteral(String),
    /// `*=`
    StarEqual,
    /// A run of tabs; the field is the count.
    Tab(usize),
    /// `=~`
    TildeEqual,
    /// `!~`
    NotTildeEqual,
    /// `try`
    Try,
    /// Text the lexer could not tokenize; only produced when `ignore_errors` is set.
    Unknown(String),
    /// `unless`
    Unless,
    /// `until`
    Until,
    /// A run of spaces; the field is the count.
    Whitespace(usize),
    /// `while`
    While,
    /// `(`
    LParen,
    /// `var`
    Var,
    /// `yield`
    Yield,
}

impl TokenKind {
    /// Binding precedence of a binary operator (higher binds tighter); `None` for other tokens.
    /// All binary operators are left-associative. Shared by the AST and CST parsers so both build
    /// the same tree shape.
    pub(crate) fn binary_op_precedence(&self) -> Option<u8> {
        match self {
            TokenKind::Equal
            | TokenKind::PlusEqual
            | TokenKind::MinusEqual
            | TokenKind::StarEqual
            | TokenKind::SlashEqual
            | TokenKind::PercentEqual
            | TokenKind::DoubleSlashEqual
            | TokenKind::PipeEqual => Some(0),
            TokenKind::Or => Some(1),
            TokenKind::And => Some(2),
            TokenKind::EqEq
            | TokenKind::NeEq
            | TokenKind::Gt
            | TokenKind::Gte
            | TokenKind::Lt
            | TokenKind::Lte
            | TokenKind::TildeEqual
            | TokenKind::NotTildeEqual => Some(3),
            TokenKind::Plus | TokenKind::Minus | TokenKind::RightShift | TokenKind::LeftShift => Some(4),
            TokenKind::Asterisk | TokenKind::Slash | TokenKind::Percent | TokenKind::Convert => Some(5),
            TokenKind::DoubleDot | TokenKind::Coalesce => Some(6),
            _ => None,
        }
    }
}

impl Token {
    /// Creates a token of `kind` with a default range and module id.
    pub fn new(kind: TokenKind) -> Self {
        Self {
            kind,
            range: Range::default(),
            module_id: default_module_id(),
        }
    }

    /// Whether this is the end-of-input token.
    #[inline(always)]
    pub fn is_eof(&self) -> bool {
        matches!(self.kind, TokenKind::Eof)
    }

    /// Whether this is a selector token.
    #[inline(always)]
    pub fn is_selector(&self) -> bool {
        matches!(self.kind, TokenKind::Selector(_))
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.kind)
    }
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match &self {
            TokenKind::And => write!(f, "&&"),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::As => write!(f, "as"),
            TokenKind::Convert => write!(f, "@"),
            TokenKind::Or => write!(f, "||"),
            TokenKind::Not => write!(f, "!"),
            TokenKind::Asterisk => write!(f, "*"),
            TokenKind::BoolLiteral(b) => write!(f, "{}", b),
            TokenKind::BytesLiteral(b) => {
                write!(f, "b\"")?;
                for byte in b {
                    if byte.is_ascii_graphic() && *byte != b'"' && *byte != b'\\' {
                        write!(f, "{}", *byte as char)?;
                    } else {
                        write!(f, "\\x{:02x}", byte)?;
                    }
                }
                write!(f, "\"")
            }
            TokenKind::Break => write!(f, "break"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Continue => write!(f, "continue"),
            TokenKind::Coalesce => write!(f, "??"),
            TokenKind::Comment(comment) => write!(f, "# {}", comment.trim()),
            TokenKind::Def => write!(f, "def"),
            TokenKind::Do => write!(f, "do"),
            TokenKind::DoubleColon => write!(f, "::"),
            TokenKind::DoubleSlashEqual => write!(f, "//="),
            TokenKind::DotDotDot => write!(f, "..."),
            TokenKind::Elif => write!(f, "elif"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::End => write!(f, "end"),
            TokenKind::Env(env) => write!(f, "${}", env),
            TokenKind::Eof => write!(f, ""),
            TokenKind::Equal => write!(f, "="),
            TokenKind::EqEq => write!(f, "=="),
            TokenKind::Fn => write!(f, "fn"),
            TokenKind::Foreach => write!(f, "foreach"),
            TokenKind::Ident(ident) => write!(f, "{}", ident),
            TokenKind::If => write!(f, "if"),
            TokenKind::Include => write!(f, "include"),
            TokenKind::Import => write!(f, "import"),
            TokenKind::InterpolatedString(segments) => {
                write!(f, "{}", segments.iter().join(""))
            }
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Lte => write!(f, "<="),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::Gte => write!(f, ">="),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::LParen => write!(f, "("),
            TokenKind::LeftShift => write!(f, "<<"),
            TokenKind::Let => write!(f, "let"),
            TokenKind::Loop => write!(f, "loop"),
            TokenKind::Match => write!(f, "match"),
            TokenKind::Module => write!(f, "module"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::MinusEqual => write!(f, "-="),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::SlashEqual => write!(f, "/="),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::PercentEqual => write!(f, "%="),
            TokenKind::NeEq => write!(f, "!="),
            TokenKind::NewLine => writeln!(f),
            TokenKind::Nodes => write!(f, "nodes"),
            TokenKind::None => write!(f, "None"),
            TokenKind::NumberLiteral(n) => write!(f, "{}", n),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::PlusEqual => write!(f, "+="),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::PipeEqual => write!(f, "|="),
            TokenKind::DoubleDot => write!(f, ".."),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::RightShift => write!(f, ">>"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::Selector(selector) => write!(f, "{}", selector),
            TokenKind::Self_ => write!(f, "self"),
            TokenKind::SemiColon => write!(f, ";"),
            TokenKind::StringLiteral(s) => write!(f, "{}", s),
            TokenKind::StarEqual => write!(f, "*="),
            TokenKind::Tab(n) => write!(f, "{}", "\t".repeat(*n)),
            TokenKind::TildeEqual => write!(f, "=~"),
            TokenKind::NotTildeEqual => write!(f, "!~"),
            TokenKind::Try => write!(f, "try"),
            TokenKind::Unknown(text) => write!(f, "{}", text),
            TokenKind::Unless => write!(f, "unless"),
            TokenKind::Until => write!(f, "until"),
            TokenKind::Catch => write!(f, "catch"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Whitespace(n) => write!(f, "{}", " ".repeat(*n)),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::Var => write!(f, "var"),
            TokenKind::Yield => write!(f, "yield"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        StringSegment::Text("hello".to_string(), Range::default()),
        "hello"
    )]
    #[case(StringSegment::Expr(SmolStr::new("world"), Range::default()), "${world}")]
    #[case(
        StringSegment::Text("".to_string(), Range::default()),
        ""
    )]
    #[case(StringSegment::Expr(SmolStr::new(""), Range::default()), "${}")]
    fn string_segment_display_works(#[case] segment: StringSegment, #[case] expected: &str) {
        assert_eq!(segment.to_string(), expected);
    }

    #[rstest]
    #[case(TokenKind::Arrow, "->")]
    #[case(TokenKind::Fn, "fn")]
    #[case(TokenKind::Minus, "-")]
    #[case(TokenKind::Gt, ">")]
    fn token_kind_arrow_display(#[case] kind: TokenKind, #[case] expected: &str) {
        assert_eq!(kind.to_string(), expected);
    }
}
