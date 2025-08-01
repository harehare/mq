use std::fmt::{self, Display, Formatter};

use compact_str::CompactString;
use itertools::Itertools;

#[cfg(feature = "ast-json")]
use crate::ArenaId;
use crate::{eval::module::ModuleId, number::Number, range::Range};
#[cfg(feature = "ast-json")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ast-json", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialOrd, PartialEq, Ord, Eq)]
pub enum StringSegment {
    Text(String, Range),
    Ident(CompactString, Range),
}

impl Display for StringSegment {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            StringSegment::Text(text, _) => write!(f, "{}", text),
            StringSegment::Ident(ident, _) => write!(f, "${{{}}}", ident),
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
pub enum TokenKind {
    Asterisk,
    BoolLiteral(bool),
    Break,
    Colon,
    Continue,
    Comma,
    Comment(String),
    Def,
    Elif,
    Else,
    End,
    Env(CompactString),
    Eof,
    Equal,
    EqEq,
    Fn,
    Foreach,
    Gt,
    Gte,
    Ident(CompactString),
    If,
    Include,
    InterpolatedString(Vec<StringSegment>),
    LBracket,
    Let,
    Lt,
    Lte,
    NeEq,
    NewLine,
    Nodes,
    None,
    NumberLiteral(Number),
    Pipe,
    Plus,
    Minus,
    Slash,
    Question,
    Percent,
    RangeOp,
    RBracket,
    RBrace,
    RParen,
    Selector(CompactString),
    Self_,
    SemiColon,
    StringLiteral(String),
    Tab(usize),
    Until,
    Whitespace(usize),
    While,
    LParen,
    LBrace,
    And,
    Or,
    Not,
}

impl Token {
    pub fn is_eof(&self) -> bool {
        matches!(self.kind, TokenKind::Eof)
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
            TokenKind::Comment(comment) => write!(f, "# {}", comment.trim()),
            TokenKind::Def => write!(f, "def"),
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
            TokenKind::InterpolatedString(segments) => {
                write!(f, "{}", segments.iter().join(""))
            }
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Lte => write!(f, "<="),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::Gte => write!(f, ">="),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::LParen => write!(f, "("),
            TokenKind::Let => write!(f, "let"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::NeEq => write!(f, "!="),
            TokenKind::NewLine => writeln!(f),
            TokenKind::Nodes => write!(f, "nodes"),
            TokenKind::None => write!(f, "None"),
            TokenKind::NumberLiteral(n) => write!(f, "{}", n),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::RangeOp => write!(f, ".."),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::Selector(selector) => write!(f, "{}", selector),
            TokenKind::Self_ => write!(f, "self"),
            TokenKind::SemiColon => write!(f, ";"),
            TokenKind::StringLiteral(s) => write!(f, "{}", s),
            TokenKind::Tab(n) => write!(f, "{}", "\t".repeat(*n)),
            TokenKind::Until => write!(f, "until"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Whitespace(n) => write!(f, "{}", " ".repeat(*n)),
            TokenKind::LBrace => write!(f, "{{"),
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
    #[case(
        StringSegment::Ident(CompactString::new("world"), Range::default()),
        "${world}"
    )]
    #[case(
        StringSegment::Text("".to_string(), Range::default()),
        ""
    )]
    #[case(StringSegment::Ident(CompactString::new(""), Range::default()), "${}")]
    fn string_segment_display_works(#[case] segment: StringSegment, #[case] expected: &str) {
        assert_eq!(segment.to_string(), expected);
    }
}
