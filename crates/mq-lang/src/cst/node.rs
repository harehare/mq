use std::fmt::{self, Display};

use smol_str::SmolStr;

use crate::{Range, Token};
use crate::{Shared, TokenKind};

type Comment = (Range, String);

#[derive(Debug, Clone, PartialEq)]
pub enum Trivia {
    Whitespace(Shared<Token>),
    NewLine,
    Tab(Shared<Token>),
    Comment(Shared<Token>),
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
    pub token: Option<Shared<Token>>,
    pub leading_trivia: Vec<Trivia>,
    pub trailing_trivia: Vec<Trivia>,
    pub children: Vec<Shared<Node>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Array,
    BinaryOp(BinaryOp),
    Break,
    Call,
    Continue,
    Def,
    Dict,
    End,
    Elif,
    Else,
    Env,
    Eof,
    Fn,
    Foreach,
    Group,
    Ident,
    If,
    Include,
    InterpolatedString,
    Let,
    Literal,
    Nodes,
    Selector,
    Self_,
    Token,
    UnaryOp(UnaryOp),
    Until,
    While,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    And,
    Division,
    Equal,
    Gt,
    Gte,
    Lt,
    Lte,
    Minus,
    Modulo,
    Multiplication,
    NotEqual,
    Or,
    Plus,
    RangeOp,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
    Negate,
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
            let end = self
                .children
                .last()
                .map(|child| child.range().end)
                .unwrap_or(start.clone());
            Range { start, end }
        }
    }

    pub fn name(&self) -> Option<SmolStr> {
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
            .collect::<Vec<_>>()
    }

    pub fn children_without_token(&self) -> Vec<Shared<Node>> {
        self.children
            .iter()
            .filter(|child| !child.is_token())
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn split_cond_and_program(&self) -> (Vec<Shared<Node>>, Vec<Shared<Node>>) {
        let expr_index = self
            .children
            .iter()
            .position(|child| {
                child
                    .token
                    .as_ref()
                    .map(|token| matches!(token.kind, TokenKind::Colon))
                    .unwrap_or(false)
            })
            .unwrap_or_default();

        (
            self.children
                .iter()
                .take(expr_index)
                .filter(|child| !child.is_token())
                .cloned()
                .collect::<Vec<_>>(),
            self.children
                .iter()
                .skip(expr_index)
                .filter(|child| !child.is_token())
                .cloned()
                .collect::<Vec<_>>(),
        )
    }

    pub fn binary_op(&self) -> Option<(Shared<Node>, Shared<Node>)> {
        if let NodeKind::BinaryOp(_) = self.kind {
            let mut non_token_children = self.children.iter().filter(|child| !child.is_token());
            let left = non_token_children.next()?;
            let right = non_token_children.next()?;
            Some((Shared::clone(left), Shared::clone(right)))
        } else {
            None
        }
    }

    pub fn unary_op(&self) -> Option<Shared<Node>> {
        if let NodeKind::UnaryOp(_) = self.kind {
            let operand = self.children.iter().find(|child| !child.is_token())?;
            Some(Shared::clone(operand))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {

    use rstest::rstest;

    use super::*;
    use crate::arena::ArenaId;

    #[rstest]
    #[case(
        Trivia::Whitespace(Shared::new(Token {
            kind: TokenKind::Whitespace(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        " "
    )]
    #[case(Trivia::NewLine, "\n")]
    #[case(
        Trivia::Tab(Shared::new(Token {
            kind: TokenKind::Tab(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        "\t"
    )]
    #[case(
        Trivia::Comment(Shared::new(Token {
            kind: TokenKind::Comment("comment".to_string()),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        "# comment"
    )]
    fn test_trivia_display(#[case] trivia: Trivia, #[case] expected: &str) {
        assert_eq!(format!("{}", trivia), expected);
    }

    #[rstest]
    #[case(
        Trivia::Whitespace(Shared::new(Token {
            kind: TokenKind::Whitespace(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        true, false, false, false
    )]
    #[case(Trivia::NewLine, false, true, false, false)]
    #[case(
        Trivia::Tab(Shared::new(Token {
            kind: TokenKind::Tab(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        false, false, true, false
    )]
    #[case(
        Trivia::Comment(Shared::new(Token {
            kind: TokenKind::Comment("comment".to_string()),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        false, false, false, true
    )]
    fn test_trivia_type_checks(
        #[case] trivia: Trivia,
        #[case] is_whitespace: bool,
        #[case] is_new_line: bool,
        #[case] is_tab: bool,
        #[case] is_comment: bool,
    ) {
        assert_eq!(trivia.is_whitespace(), is_whitespace);
        assert_eq!(trivia.is_new_line(), is_new_line);
        assert_eq!(trivia.is_tab(), is_tab);
        assert_eq!(trivia.is_comment(), is_comment);
    }

    #[rstest]
    #[case(
        Trivia::Comment(Shared::new(Token {
            kind: TokenKind::Comment("test comment".to_string()),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        "test comment"
    )]
    #[case(
        Trivia::Whitespace(Shared::new(Token {
            kind: TokenKind::Whitespace(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        ""
    )]
    fn test_trivia_comment(#[case] trivia: Trivia, #[case] expected: &str) {
        assert_eq!(trivia.comment(), expected);
    }

    #[rstest]
    #[case(NodeKind::Token, true)]
    #[case(NodeKind::Call, false)]
    #[case(NodeKind::Def, false)]
    fn test_node_is_token(#[case] kind: NodeKind, #[case] expected: bool) {
        let node = Node {
            kind,
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
            children: Vec::new(),
        };
        assert_eq!(node.is_token(), expected);
    }

    #[test]
    fn test_node_has_new_line() {
        let node = Node {
            kind: NodeKind::Call,
            token: None,
            leading_trivia: vec![Trivia::NewLine],
            trailing_trivia: Vec::new(),
            children: Vec::new(),
        };
        assert!(node.has_new_line());

        let node_without_newline = Node {
            kind: NodeKind::Call,
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
            children: Vec::new(),
        };
        assert!(!node_without_newline.has_new_line());
    }

    #[test]
    fn test_children_without_token() {
        let token_node = Shared::new(Node {
            kind: NodeKind::Token,
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
            children: Vec::new(),
        });

        let call_node = Shared::new(Node {
            kind: NodeKind::Call,
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
            children: Vec::new(),
        });

        let parent = Node {
            kind: NodeKind::Def,
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
            children: vec![token_node, call_node.clone()],
        };

        let result = parent.children_without_token();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], call_node);
    }
}
