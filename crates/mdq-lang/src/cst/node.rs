use std::{
    fmt::{self, Display},
    sync::Arc,
};

use compact_str::CompactString;
use itertools::Itertools;

use crate::{Range, Token, TokenKind};

type Comment = (Range, String);

#[derive(Debug, Clone, PartialEq)]
pub enum Trivia {
    Whitespace(Arc<Token>),
    NewLine,
    Tab(Arc<Token>),
    Comment(Arc<Token>),
}

impl Display for Trivia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trivia::Whitespace(token) => write!(f, "{}", token),
            Trivia::NewLine => writeln!(f),
            Trivia::Tab(token) => write!(f, "{}", token),
            Trivia::Comment(token) => write!(f, "{}", token),
        }
    }
}

impl Trivia {
    pub fn is_whitespace(&self) -> bool {
        matches!(self, Trivia::Whitespace(_))
    }

    pub fn is_new_line(&self) -> bool {
        matches!(self, Trivia::NewLine)
    }

    pub fn is_tab(&self) -> bool {
        matches!(self, Trivia::Tab(_))
    }

    pub fn is_comment(&self) -> bool {
        matches!(self, Trivia::Comment(_))
    }

    pub fn comment(&self) -> String {
        match self {
            Trivia::Comment(token) => match &token.kind {
                TokenKind::Comment(comment) => comment.to_string(),
                _ => String::new(),
            },
            _ => String::new(),
        }
    }

    pub fn range(&self) -> Range {
        match self {
            Trivia::Comment(token) => token.range.clone(),
            Trivia::Whitespace(token) => token.range.clone(),
            Trivia::Tab(token) => token.range.clone(),
            _ => Range::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    pub token: Option<Arc<Token>>,
    pub leading_trivia: Vec<Trivia>,
    pub trailing_trivia: Vec<Trivia>,
    pub children: Vec<Arc<Node>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Call,
    Def,
    Let,
    Literal,
    Ident,
    Include,
    If,
    Elif,
    Else,
    Selector,
    Self_,
    While,
    Until,
    Foreach,
    Eof,
    Token,
}

impl Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.token
                .as_ref()
                .map(|token| token.kind.to_string())
                .unwrap_or_default()
        )
    }
}

impl Node {
    pub fn has_new_line(&self) -> bool {
        self.leading_trivia.contains(&Trivia::NewLine)
    }

    pub fn range(&self) -> Range {
        self.token
            .as_ref()
            .map(|token| token.range.clone())
            .unwrap_or_default()
    }

    pub fn node_range(&self) -> Range {
        if self.children.is_empty() {
            self.range()
        } else {
            let start = self.range().start;
            let end = self.children.last().unwrap().range().end;
            Range { start, end }
        }
    }

    pub fn name(&self) -> Option<CompactString> {
        self.token.as_ref().map(|token| token.to_string().into())
    }

    pub fn is_token(&self) -> bool {
        matches!(self.kind, NodeKind::Token)
    }

    pub fn comments(&self) -> Vec<Comment> {
        self.leading_trivia
            .iter()
            .filter(|trivia| trivia.is_comment())
            .map(|trivia| (trivia.range(), trivia.comment()))
            .collect_vec()
    }

    pub fn children_without_token(&self) -> Vec<Arc<Node>> {
        self.children
            .iter()
            .filter(|child| !child.is_token())
            .cloned()
            .collect_vec()
    }

    pub fn split_cond_and_program(&self) -> (Vec<Arc<Node>>, Vec<Arc<Node>>) {
        let expr_index = self
            .children
            .iter()
            .position(|child| matches!(child.token.as_ref().unwrap().kind, TokenKind::Colon))
            .unwrap_or_default();

        (
            self.children
                .iter()
                .take(expr_index)
                .filter(|child| !child.is_token())
                .cloned()
                .collect_vec(),
            self.children
                .iter()
                .skip(expr_index)
                .filter(|child| !child.is_token())
                .cloned()
                .collect_vec(),
        )
    }
}
