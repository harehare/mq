use std::{collections::BTreeSet, fmt::Display, iter::Peekable};

use crate::{
    Position, Range, Shared, Token, TokenKind,
    cst::node::{BinaryOp, UnaryOp},
};

use super::{
    error::ParseError,
    node::{Node, NodeKind, Trivia},
};
use itertools::Itertools;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub struct ErrorReporter {
    errors: BTreeSet<ParseError>,
    max_errors: usize,
}

impl Display for ErrorReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.errors.iter().join(", "))
    }
}

impl Default for ErrorReporter {
    fn default() -> Self {
        Self {
            errors: BTreeSet::new(),
            max_errors: 100,
        }
    }
}

impl ErrorReporter {
    pub fn new(max_errors: usize) -> Self {
        Self {
            errors: BTreeSet::new(),
            max_errors,
        }
    }

    pub fn with_error(errors: Vec<ParseError>, max_errors: usize) -> Self {
        let mut h = BTreeSet::new();
        h.extend(errors);

        Self {
            errors: h,
            max_errors,
        }
    }

    pub fn report(&mut self, error: ParseError) {
        if self.errors.len() < self.max_errors {
            self.errors.insert(error);
        }
    }

    pub fn to_vec(&self) -> Vec<ParseError> {
        self.errors
            .iter()
            .sorted_by(|a, b| {
                let a_range = match a {
                    ParseError::UnexpectedToken(token) => &token.range,
                    ParseError::InsufficientTokens(token) => &token.range,
                    ParseError::ExpectedClosingBracket(token) => &token.range,
                    ParseError::UnexpectedEOFDetected => return std::cmp::Ordering::Greater,
                };

                let b_range = match b {
                    ParseError::UnexpectedToken(token) => &token.range,
                    ParseError::InsufficientTokens(token) => &token.range,
                    ParseError::ExpectedClosingBracket(token) => &token.range,
                    ParseError::UnexpectedEOFDetected => return std::cmp::Ordering::Less,
                };

                a_range
                    .start
                    .line
                    .cmp(&b_range.start.line)
                    .then_with(|| a_range.start.column.cmp(&b_range.start.column))
            })
            .cloned()
            .collect()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn error_ranges(&self, text: &str) -> Vec<(String, Range)> {
        self.to_vec()
            .iter()
            .map(|e| {
                (
                    e.to_string(),
                    match e {
                        ParseError::UnexpectedToken(token) => token.range.clone(),
                        ParseError::InsufficientTokens(token) => token.range.clone(),
                        ParseError::ExpectedClosingBracket(token) => token.range.clone(),
                        ParseError::UnexpectedEOFDetected => Range {
                            start: Position {
                                line: text.lines().count() as u32,
                                column: text.lines().last().map(|line| line.len()).unwrap_or(0),
                            },
                            end: Position {
                                line: text.lines().count() as u32,
                                column: text.lines().last().map(|line| line.len()).unwrap_or(0),
                            },
                        },
                    },
                )
            })
            .collect::<Vec<_>>()
    }
}

pub struct Parser<'a> {
    tokens: Peekable<core::slice::Iter<'a, Shared<Token>>>,
    errors: ErrorReporter,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: core::slice::Iter<'a, Shared<Token>>) -> Self {
        Self {
            tokens: tokens.peekable(),
            errors: ErrorReporter::new(100),
        }
    }

    pub fn parse(&mut self) -> (Vec<Shared<Node>>, ErrorReporter) {
        self.parse_program(true, false)
    }

    fn parse_program(&mut self, root: bool, in_loop: bool) -> (Vec<Shared<Node>>, ErrorReporter) {
        let mut nodes: Vec<Shared<Node>> = Vec::with_capacity(self.tokens.len());
        let mut leading_trivia = self.parse_leading_trivia();

        while self.tokens.peek().is_some() {
            let node = self.parse_expr(leading_trivia, root, in_loop);
            match node {
                Ok(node) => nodes.push(node),
                Err(e) => {
                    self.skip_tokens();
                    self.errors.report(e)
                }
            }

            leading_trivia = self.parse_leading_trivia();

            let token = match self.tokens.peek() {
                Some(token) => Shared::clone(token),
                None => break,
            };

            match &*token {
                Token {
                    range: _,
                    kind: TokenKind::Eof,
                    ..
                } => {
                    if !nodes.is_empty() {
                        self.tokens.next();

                        nodes.push(Shared::new(Node {
                            kind: NodeKind::Eof,
                            token: Some(Shared::clone(&token)),
                            leading_trivia,
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }));
                    }

                    break;
                }
                Token {
                    range: _,
                    kind: TokenKind::Pipe,
                    ..
                } => {
                    self.tokens.next();
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(&token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));
                    leading_trivia = self.parse_leading_trivia();

                    continue;
                }
                Token {
                    range: _,
                    kind: TokenKind::SemiColon,
                    ..
                }
                | Token {
                    range: _,
                    kind: TokenKind::End,
                    ..
                } => {
                    self.tokens.next();
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(Shared::new(Node {
                        kind: if matches!(token.kind, TokenKind::End) {
                            NodeKind::End
                        } else {
                            NodeKind::Token
                        },
                        token: Some(Shared::clone(&token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));

                    if root {
                        leading_trivia = self.parse_leading_trivia();

                        if let Some(token) = self.tokens.clone().peek() {
                            if matches!(token.kind, TokenKind::Eof) {
                                break;
                            } else if matches!(token.kind, TokenKind::Pipe) {
                                self.tokens.next();
                                nodes.push(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::clone(token)),

                                    leading_trivia,
                                    trailing_trivia: self.parse_trailing_trivia(),
                                    children: Vec::new(),
                                }));
                                leading_trivia = self.parse_leading_trivia();
                                continue;
                            } else {
                                self.errors.report(ParseError::UnexpectedEOFDetected);
                            }
                        }
                    }

                    break;
                }
                Token {
                    range: _,
                    kind: TokenKind::Def,
                    ..
                }
                | Token {
                    range: _,
                    kind: TokenKind::Let,
                    ..
                } => {}
                token => {
                    self.errors
                        .report(ParseError::UnexpectedToken(Shared::new(token.clone())));
                    break;
                }
            }
        }

        if nodes.is_empty() {
            match self.tokens.peek() {
                Some(token) => self
                    .errors
                    .report(ParseError::UnexpectedToken(Shared::clone(token))),
                None => self.errors.report(ParseError::UnexpectedEOFDetected),
            };
        }

        (nodes, self.errors.clone())
    }

    fn parse_expr(
        &mut self,
        leading_trivia: Vec<Trivia>,
        root: bool,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        self.parse_equality_expr(leading_trivia, root, in_loop)
    }

    fn parse_equality_expr(
        &mut self,
        leading_trivia: Vec<Trivia>,
        root: bool,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let mut lhs = self.parse_primary_expr(leading_trivia, root, in_loop)?;

        while self.try_next_token(|kind| {
            matches!(
                kind,
                TokenKind::And
                    | TokenKind::Asterisk
                    | TokenKind::Coalesce
                    | TokenKind::EqEq
                    | TokenKind::Gte
                    | TokenKind::Gt
                    | TokenKind::Lt
                    | TokenKind::Lte
                    | TokenKind::Minus
                    | TokenKind::NeEq
                    | TokenKind::Or
                    | TokenKind::Percent
                    | TokenKind::Plus
                    | TokenKind::RangeOp
                    | TokenKind::Slash
            )
        }) {
            let leading_trivia = self.parse_leading_trivia();
            let operator_token = self.tokens.next().unwrap();
            let mut op = Node {
                kind: match &**operator_token {
                    Token {
                        range: _,
                        kind: TokenKind::And,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::And),
                    Token {
                        range: _,
                        kind: TokenKind::Asterisk,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Multiplication),
                    Token {
                        range: _,
                        kind: TokenKind::Coalesce,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Coalesce),
                    Token {
                        range: _,
                        kind: TokenKind::EqEq,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Equal),
                    Token {
                        range: _,
                        kind: TokenKind::Gte,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Gte),
                    Token {
                        range: _,
                        kind: TokenKind::Gt,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Gt),
                    Token {
                        range: _,
                        kind: TokenKind::Lte,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Lte),
                    Token {
                        range: _,
                        kind: TokenKind::Lt,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Lt),
                    Token {
                        range: _,
                        kind: TokenKind::Minus,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Minus),
                    Token {
                        range: _,
                        kind: TokenKind::NeEq,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::NotEqual),
                    Token {
                        range: _,
                        kind: TokenKind::Or,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Or),
                    Token {
                        range: _,
                        kind: TokenKind::Percent,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Modulo),
                    Token {
                        range: _,
                        kind: TokenKind::Plus,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Plus),
                    Token {
                        range: _,
                        kind: TokenKind::RangeOp,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::RangeOp),
                    Token {
                        range: _,
                        kind: TokenKind::Slash,
                        ..
                    } => NodeKind::BinaryOp(BinaryOp::Division),
                    _ => unreachable!(),
                },
                token: Some(Shared::clone(operator_token)),
                leading_trivia,
                trailing_trivia: self.parse_trailing_trivia(),
                children: Vec::new(),
            };

            let leading_trivia = self.parse_leading_trivia();
            let rhs = self.parse_primary_expr(leading_trivia, root, in_loop)?;

            op.children = vec![lhs, rhs];
            lhs = Shared::new(op);
        }

        Ok(lhs)
    }

    fn parse_primary_expr(
        &mut self,
        leading_trivia: Vec<Trivia>,
        root: bool,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        if let Some(token) = &self.tokens.peek() {
            match &****token {
                Token {
                    range: _,
                    kind: TokenKind::Def,
                    ..
                } => self.parse_def(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Do,
                    ..
                } => self.parse_block(leading_trivia, in_loop),
                Token {
                    range: _,
                    kind: TokenKind::Fn,
                    ..
                } => self.parse_fn(leading_trivia, in_loop),
                Token {
                    range: _,
                    kind: TokenKind::If,
                    ..
                } => self.parse_if(leading_trivia, in_loop),
                Token {
                    range: _,
                    kind: TokenKind::Foreach,
                    ..
                } => self.parse_foreach(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Include,
                    ..
                } => self.parse_include(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::While,
                    ..
                } => self.parse_while(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Until,
                    ..
                } => self.parse_until(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Try,
                    ..
                } => self.parse_try(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Ident(_),
                    ..
                } => self.parse_ident(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Self_,
                    ..
                } => self.parse_self(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Let,
                    ..
                } => self.parse_let(leading_trivia, in_loop),
                Token {
                    range: _,
                    kind: TokenKind::Selector(_),
                    ..
                } => self.parse_selector(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::StringLiteral(_),
                    ..
                }
                | Token {
                    range: _,
                    kind: TokenKind::NumberLiteral(_),
                    ..
                }
                | Token {
                    range: _,
                    kind: TokenKind::BoolLiteral(_),
                    ..
                }
                | Token {
                    range: _,
                    kind: TokenKind::None,
                    ..
                } => self.parse_node(NodeKind::Literal, leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::InterpolatedString(_),
                    ..
                } => self.parse_interpolated_string(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::LBracket,
                    ..
                } => self.parse_array(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::LBrace,
                    ..
                } => self.parse_dict(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::LParen,
                    ..
                } => self.parse_group_expr(leading_trivia, root, in_loop),
                Token {
                    range: _,
                    kind: TokenKind::Nodes,
                    ..
                } if root => self.parse_nodes(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Env(_),
                    ..
                } => self.parse_node(NodeKind::Env, leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Not,
                    ..
                }
                | Token {
                    range: _,
                    kind: TokenKind::Minus,
                    ..
                } => self.parse_unary_op(leading_trivia, root),
                Token {
                    range: _,
                    kind: TokenKind::Break,
                    ..
                } if in_loop => self.parse_node(NodeKind::Break, leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Continue,
                    ..
                } if in_loop => self.parse_node(NodeKind::Continue, leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::Eof,
                    ..
                } => {
                    self.tokens.next();
                    Err(ParseError::UnexpectedEOFDetected)
                }
                token => {
                    self.tokens.next();
                    Err(ParseError::UnexpectedToken(Shared::new(token.clone())))
                }
            }
        } else {
            Err(ParseError::UnexpectedEOFDetected)
        }
    }

    fn parse_group_expr(
        &mut self,
        leading_trivia: Vec<Trivia>,
        root: bool,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let mut node = Node {
            kind: NodeKind::Group,
            token: None,
            leading_trivia,
            trailing_trivia: Vec::new(),
            children: Vec::new(),
        };

        let mut children: Vec<Shared<Node>> = Vec::with_capacity(64);

        children.push(self.next_node(
            |token_kind| matches!(token_kind, TokenKind::LParen),
            NodeKind::Token,
        )?);

        match self.tokens.peek() {
            Some(token) => Shared::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        let leading_trivia = self.parse_leading_trivia();

        children.push(self.parse_expr(leading_trivia, root, in_loop)?);
        children.push(self.next_node(
            |token_kind| matches!(token_kind, TokenKind::RParen),
            NodeKind::Token,
        )?);

        node.children = children;

        Ok(Shared::new(node))
    }

    fn parse_ident(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(100);
        let mut node = Node {
            kind: NodeKind::Ident,
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        match self.tokens.peek() {
            Some(token) if matches!(token.kind, TokenKind::LParen) => {
                let mut args = self.parse_args()?;
                children.append(&mut args);

                if self.try_next_token(|kind| matches!(kind, TokenKind::Question)) {
                    children.push(self.next_node(
                        |token_kind| matches!(token_kind, TokenKind::Question),
                        NodeKind::Token,
                    )?);
                }

                node.kind = NodeKind::Call;
                node.children = children;

                // Check for bracket access after function call (e.g., foo()[0])
                if matches!(
                    self.tokens.peek().map(|t| &t.kind),
                    Some(TokenKind::LBracket)
                ) {
                    self.parse_bracket_access(node)
                } else {
                    Ok(Shared::new(node))
                }
            }
            Some(token) if matches!(token.kind, TokenKind::LBracket) => {
                self.parse_bracket_access(node)
            }
            _ => Ok(Shared::new(node)),
        }
    }

    // Parses bracket access operations recursively to handle nested access like arr[0][1][2]
    fn parse_bracket_access(&mut self, mut node: Node) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = node.children;

        // Parse bracket access: ident[key] -> get(ident, key) or ident[start:end] -> slice(ident, start, end)
        children.push(self.next_node(
            |token_kind| matches!(token_kind, TokenKind::LBracket),
            NodeKind::Token,
        )?);

        // Parse the first expression
        let first_expr = self.parse_expr(Vec::new(), false, false)?;
        children.push(first_expr);

        // Check if this is a slice operation (contains ':')
        let is_slice =
            matches!(self.tokens.peek(), Some(token) if matches!(token.kind, TokenKind::Colon));

        if is_slice {
            // Add the colon token
            children.push(self.next_node(
                |token_kind| matches!(token_kind, TokenKind::Colon),
                NodeKind::Token,
            )?);

            // Parse the second expression (end index)
            let second_expr = self.parse_expr(Vec::new(), false, false)?;
            children.push(second_expr);
        }

        // Expect closing bracket
        match self.tokens.peek() {
            Some(token) if matches!(token.kind, TokenKind::RBracket) => {
                children.push(self.next_node(
                    |token_kind| matches!(token_kind, TokenKind::RBracket),
                    NodeKind::Token,
                )?);
            }
            Some(token) => {
                return Err(ParseError::ExpectedClosingBracket(Shared::clone(token)));
            }
            None => {
                return Err(ParseError::UnexpectedEOFDetected);
            }
        }

        node.kind = NodeKind::Call;
        node.children = children;

        // Check for additional bracket access (nested indexing)
        let final_node = if matches!(
            self.tokens.peek().map(|t| &t.kind),
            Some(TokenKind::LBracket)
        ) {
            self.parse_bracket_access(node)?
        } else {
            Shared::new(node)
        };

        // Check for function call after bracket access (e.g., arr[0]())
        if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LParen)) {
            let args = self.parse_args()?;
            let mut dynamic_call_node = Node {
                kind: NodeKind::CallDynamic,
                token: final_node.token.clone(),
                leading_trivia: final_node.leading_trivia.clone(),
                trailing_trivia: Vec::new(),
                children: vec![final_node],
            };
            dynamic_call_node.children.extend(args);
            Ok(Shared::new(dynamic_call_node))
        } else {
            Ok(final_node)
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Shared<Node>>, ParseError> {
        let mut nodes: Vec<Shared<Node>> = Vec::with_capacity(64);

        nodes.push(self.next_node(
            |token_kind| matches!(token_kind, TokenKind::LParen),
            NodeKind::Token,
        )?);

        let token = match self.tokens.peek() {
            Some(token) => Shared::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        if matches!(token.kind, TokenKind::RParen) {
            let leading_trivia = self.parse_leading_trivia();
            let token = self.tokens.next().unwrap();
            let trailing_trivia = self.parse_trailing_trivia();
            nodes.push(Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia,
                children: Vec::new(),
            }));

            return Ok(nodes);
        }

        loop {
            let arg_node = self.parse_arg()?;
            let leading_trivia = self.parse_leading_trivia();
            let token = match self.tokens.peek() {
                Some(token) => Shared::clone(token),
                None => return Err(ParseError::UnexpectedEOFDetected),
            };

            match &token.kind {
                TokenKind::Comma => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(arg_node);
                    nodes.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));
                }
                TokenKind::RParen => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(arg_node);
                    nodes.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));

                    break;
                }
                _ => return Err(ParseError::UnexpectedToken(Shared::clone(&token))),
            }
        }

        Ok(nodes)
    }

    fn parse_arg(&mut self) -> Result<Shared<Node>, ParseError> {
        let leading_trivia = self.parse_leading_trivia();
        let token = match self.tokens.peek() {
            Some(token) => Shared::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        match &*token {
            Token {
                range: _,
                kind: TokenKind::Ident(_),
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::StringLiteral(_),
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::BoolLiteral(_),
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::NumberLiteral(_),
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::None,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::Selector(_),
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::InterpolatedString(_),
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::Self_,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::LBracket,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::Not,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::Minus,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::Fn,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::Until,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::Foreach,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::While,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::If,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::LParen,
                ..
            }
            | Token {
                range: _,
                kind: TokenKind::LBrace,
                ..
            } => self.parse_expr(leading_trivia, false, false),
            _ => Err(ParseError::UnexpectedToken(Shared::clone(&token))),
        }
    }

    fn parse_def(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(100);

        let mut node = Node {
            kind: NodeKind::Def,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Ident(_)), NodeKind::Ident)?);

        let mut params = self.parse_params()?;
        children.append(&mut params);

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let (mut program, _) = self.parse_program(false, false);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_fn(
        &mut self,
        leading_trivia: Vec<Trivia>,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(100);

        let mut node = Node {
            kind: NodeKind::Fn,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        let mut params = self.parse_params()?;
        children.append(&mut params);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let (mut program, _) = self.parse_program(false, in_loop);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_block(
        &mut self,
        leading_trivia: Vec<Trivia>,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();

        let mut node = Node {
            kind: NodeKind::Block,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        let (program, _) = self.parse_program(false, in_loop);

        node.children = program;
        Ok(Shared::new(node))
    }

    fn parse_selector(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();

        let mut node = Node {
            kind: NodeKind::Selector,
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        match &**token {
            Token {
                range: _,
                kind: TokenKind::Selector(s),
                ..
            } if s == "." => {
                let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

                // []
                children.push(
                    self.next_node(|kind| matches!(kind, TokenKind::LBracket), NodeKind::Token)?,
                );

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                if let Token {
                    range: _,
                    kind: TokenKind::NumberLiteral(_),
                    ..
                } = &*token
                {
                    children.push(self.next_node(
                        |kind| matches!(kind, TokenKind::NumberLiteral(_)),
                        NodeKind::Literal,
                    )?);
                }

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                if let Token {
                    range: _,
                    kind: TokenKind::RBracket,
                    ..
                } = &*token
                {
                    children.push(
                        self.next_node(
                            |kind| matches!(kind, TokenKind::RBracket),
                            NodeKind::Token,
                        )?,
                    );
                } else {
                    return Err(ParseError::UnexpectedToken(Shared::clone(&token)));
                }

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                // [][]
                if let Token {
                    range: _,
                    kind: TokenKind::LBracket,
                    ..
                } = &*token
                {
                    children.push(
                        self.next_node(
                            |kind| matches!(kind, TokenKind::LBracket),
                            NodeKind::Token,
                        )?,
                    );
                } else {
                    node.children = children;
                    return Ok(Shared::new(node));
                }

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                if let Token {
                    range: _,
                    kind: TokenKind::NumberLiteral(_),
                    ..
                } = &*token
                {
                    children.push(self.next_node(
                        |kind| matches!(kind, TokenKind::NumberLiteral(_)),
                        NodeKind::Literal,
                    )?);
                }

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                if let Token {
                    range: _,
                    kind: TokenKind::RBracket,
                    ..
                } = &*token
                {
                    children.push(
                        self.next_node(
                            |kind| matches!(kind, TokenKind::RBracket),
                            NodeKind::Token,
                        )?,
                    );
                } else {
                    return Err(ParseError::UnexpectedToken(Shared::clone(&token)));
                }

                node.children = children;
                Ok(Shared::new(node))
            }
            Token {
                range: _,
                kind: TokenKind::Selector(s),
                ..
            } if s == ".h" || s == ".code" || s == ".list" => {
                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                match token.kind {
                    TokenKind::LParen => {
                        let mut children: Vec<Shared<Node>> = Vec::with_capacity(64);
                        let mut args = self.parse_args()?;

                        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
                            return Err(ParseError::UnexpectedToken(Shared::clone(&token)));
                        }
                        children.append(&mut args);

                        node.children = children;
                        Ok(Shared::new(node))
                    }
                    _ => Ok(Shared::new(node)),
                }
            }
            _ => Ok(Shared::new(node)),
        }
    }

    fn parse_include(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(100);

        let mut node = Node {
            kind: NodeKind::Include,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(
            |kind| matches!(kind, TokenKind::StringLiteral(_)),
            NodeKind::Literal,
        )?);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_if(
        &mut self,
        leading_trivia: Vec<Trivia>,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::If,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        let mut args = self.parse_args()?;

        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
            return Err(ParseError::UnexpectedToken(Shared::clone(token.unwrap())));
        }

        children.append(&mut args);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, in_loop)?);

        loop {
            if !self.try_next_token(|kind| matches!(kind, TokenKind::Elif)) {
                break;
            }

            let leading_trivia = self.parse_leading_trivia();
            children.push(self.parse_elif(leading_trivia, in_loop)?);
        }

        if !self.try_next_token(|kind| matches!(kind, TokenKind::Else)) {
            node.children = children;
            return Ok(Shared::new(node));
        }

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_else(leading_trivia, in_loop)?);
        node.children = children;

        Ok(Shared::new(node))
    }

    fn parse_elif(
        &mut self,
        leading_trivia: Vec<Trivia>,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Elif,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        let mut args = self.parse_args()?;

        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
            return Err(ParseError::UnexpectedToken(Shared::clone(token.unwrap())));
        }

        children.append(&mut args);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, in_loop)?);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_else(
        &mut self,
        leading_trivia: Vec<Trivia>,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Else,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, in_loop)?);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_node(
        &mut self,
        node_kind: NodeKind,
        leading_trivia: Vec<Trivia>,
    ) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();

        Ok(Shared::new(Node {
            kind: node_kind,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        }))
    }

    fn parse_array(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(64);

        children.push(self.next_node(
            |token_kind| matches!(token_kind, TokenKind::LBracket),
            NodeKind::Token,
        )?);

        let token = match self.tokens.peek() {
            Some(token) => Shared::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        if matches!(token.kind, TokenKind::RBracket) {
            let leading_trivia = self.parse_leading_trivia();
            let token = self.tokens.next().unwrap();
            let trailing_trivia = self.parse_trailing_trivia();
            children.push(Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia,
                children: Vec::new(),
            }));

            return Ok(Shared::new(Node {
                kind: NodeKind::Array,
                token: None,
                leading_trivia: Vec::new(),
                trailing_trivia: Vec::new(),
                children,
            }));
        }

        loop {
            let element_node = {
                let leading_trivia = self.parse_leading_trivia();
                self.parse_expr(leading_trivia, false, false)
            }?;
            let leading_trivia = self.parse_leading_trivia();
            let token = match self.tokens.peek() {
                Some(token) => Shared::clone(token),
                None => return Err(ParseError::UnexpectedEOFDetected),
            };

            match &token.kind {
                TokenKind::Comma => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();

                    children.push(element_node);
                    children.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));
                }
                TokenKind::RBracket => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();

                    children.push(element_node);
                    children.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));

                    break;
                }
                _ => return Err(ParseError::UnexpectedToken(Shared::clone(&token))),
            }
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Array,
            token: None,
            leading_trivia,
            trailing_trivia: Vec::new(),
            children,
        }))
    }

    fn parse_dict(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(64);

        children.push(self.next_node(
            |token_kind| matches!(token_kind, TokenKind::LBrace),
            NodeKind::Token,
        )?);

        let token = match self.tokens.peek() {
            Some(token) => Shared::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        if matches!(token.kind, TokenKind::RBrace) {
            let leading_trivia = self.parse_leading_trivia();
            let token = self.tokens.next().unwrap();
            let trailing_trivia = self.parse_trailing_trivia();
            children.push(Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(token)),
                leading_trivia: Vec::new(),
                trailing_trivia,
                children: Vec::new(),
            }));

            return Ok(Shared::new(Node {
                kind: NodeKind::Dict,
                token: None,
                leading_trivia,
                trailing_trivia: Vec::new(),
                children,
            }));
        }

        loop {
            let key_node = {
                let leading_trivia = self.parse_leading_trivia();
                self.parse_expr(leading_trivia, false, false)
            }?;

            let colon_node = self.next_node(
                |token_kind| matches!(token_kind, TokenKind::Colon),
                NodeKind::Token,
            )?;

            let value_node = {
                let leading_trivia = self.parse_leading_trivia();
                self.parse_expr(leading_trivia, false, false)
            }?;

            // Add key, colon, value individually to children
            children.push(key_node);
            children.push(colon_node);
            children.push(value_node);

            let leading_trivia = self.parse_leading_trivia();
            let token = match self.tokens.peek() {
                Some(token) => Shared::clone(token),
                None => return Err(ParseError::UnexpectedEOFDetected),
            };

            match &token.kind {
                TokenKind::Comma => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();

                    children.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));
                }
                TokenKind::RBrace => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();

                    children.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));

                    break;
                }
                _ => return Err(ParseError::UnexpectedToken(Shared::clone(&token))),
            }
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Dict,
            token: None,
            leading_trivia,
            trailing_trivia: Vec::new(),
            children,
        }))
    }

    fn parse_interpolated_string(
        &mut self,
        leading_trivia: Vec<Trivia>,
    ) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();

        if let Token {
            range: _,
            kind: TokenKind::InterpolatedString(_),
            module_id: _,
        } = &**token
        {
            Ok(Shared::new(Node {
                kind: NodeKind::InterpolatedString,
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia,
                children: Vec::new(),
            }))
        } else {
            Err(ParseError::UnexpectedToken(Shared::clone(token)))
        }
    }

    fn parse_let(
        &mut self,
        leading_trivia: Vec<Trivia>,
        in_loop: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Let,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_ident(leading_trivia)?);

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Equal), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, in_loop)?);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_self(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();

        let node = Node {
            kind: NodeKind::Self_,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        match self.tokens.peek() {
            Some(token) if matches!(token.kind, TokenKind::LBracket) => {
                self.parse_bracket_access(node)
            }
            _ => Ok(Shared::new(node)),
        }
    }

    fn parse_nodes(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();

        Ok(Shared::new(Node {
            kind: NodeKind::Nodes,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        }))
    }

    fn parse_foreach(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Foreach,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Ident(_)), NodeKind::Ident)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();

        children.push(self.parse_ident(leading_trivia)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let (mut program, _) = self.parse_program(false, true);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_while(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::While,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();

        children.push(self.parse_expr(leading_trivia, false, true)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let (mut program, _) = self.parse_program(false, true);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_until(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Until,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();

        children.push(self.parse_expr(leading_trivia, false, true)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let (mut program, _) = self.parse_program(false, true);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_try(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(4);

        let mut node = Node {
            kind: NodeKind::Try,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, false)?);

        if !self.try_next_token(|kind| matches!(kind, TokenKind::Catch)) {
            node.children = children;
            return Ok(Shared::new(node));
        }

        // Parse catch keyword
        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_catch(leading_trivia)?);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_catch(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(4);

        let mut node = Node {
            kind: NodeKind::Catch,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, false)?);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_unary_op(
        &mut self,
        leading_trivia: Vec<Trivia>,
        root: bool,
    ) -> Result<Shared<Node>, ParseError> {
        let operator_token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(2);

        let mut node = Node {
            kind: match &**operator_token {
                Token {
                    range: _,
                    kind: TokenKind::Not,
                    ..
                } => NodeKind::UnaryOp(UnaryOp::Not),
                Token {
                    range: _,
                    kind: TokenKind::Minus,
                    ..
                } => NodeKind::UnaryOp(UnaryOp::Negate),
                _ => return Err(ParseError::UnexpectedToken(Shared::clone(operator_token))),
            },
            token: Some(Shared::clone(operator_token)),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        // Parse the operand expression
        let operand_leading_trivia = self.parse_leading_trivia();
        let operand = self.parse_primary_expr(operand_leading_trivia, root, false)?;
        children.push(operand);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_params(&mut self) -> Result<Vec<Shared<Node>>, ParseError> {
        let mut nodes: Vec<Shared<Node>> = Vec::with_capacity(8);

        nodes.push(self.next_node(
            |token_kind| matches!(token_kind, TokenKind::LParen),
            NodeKind::Token,
        )?);

        let token = match self.tokens.peek() {
            Some(token) => Shared::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        if matches!(token.kind, TokenKind::RParen) {
            let leading_trivia = self.parse_leading_trivia();
            let token = self.tokens.next().unwrap();
            let trailing_trivia = self.parse_trailing_trivia();
            nodes.push(Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia,
                children: Vec::new(),
            }));

            return Ok(nodes);
        }

        loop {
            let param_node = self.parse_param()?;
            let leading_trivia = self.parse_leading_trivia();
            let token = match self.tokens.peek() {
                Some(token) => token,
                None => return Err(ParseError::UnexpectedEOFDetected),
            };

            match &token.kind {
                TokenKind::Comma => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(param_node);
                    nodes.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));
                }
                TokenKind::RParen => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(param_node);
                    nodes.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));

                    break;
                }
                _ => return Err(ParseError::UnexpectedToken(Shared::clone(token))),
            }
        }

        Ok(nodes)
    }

    fn parse_param(&mut self) -> Result<Shared<Node>, ParseError> {
        let leading_trivia = self.parse_leading_trivia();

        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::Ident(_) => self.parse_expr(leading_trivia, false, false),
                _ => Err(ParseError::UnexpectedToken(Shared::clone(token))),
            },
            None => Err(ParseError::UnexpectedEOFDetected),
        }
    }

    fn parse_leading_trivia(&mut self) -> Vec<Trivia> {
        let mut trivia = Vec::with_capacity(100);

        while let Some(token) = self.tokens.peek() {
            match &token.kind {
                TokenKind::Whitespace(_) => trivia.push(Trivia::Whitespace(Shared::clone(token))),
                TokenKind::Tab(_) => trivia.push(Trivia::Tab(Shared::clone(token))),
                TokenKind::Comment(_) => trivia.push(Trivia::Comment(Shared::clone(token))),
                TokenKind::NewLine => trivia.push(Trivia::NewLine),
                _ => break,
            };
            self.tokens.next();
        }

        trivia
    }

    fn try_parse_leading_trivia(
        tokens: &mut Peekable<core::slice::Iter<'a, Shared<Token>>>,
    ) -> Vec<Trivia> {
        let mut trivia = Vec::with_capacity(100);

        while let Some(token) = tokens.peek() {
            match &token.kind {
                TokenKind::Whitespace(_) => trivia.push(Trivia::Whitespace(Shared::clone(token))),
                TokenKind::Tab(_) => trivia.push(Trivia::Tab(Shared::clone(token))),
                TokenKind::Comment(_) => trivia.push(Trivia::Comment(Shared::clone(token))),
                TokenKind::NewLine => trivia.push(Trivia::NewLine),
                _ => break,
            };
            tokens.next();
        }

        trivia
    }

    fn try_next_token(&mut self, match_token_kind: fn(&TokenKind) -> bool) -> bool {
        let tokens = &mut self.tokens.clone();
        Self::try_parse_leading_trivia(tokens);

        let token = tokens.peek().ok_or(ParseError::UnexpectedEOFDetected);

        if token.is_err() {
            return false;
        }

        match_token_kind(&token.unwrap().kind)
    }

    fn parse_trailing_trivia(&mut self) -> Vec<Trivia> {
        let mut trivia = Vec::with_capacity(10);

        while let Some(token) = self.tokens.peek() {
            match &token.kind {
                TokenKind::Whitespace(_) => trivia.push(Trivia::Whitespace(Shared::clone(token))),
                TokenKind::Tab(_) => trivia.push(Trivia::Tab(Shared::clone(token))),
                _ => break,
            }
            self.tokens.next();
        }

        trivia
    }

    fn skip_tokens(&mut self) {
        loop {
            let token = match self.tokens.peek() {
                Some(token) => token,
                None => return,
            };
            match token.kind {
                TokenKind::If
                | TokenKind::While
                | TokenKind::Foreach
                | TokenKind::Let
                | TokenKind::Def
                | TokenKind::Ident(_)
                | TokenKind::Pipe
                | TokenKind::SemiColon
                | TokenKind::End
                | TokenKind::Eof => return,
                _ => {
                    self.tokens.next();
                }
            }
        }
    }

    fn next_token(
        &mut self,
        match_token_kind: fn(&TokenKind) -> bool,
    ) -> Result<Shared<Token>, ParseError> {
        let token = self
            .tokens
            .peek()
            .cloned()
            .ok_or(ParseError::UnexpectedEOFDetected)?;

        if match_token_kind(&token.kind) {
            self.tokens.next();
            Ok(Shared::clone(token))
        } else {
            Err(ParseError::UnexpectedToken(Shared::clone(token)))
        }
    }

    fn next_node(
        &mut self,
        expected_token: fn(&TokenKind) -> bool,
        node_kind: NodeKind,
    ) -> Result<Shared<Node>, ParseError> {
        let leading_trivia = self.parse_leading_trivia();
        let token = self.next_token(expected_token)?;
        let trailing_trivia = self.parse_trailing_trivia();

        Ok(Shared::new(Node {
            kind: node_kind,
            token: Some(Shared::clone(&token)),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::{lexer::token::StringSegment, range::Range};

    use super::*;
    use rstest::rstest;

    fn token(token_kind: TokenKind) -> Token {
        Token {
            range: Range::default(),
            kind: token_kind,
            module_id: 1.into(),
        }
    }

    #[rstest]
    #[case::def(
        vec![
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Comment("test comment".into()))),
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("test comment2".into()))),
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def,
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1)))),
                                         Trivia::Comment(Shared::new(token(TokenKind::Comment("test comment".into()))))],
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("test comment2".into()))))],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Eof,
                            token: Some(Shared::new(token(TokenKind::Eof))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::let_(
        vec![
            Shared::new(token(TokenKind::Whitespace(4))),
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Whitespace(4))),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Let,
                    token: Some(Shared::new(token(TokenKind::Let))),
                    leading_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Equal))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::unexpected_token(
        vec![
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Comment("test comment".into()))),
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("test comment2".into()))),
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def,
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1)))),
                                         Trivia::Comment(Shared::new(token(TokenKind::Comment("test comment".into())))) ],
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("test comment2".into()))))],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Comma)))], 100)
        )
    )]
    #[case::unexpected_eof(
        vec![
            Shared::new(token(TokenKind::Whitespace(4))),
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Whitespace(4))),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            Vec::new(),
            ErrorReporter::with_error(vec![ParseError::UnexpectedEOFDetected], 100)
        )
    )]
    #[case::if_(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch".into()))),
            Shared::new(token(TokenKind::Else)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("else_branch".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If,
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Else,
                            token: Some(Shared::new(token(TokenKind::Else))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("else_branch".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                })],
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::if_elif_else(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition1".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch1".into()))),
            Shared::new(token(TokenKind::Elif)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition2".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch2".into()))),
            Shared::new(token(TokenKind::Else)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("else_branch".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If,
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("condition1".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch1".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Elif,
                            token: Some(Shared::new(token(TokenKind::Elif))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("condition2".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("then_branch2".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Else,
                            token: Some(Shared::new(token(TokenKind::Else))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("else_branch".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::if_only(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition1".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch1".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If,
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("condition1".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch1".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::if_elif_else(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition1".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch1".into()))),
            Shared::new(token(TokenKind::Elif)),
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition2".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch2".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If,
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("condition1".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch1".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Elif,
                            token: Some(Shared::new(token(TokenKind::Elif))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("condition2".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("then_branch2".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::multiple_expr_with_trivia(
        vec![
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("y".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Ident,
                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                    leading_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: Vec::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: Vec::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Ident,
                    token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::args_with_function(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Call,
                            token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::foreach(
        vec![
            Shared::new(token(TokenKind::Foreach)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("item".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("collection".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Foreach,
                    token: Some(Shared::new(token(TokenKind::Foreach))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("item".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("collection".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("comment".into())))), Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::while_(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::Whitespace(4))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::While,
                    token: Some(Shared::new(token(TokenKind::While))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("comment".into())))), Trivia::NewLine],
                            trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::selector1(
        vec![
            Shared::new(token(TokenKind::Selector(".#(2)".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Shared::new(token(TokenKind::Selector(".#(2)".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::selector2(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::selector3(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::selector4(
        vec![
            Shared::new(token(TokenKind::Selector(".list.checked".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Shared::new(token(TokenKind::Selector(".list.checked".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::include(
        vec![
            Shared::new(token(TokenKind::Include)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::StringLiteral("module".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Include,
                    token: Some(Shared::new(token(TokenKind::Include))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("module".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::root_with_token_after_semicolon(
        vec![
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Ident("y".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Ident,
                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::SemiColon))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::with_error(vec![ParseError::UnexpectedEOFDetected], 100)
        )
    )]
    #[case::code_selector(
        vec![
            Shared::new(token(TokenKind::Selector(".code".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Shared::new(token(TokenKind::Selector(".code".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::until(
        vec![
            Shared::new(token(TokenKind::Until)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::Whitespace(4))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Until,
                    token: Some(Shared::new(token(TokenKind::Until))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("comment".into())))), Trivia::NewLine],
                            trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(4))))],
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::call_with_newlines(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("param comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("arg1".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("arg2".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("arg1".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("param comment".into())))), Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("arg2".into())))),
                            leading_trivia: vec![Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: vec![Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::interpolated_string(
        vec![
            Shared::new(token(TokenKind::InterpolatedString(vec![StringSegment::Ident("val".into(), Range::default()), StringSegment::Text("hello".into(), Range::default())]))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::InterpolatedString,
                    token: Some(Shared::new(token(TokenKind::InterpolatedString(vec![StringSegment::Ident("val".into(), Range::default()), StringSegment::Text("hello".into(), Range::default())])))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::nodes(
        vec![
            Shared::new(token(TokenKind::Nodes)),
            Shared::new(token(TokenKind::Whitespace(1))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Nodes,
                    token: Some(Shared::new(token(TokenKind::Nodes))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::fn_with_program(
        vec![
            Shared::new(token(TokenKind::Fn)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("y".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn,
                    token: Some(Shared::new(token(TokenKind::Fn))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Ident,
                    token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::fn_with_parameter_and_program(
        vec![
            Shared::new(token(TokenKind::Fn)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("param".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn,
                    token: Some(Shared::new(token(TokenKind::Fn))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("param".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),

            ],
            ErrorReporter::default()
        )
    )]
    #[case::fn_with_multiple_parameters(
        vec![
            Shared::new(token(TokenKind::Fn)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("param1".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("param2".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("result".into()))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Fn,
                    token: Some(Shared::new(token(TokenKind::Fn))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("param1".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("param2".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("result".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::array_empty(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::array_with_single_element(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::array_with_multiple_elements(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::StringLiteral("hello".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("hello".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::array_with_trivia(
        vec![
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("array element".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Whitespace(1))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array,
                    token: None,
                    leading_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    trailing_trivia: vec![],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: vec![
                                Trivia::NewLine,
                                Trivia::Comment(Shared::new(token(TokenKind::Comment("array element".into())))),
                                Trivia::NewLine
                            ],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: vec![Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: vec![Trivia::NewLine],
                                trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::array_nested(
        vec![
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Array,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Array,
                            token: None,
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Array,
                            token: None,
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::dict_empty(
        vec![
            Shared::new(token(TokenKind::LBrace)),
            Shared::new(token(TokenKind::RBrace)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Dict,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBrace))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBrace))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::dict_with_single_pair(
        vec![
            Shared::new(token(TokenKind::LBrace)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::RBrace)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Dict,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBrace))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBrace))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::dict_with_multiple_pairs(
        vec![
            Shared::new(token(TokenKind::LBrace)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::StringLiteral("foo".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::RBrace)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Dict,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBrace))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Comma))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("foo".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBrace))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::eq_eq(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::EqEq)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Equal),
                    token: Some(Shared::new(token(TokenKind::EqEq))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::ne_eq(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::NeEq)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::NotEqual),
                    token: Some(Shared::new(token(TokenKind::NeEq))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::plus(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Plus),
                    token: Some(Shared::new(token(TokenKind::Plus))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::coalesce(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Coalesce)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Coalesce),
                    token: Some(Shared::new(token(TokenKind::Coalesce))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::lt(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Lt)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Lt),
                    token: Some(Shared::new(token(TokenKind::Lt))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::lte(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Lte)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Lte),
                    token: Some(Shared::new(token(TokenKind::Lte))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::gt(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Gt)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Gt),
                    token: Some(Shared::new(token(TokenKind::Gt))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::gte(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Gte)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Gte),
                    token: Some(Shared::new(token(TokenKind::Gte))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::range(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::RangeOp)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::RangeOp),
                    token: Some(Shared::new(token(TokenKind::RangeOp))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::minus(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Minus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Minus),
                    token: Some(Shared::new(token(TokenKind::Minus))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::division(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Slash)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Division),
                    token: Some(Shared::new(token(TokenKind::Slash))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::percent(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Percent)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Modulo),
                    token: Some(Shared::new(token(TokenKind::Percent))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::multiplication(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Asterisk)),
            Shared::new(token(TokenKind::Ident("b".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Multiplication),
                    token: Some(Shared::new(token(TokenKind::Asterisk))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::multiple_binary_ops(
        vec![
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::Minus)),
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::Asterisk)),
            Shared::new(token(TokenKind::Ident("d".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Multiplication),
                    token: Some(Shared::new(token(TokenKind::Asterisk))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::BinaryOp(BinaryOp::Minus),
                            token: Some(Shared::new(token(TokenKind::Minus))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::BinaryOp(BinaryOp::Plus),
                                    token: Some(Shared::new(token(TokenKind::Plus))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: vec![
                                        Shared::new(Node {
                                            kind: NodeKind::Ident,
                                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                            leading_trivia: Vec::new(),
                                            trailing_trivia: Vec::new(),
                                            children: Vec::new(),
                                        }),
                                        Shared::new(Node {
                                            kind: NodeKind::Ident,
                                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                            leading_trivia: Vec::new(),
                                            trailing_trivia: Vec::new(),
                                            children: Vec::new(),
                                        }),
                                    ],
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("d".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::binary_ops_with_trivia(
        vec![
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("y".into()))),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Minus)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::Ident("z".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::BinaryOp(BinaryOp::Minus),
                    token: Some(Shared::new(token(TokenKind::Minus))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::BinaryOp(BinaryOp::Plus),
                            token: Some(Shared::new(token(TokenKind::Plus))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("y".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("z".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::string_literal_with_escape_sequences(
        vec![
            Shared::new(token(TokenKind::StringLiteral("\\x1b[2J\\x1b[H".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Literal,
                    token: Some(Shared::new(token(TokenKind::StringLiteral("\\x1b[2J\\x1b[H".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::not_operation(
        vec![
            Shared::new(token(TokenKind::Not)),
            Shared::new(token(TokenKind::BoolLiteral(true))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::UnaryOp(UnaryOp::Not),
                    token: Some(Shared::new(token(TokenKind::Not))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::BoolLiteral(true)))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::break_in_loop(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::BoolLiteral(true))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Break)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::While,
                    token: Some(Shared::new(token(TokenKind::While))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::BoolLiteral(true)))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Break,
                            token: Some(Shared::new(token(TokenKind::Break))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::continue_in_loop(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::BoolLiteral(true))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Continue)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::While,
                    token: Some(Shared::new(token(TokenKind::While))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::BoolLiteral(true)))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Continue,
                            token: Some(Shared::new(token(TokenKind::Continue))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::break_outside_loop(
        vec![
            Shared::new(token(TokenKind::Break)),
        ],
        (
            Vec::new(),
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Break))), ParseError::UnexpectedEOFDetected], 100)
        )
    )]
    #[case::continue_outside_loop(
        vec![
            Shared::new(token(TokenKind::Continue)),
        ],
        (
            Vec::new(),
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Continue))), ParseError::UnexpectedEOFDetected], 100)
        )
    )]
    #[case::bracket_access_with_number(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(5.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(5.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::bracket_access_with_string(
        vec![
            Shared::new(token(TokenKind::Ident("dict".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::StringLiteral("key".into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("dict".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("key".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::bracket_access_error_missing_rbracket(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(5.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            Vec::new(),
            ErrorReporter::with_error(vec![ParseError::ExpectedClosingBracket(Shared::new(token(TokenKind::Eof))), ParseError::UnexpectedToken(Shared::new(token(TokenKind::Eof)))], 100)
        )
    )]
    #[case::call_with_not_ident_arg(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Not)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::UnaryOp(UnaryOp::Not),
                            token: Some(Shared::new(token(TokenKind::Not))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::slice_access_with_numbers(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::slice_access_with_variables(
        vec![
            Shared::new(token(TokenKind::Ident("items".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::Ident("start".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("end".into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("items".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("start".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("end".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
                Shared::new(Node {
                    kind: NodeKind::Eof,
                    token: Some(Shared::new(token(TokenKind::Eof))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::group_expr_single_ident(
        vec![
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Group,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::group_expr_binary_op(
        vec![
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Group,
                    token: None,
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::BinaryOp(BinaryOp::Plus),
                            token: Some(Shared::new(token(TokenKind::Plus))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::group_expr_with_trivia(
        vec![
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Whitespace(1))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Group,
                    token: None,
                    leading_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: vec![Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::group_expr_missing_rparen(
        vec![
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            Vec::new(),
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Eof)))], 100)
        )
    )]
    #[case::if_with_grouped_binary_op_condition(
        vec![
            Shared::new(token(TokenKind::If)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("then_branch".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::If,
                    token: Some(Shared::new(token(TokenKind::If))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Group,
                            token: None,
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LParen))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::BinaryOp(BinaryOp::Plus),
                                    token: Some(Shared::new(token(TokenKind::Plus))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: vec![
                                        Shared::new(Node {
                                            kind: NodeKind::Ident,
                                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
                                            leading_trivia: Vec::new(),
                                            trailing_trivia: Vec::new(),
                                            children: Vec::new(),
                                        }),
                                        Shared::new(Node {
                                            kind: NodeKind::Ident,
                                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                                            leading_trivia: Vec::new(),
                                            trailing_trivia: Vec::new(),
                                            children: Vec::new(),
                                        }),
                                    ],
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RParen))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("then_branch".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::self_bracket_access_with_number(
        vec![
            Shared::new(token(TokenKind::Self_)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Self_))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::call_with_index_access(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RBracket)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RBracket))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::negate(
        vec![
            Shared::new(token(TokenKind::Minus)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::UnaryOp(UnaryOp::Negate),
                    token: Some(Shared::new(token(TokenKind::Minus))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::do_block_simple(
        vec![
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Ident("test".into()))),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block,
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("test".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::do_block_with_whitespace(
        vec![
            Shared::new(token(TokenKind::Whitespace(2))),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Tab(1))),
            Shared::new(token(TokenKind::Ident("expression".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block,
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("expression".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Tab(Shared::new(token(TokenKind::Tab(1))))],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: vec![Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::do_block_with_comment(
        vec![
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Comment("inside block".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("value".into()))),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block,
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("value".into())))),
                            leading_trivia: vec![Trivia::Comment(Shared::new(token(TokenKind::Comment("inside block".into())))), Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::do_block_empty(
        vec![
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block,
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![],
                }),
            ],
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::End))), ParseError::UnexpectedEOFDetected], 100)
        )
    )]
    #[case::do_block_nested(
        vec![
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Ident("inner".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Block,
                    token: Some(Shared::new(token(TokenKind::Do))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Block,
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("inner".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::End,
                                    token: Some(Shared::new(token(TokenKind::End))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::try_catch(
        vec![
            Shared::new(token(TokenKind::Try)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("try_body".into()))),
            Shared::new(token(TokenKind::Catch)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("catch_body".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Try,
                    token: Some(Shared::new(token(TokenKind::Try))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("try_body".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Catch,
                            token: Some(Shared::new(token(TokenKind::Catch))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("catch_body".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::try_without_catch(
        vec![
            Shared::new(token(TokenKind::Try)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("try_body".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Try,
                    token: Some(Shared::new(token(TokenKind::Try))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("try_body".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::call_with_question_mark(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Question)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Question))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::index_access_with_function_call(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(0.into()))),
            Shared::new(token(TokenKind::RBracket)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::CallDynamic,
                    token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Call,
                            token: Some(Shared::new(token(TokenKind::Ident("arr".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::LBracket))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(0.into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::RBracket))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::LParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::RParen))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    fn test_parse(
        #[case] input: Vec<Shared<Token>>,
        #[case] expected: (Vec<Shared<Node>>, ErrorReporter),
    ) {
        let (nodes, errors) = Parser::new(input.iter()).parse();
        assert_eq!(errors, expected.1);
        assert_eq!(nodes, expected.0);
    }

    #[test]
    fn test_error_reporter_error_ranges() {
        let text = "def foo():\n bar()\n";

        let mut reporter = ErrorReporter::new(100);

        let unexpected_token = Shared::new(Token {
            range: Range {
                start: Position { line: 1, column: 4 },
                end: Position { line: 1, column: 7 },
            },
            kind: TokenKind::Ident("foo".into()),
            module_id: 1.into(),
        });
        reporter.report(ParseError::UnexpectedToken(Shared::clone(
            &unexpected_token,
        )));

        let insufficient_token = Shared::new(Token {
            range: Range {
                start: Position { line: 1, column: 4 },
                end: Position { line: 1, column: 7 },
            },
            kind: TokenKind::Ident("bar".into()),
            module_id: 1.into(),
        });
        reporter.report(ParseError::InsufficientTokens(Shared::clone(
            &insufficient_token,
        )));

        reporter.report(ParseError::UnexpectedEOFDetected);

        let ranges = reporter.error_ranges(text);

        assert_eq!(ranges.len(), 3);

        assert_eq!(ranges[0].1.start.line, 1);
        assert_eq!(ranges[0].1.start.column, 4);
        assert_eq!(ranges[0].1.end.line, 1);
        assert_eq!(ranges[0].1.end.column, 7);

        assert_eq!(ranges[1].1.start.line, 1);
        assert_eq!(ranges[1].1.start.column, 4);
        assert_eq!(ranges[1].1.end.line, 1);
        assert_eq!(ranges[1].1.end.column, 7);

        assert_eq!(ranges[2].1.start.line, 2);
        assert_eq!(ranges[2].1.start.column, 6);
        assert_eq!(ranges[2].1.end.line, 2);
        assert_eq!(ranges[2].1.end.column, 6);
    }

    #[test]
    fn test_error_reporter_has_errors() {
        let mut reporter = ErrorReporter::new(100);
        assert!(!reporter.has_errors());

        reporter.report(ParseError::UnexpectedEOFDetected);
        assert!(reporter.has_errors());
    }

    #[test]
    fn test_error_reporter_max_errors() {
        let mut reporter = ErrorReporter::new(2);

        reporter.report(ParseError::UnexpectedToken(Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Ident("foo".into()),
            module_id: 1.into(),
        })));
        reporter.report(ParseError::UnexpectedToken(Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Ident("bar".into()),
            module_id: 1.into(),
        })));
        reporter.report(ParseError::UnexpectedEOFDetected);

        assert_eq!(reporter.errors.len(), 2);
    }

    #[test]
    fn test_error_reporter_unexpected_eof_detected() {
        let mut reporter = ErrorReporter::new(2);

        reporter.report(ParseError::UnexpectedEOFDetected);
        reporter.report(ParseError::UnexpectedEOFDetected);

        assert_eq!(reporter.errors.len(), 1);
    }

    #[test]
    fn test_error_reporter_display() {
        let mut reporter = ErrorReporter::new(100);

        reporter.report(ParseError::UnexpectedEOFDetected);
        let token = Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Ident("foo".into()),
            module_id: 1.into(),
        });
        reporter.report(ParseError::UnexpectedToken(token));

        let display = format!("{}", reporter);
        assert!(display.contains("Unexpected EOF detected"));
        assert!(display.contains("Unexpected token"));
    }
}
