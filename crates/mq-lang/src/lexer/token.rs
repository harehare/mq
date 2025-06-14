use std::fmt::{self, Display, Formatter};

use compact_str::CompactString;
use itertools::Itertools;

use crate::{eval::module::ModuleId, number::Number, range::Range};

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

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct Token {
    pub range: Range,
    pub kind: TokenKind,
    pub module_id: ModuleId,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub enum TokenKind {
    BoolLiteral(bool),
    Colon,
    Comma,
    Comment(String),
    Def,
    Elif,
    Else,
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
    Question,
    RBracket,
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
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.kind)
    }
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match &self {
            TokenKind::BoolLiteral(b) => write!(f, "{}", b),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Comment(comment) => write!(f, "# {}", comment.trim()),
            TokenKind::Def => write!(f, "def"),
            TokenKind::Elif => write!(f, "elif"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Env(env) => write!(f, "{}", env),
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
            TokenKind::NeEq => write!(f, "!="),
            TokenKind::NewLine => writeln!(f),
            TokenKind::Nodes => write!(f, "nodes"),
            TokenKind::None => write!(f, "None"),
            TokenKind::NumberLiteral(n) => write!(f, "{}", n),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::Selector(selector) => write!(f, "{}", selector),
            TokenKind::Self_ => write!(f, "self"),
            TokenKind::SemiColon => write!(f, ";"),
            TokenKind::StringLiteral(s) => write!(f, "{}", s),
            TokenKind::Tab(n) => write!(f, "{}", "\t".repeat(*n)),
            TokenKind::Until => write!(f, "until"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Whitespace(n) => write!(f, "{}", " ".repeat(*n)),
        }
    }
}
