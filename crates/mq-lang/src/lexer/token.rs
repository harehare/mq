use std::fmt::{self, Display, Formatter};

use compact_str::CompactString;
use itertools::Itertools;

use crate::{eval::module::ModuleId, number::Number, range::Range};

#[derive(Debug, Clone, PartialOrd, PartialEq, Eq)]
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

#[derive(PartialEq, Eq, PartialOrd, Debug, Clone)]
pub struct Token {
    pub range: Range,
    pub kind: TokenKind,
    pub module_id: ModuleId,
}

#[derive(PartialEq, Eq, PartialOrd, Debug, Clone)]
pub enum TokenKind {
    Def,
    Colon,
    Equal,
    Eof,
    Let,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Pipe,
    SemiColon,
    Question,
    Self_,
    While,
    Until,
    If,
    Else,
    Elif,
    None,
    Include,
    Foreach,
    Comment(String),
    Env(CompactString),
    Selector(CompactString),
    Ident(CompactString),
    StringLiteral(String),
    InterpolatedString(Vec<StringSegment>),
    NumberLiteral(Number),
    BoolLiteral(bool),
    Whitespace(usize),
    NewLine,
    Tab(usize),
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.kind)
    }
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match &self {
            TokenKind::Def => write!(f, "def"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Eof => write!(f, ""),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::SemiColon => write!(f, ";"),
            TokenKind::Let => write!(f, "let"),
            TokenKind::Equal => write!(f, "="),
            TokenKind::Self_ => write!(f, "self"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Elif => write!(f, "elif"),
            TokenKind::Until => write!(f, "until"),
            TokenKind::While => write!(f, "while"),
            TokenKind::None => write!(f, "None"),
            TokenKind::Include => write!(f, "include"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::NewLine => writeln!(f),
            TokenKind::InterpolatedString(segments) => {
                write!(f, "{}", segments.iter().join(""))
            }
            TokenKind::Foreach => write!(f, "foreach"),
            TokenKind::Tab(n) => write!(f, "{}", "\t".repeat(*n)),
            TokenKind::Whitespace(n) => write!(f, "{}", " ".repeat(*n)),
            TokenKind::Comment(comment) => write!(f, "# {}", comment.trim()),
            TokenKind::Env(env) => write!(f, "{}", env),
            TokenKind::Selector(selector) => write!(f, "{}", selector),
            TokenKind::Ident(ident) => write!(f, "{}", ident),
            TokenKind::StringLiteral(s) => write!(f, "{}", s),
            TokenKind::NumberLiteral(n) => write!(f, "{}", n),
            TokenKind::BoolLiteral(b) => write!(f, "{}", b),
        }
    }
}
