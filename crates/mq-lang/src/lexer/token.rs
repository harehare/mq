use std::fmt::{self, Display, Formatter};

use itertools::Itertools;
use smol_str::SmolStr;

#[cfg(feature = "ast-json")]
use crate::ArenaId;
use crate::{module::ModuleId, number::Number, range::Range};
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialOrd, PartialEq, Ord, Eq)]
pub enum StringSegment {
    Text(String, Range),
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

#[cfg(feature = "ast-json")]
fn default_module_id() -> ModuleId {
    ArenaId::new(0)
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct Token {
    pub range: Range,
    pub kind: TokenKind,
    #[cfg_attr(
        feature = "ast-json",
        serde(skip_serializing, skip_deserializing, default = "default_module_id")
    )]
    pub module_id: ModuleId,
}

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
/// Represents the kind of a token in the mq language.
///
/// TokenKind variants are sorted alphabetically for maintainability.
pub enum TokenKind {
    And,
    Asterisk,
    BoolLiteral(bool),
    Break,
    Catch,
    Coalesce,
    Colon,
    DoubleColon,
    DoubleSlashEqual,
    Comma,
    Comment(String),
    Continue,
    Def,
    Do,
    Elif,
    Else,
    End,
    Env(SmolStr),
    Eof,
    Equal,
    EqEq,
    Fn,
    Foreach,
    Gt,
    Gte,
    Ident(SmolStr),
    If,
    Include,
    InterpolatedString(Vec<StringSegment>),
    Import,
    LBrace,
    LBracket,
    Let,
    LeftShift,
    Loop,
    Lt,
    Lte,
    Macro,
    Match,
    Module,
    Minus,
    MinusEqual,
    NeEq,
    NewLine,
    Nodes,
    None,
    Not,
    NumberLiteral(Number),
    Or,
    Percent,
    PercentEqual,
    Pipe,
    PipeEqual,
    Plus,
    PlusEqual,
    Question,
    Quote,
    RBrace,
    DoubleDot,
    RBracket,
    RightShift,
    RParen,
    Selector(SmolStr),
    Self_,
    SemiColon,
    Slash,
    SlashEqual,
    StringLiteral(String),
    StarEqual,
    Tab(usize),
    TildeEqual,
    Try,
    Unquote,
    Whitespace(usize),
    While,
    LParen,
    Var,
}

impl Token {
    #[inline(always)]
    pub fn is_eof(&self) -> bool {
        matches!(self.kind, TokenKind::Eof)
    }

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
            TokenKind::Or => write!(f, "||"),
            TokenKind::Not => write!(f, "!"),
            TokenKind::Asterisk => write!(f, "*"),
            TokenKind::BoolLiteral(b) => write!(f, "{}", b),
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
            TokenKind::Macro => write!(f, "macro"),
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
            TokenKind::Quote => write!(f, "quote"),
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
            TokenKind::Try => write!(f, "try"),
            TokenKind::Unquote => write!(f, "unquote"),
            TokenKind::Catch => write!(f, "catch"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Whitespace(n) => write!(f, "{}", " ".repeat(*n)),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::Var => write!(f, "var"),
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
}
