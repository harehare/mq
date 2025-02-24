use std::{fmt::Display, iter::Peekable, sync::Arc};

use crate::{Position, Range, Token, TokenKind};

use super::{
    error::ParseError,
    node::{Node, NodeKind, Trivia},
};
use itertools::Itertools;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Default, Error)]
pub struct ErrorReporter {
    errors: Vec<ParseError>,
    max_errors: usize,
}

impl Display for ErrorReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Formatting Failed: {}", &self.errors.iter().join(", "))
    }
}

impl ErrorReporter {
    pub fn new(max_errors: usize) -> Self {
        Self {
            errors: Vec::new(),
            max_errors,
        }
    }

    pub fn report(&mut self, error: ParseError) {
        if self.errors.len() < self.max_errors {
            self.errors.push(error);
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn error_ranges(&self, text: &str) -> Vec<(String, Range)> {
        self.errors
            .iter()
            .map(|e| {
                (
                    e.to_string(),
                    match e {
                        ParseError::UnexpectedToken(token) => token.range.clone(),
                        ParseError::InsufficientTokens(token) => token.range.clone(),
                        ParseError::UnexpectedEOFDetected => Range {
                            start: Position {
                                line: text.lines().count() as u32,
                                column: text.lines().last().unwrap().len(),
                            },
                            end: Position {
                                line: text.lines().count() as u32,
                                column: text.lines().last().unwrap().len(),
                            },
                        },
                    },
                )
            })
            .collect_vec()
    }
}

pub struct Parser<'a> {
    tokens: Peekable<core::slice::Iter<'a, Arc<Token>>>,
    errors: ErrorReporter,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: core::slice::Iter<'a, Arc<Token>>) -> Self {
        Self {
            tokens: tokens.peekable(),
            errors: ErrorReporter::new(100),
        }
    }

    pub fn parse(&mut self) -> (Vec<Arc<Node>>, ErrorReporter) {
        self.parse_program(true)
    }

    #[inline(always)]
    fn parse_program(&mut self, root: bool) -> (Vec<Arc<Node>>, ErrorReporter) {
        let mut nodes: Vec<Arc<Node>> = Vec::with_capacity(self.tokens.len());
        let mut leading_trivia = self.parse_leading_trivia();

        while self.tokens.peek().is_some() {
            let node = self.parse_expr(leading_trivia);
            match node {
                Ok(node) => nodes.push(Arc::clone(&node)),
                Err(e) => {
                    self.skip_tokens();
                    self.errors.report(e)
                }
            }

            leading_trivia = self.parse_leading_trivia();

            let token = match self.tokens.peek() {
                Some(token) => Arc::clone(token),
                None => break,
            };

            match &*token {
                Token {
                    range: _,
                    kind: TokenKind::Eof,
                    ..
                } => break,
                Token {
                    range: _,
                    kind: TokenKind::Pipe,
                    ..
                } => {
                    self.tokens.next();
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(Arc::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Arc::clone(&token)),
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
                } => {
                    self.tokens.next();
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(Arc::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Arc::clone(&token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    }));

                    if root {
                        self.parse_leading_trivia();

                        if let Some(token) = self.tokens.peek() {
                            if matches!(token.kind, TokenKind::Eof) {
                                break;
                            } else {
                                self.errors.report(ParseError::UnexpectedEOFDetected);
                            }
                        }
                    }

                    break;
                }
                _ => {}
            }
        }

        (nodes, self.errors.clone())
    }

    #[inline(always)]
    fn parse_expr(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        if let Some(token) = self.tokens.peek().cloned() {
            match &**token {
                Token {
                    range: _,
                    kind: TokenKind::Def,
                    ..
                } => self.parse_def(leading_trivia),
                Token {
                    range: _,
                    kind: TokenKind::If,
                    ..
                } => self.parse_if(leading_trivia),
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
                } => self.parse_let(leading_trivia),
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
                } => self.parse_literal(leading_trivia),
                token => Err(ParseError::UnexpectedToken(Arc::new(token.clone()))),
            }
        } else {
            Err(ParseError::UnexpectedEOFDetected)
        }
    }

    #[inline(always)]
    fn parse_ident(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(100);
        let mut node = Node {
            kind: NodeKind::Ident,
            token: Some(Arc::clone(token)),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        match self.tokens.peek() {
            Some(token) if matches!(token.kind, TokenKind::LParen) => {
                children.push(self.next_node(
                    |token_kind| matches!(token_kind, TokenKind::LParen),
                    NodeKind::Token,
                )?);

                let mut args = self.parse_args()?;
                children.append(&mut args);

                children.push(self.next_node(
                    |token_kind| matches!(token_kind, TokenKind::RParen),
                    NodeKind::Token,
                )?);

                if let Some(token) = self.tokens.peek() {
                    if matches!(token.kind, TokenKind::Question) {
                        children.push(self.next_node(
                            |token_kind| matches!(token_kind, TokenKind::Question),
                            NodeKind::Token,
                        )?);
                    }
                }

                node.kind = NodeKind::Call;
                node.children = children;
                Ok(Arc::new(node))
            }
            _ => Ok(Arc::new(node)),
        }
    }

    #[inline(always)]
    fn parse_args(&mut self) -> Result<Vec<Arc<Node>>, ParseError> {
        let mut nodes: Vec<Arc<Node>> = Vec::new();

        let token = match self.tokens.peek() {
            Some(token) => Arc::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        if matches!(token.kind, TokenKind::RParen) {
            return Ok(nodes);
        }

        loop {
            let arg_node = self.parse_arg()?;

            let token = match self.tokens.peek() {
                Some(token) => Arc::clone(token),
                None => return Err(ParseError::UnexpectedEOFDetected),
            };

            match &token.kind {
                TokenKind::Comma => {
                    nodes.push(arg_node);
                    nodes.push(
                        self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?,
                    );
                }
                TokenKind::RParen => {
                    nodes.push(arg_node);
                    break;
                }
                _ => return Err(ParseError::UnexpectedToken(Arc::clone(&token))),
            }
        }

        Ok(nodes)
    }

    #[inline(always)]
    fn parse_arg(&mut self) -> Result<Arc<Node>, ParseError> {
        let leading_trivia = self.parse_leading_trivia();
        let token = match self.tokens.peek() {
            Some(token) => Arc::clone(token),
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
                kind: TokenKind::Self_,
                ..
            } => self.parse_expr(leading_trivia),
            _ => Err(ParseError::UnexpectedToken(Arc::clone(&token))),
        }
    }

    #[inline(always)]
    fn parse_def(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(100);

        let mut node = Node {
            kind: NodeKind::Def,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Ident(_)), NodeKind::Ident)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);

        let mut params = self.parse_params()?;
        children.append(&mut params);

        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let (mut program, _) = self.parse_program(false);
        children.append(&mut program);

        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_selector(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();

        let mut node = Node {
            kind: NodeKind::Selector,
            token: Some(Arc::clone(token)),
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
                let mut children: Vec<Arc<Node>> = Vec::with_capacity(6);

                // []
                children.push(
                    self.next_node(|kind| matches!(kind, TokenKind::LBracket), NodeKind::Token)?,
                );

                let token = match self.tokens.peek() {
                    Some(token) => Arc::clone(token),
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
                    Some(token) => Arc::clone(token),
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
                    return Err(ParseError::UnexpectedToken(Arc::clone(&token)));
                }

                let token = match self.tokens.peek() {
                    Some(token) => Arc::clone(token),
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
                    return Ok(Arc::new(node));
                }

                let token = match self.tokens.peek() {
                    Some(token) => Arc::clone(token),
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
                    Some(token) => Arc::clone(token),
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
                    return Err(ParseError::UnexpectedToken(Arc::clone(&token)));
                }

                node.children = children;
                Ok(Arc::new(node))
            }
            _ => Ok(Arc::new(node)),
        }
    }

    #[inline(always)]
    fn parse_include(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(100);

        let mut node = Node {
            kind: NodeKind::Include,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(
            |kind| matches!(kind, TokenKind::StringLiteral(_)),
            NodeKind::Literal,
        )?);

        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_if(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::If,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);

        let mut args = self.parse_args()?;

        if args.len() != 1 {
            return Err(ParseError::UnexpectedToken(Arc::clone(token.unwrap())));
        }

        children.append(&mut args);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia)?);

        let mut leading_trivia = self.parse_leading_trivia();

        loop {
            let token = match self.tokens.peek().cloned() {
                Some(token) => token,
                None => return Err(ParseError::UnexpectedEOFDetected),
            };

            if matches!(token.kind, TokenKind::Else) {
                break;
            }

            children.push(self.parse_elif(leading_trivia)?);
            leading_trivia = self.parse_leading_trivia();
        }

        children.push(self.parse_else(leading_trivia)?);
        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_elif(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Elif,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);

        let mut args = self.parse_args()?;

        if args.len() != 1 {
            return Err(ParseError::UnexpectedToken(Arc::clone(token.unwrap())));
        }

        children.append(&mut args);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia)?);

        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_else(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Else,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia)?);

        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_literal(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();

        Ok(Arc::new(Node {
            kind: NodeKind::Literal,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        }))
    }

    #[inline(always)]
    fn parse_let(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Let,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_ident(leading_trivia)?);

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Equal), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia)?);

        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_self(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();

        Ok(Arc::new(Node {
            kind: NodeKind::Self_,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        }))
    }

    #[inline(always)]
    fn parse_foreach(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Foreach,
            token: Some(Arc::clone(token.unwrap())),
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

        let (mut program, _) = self.parse_program(false);

        children.append(&mut program);

        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_while(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::While,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let (mut program, _) = self.parse_program(false);

        children.append(&mut program);

        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_until(&mut self, leading_trivia: Vec<Trivia>) -> Result<Arc<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Arc<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Until,
            token: Some(Arc::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        let (mut program, _) = self.parse_program(false);

        children.append(&mut program);

        node.children = children;
        Ok(Arc::new(node))
    }

    #[inline(always)]
    fn parse_params(&mut self) -> Result<Vec<Arc<Node>>, ParseError> {
        let mut nodes: Vec<Arc<Node>> = Vec::new();
        let token = match self.tokens.peek() {
            Some(token) => token,
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        if matches!(token.kind, TokenKind::RParen) {
            return Ok(nodes);
        }

        loop {
            let param_node = self.parse_param()?;

            let token = match self.tokens.peek() {
                Some(token) => token,
                None => return Err(ParseError::UnexpectedEOFDetected),
            };

            match &token.kind {
                TokenKind::Comma => {
                    nodes.push(param_node);
                    nodes.push(
                        self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?,
                    );
                }
                TokenKind::RParen => {
                    nodes.push(param_node);
                    break;
                }
                _ => return Err(ParseError::UnexpectedToken(Arc::clone(token))),
            }
        }

        Ok(nodes)
    }

    #[inline(always)]
    fn parse_param(&mut self) -> Result<Arc<Node>, ParseError> {
        let leading_trivia = self.parse_leading_trivia();

        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::Ident(_) => self.parse_expr(leading_trivia),
                _ => Err(ParseError::UnexpectedToken(Arc::clone(token))),
            },
            None => Err(ParseError::UnexpectedEOFDetected),
        }
    }

    #[inline(always)]
    fn parse_leading_trivia(&mut self) -> Vec<Trivia> {
        let mut trivia = Vec::with_capacity(100);

        while let Some(token) = self.tokens.peek() {
            match &token.kind {
                TokenKind::Whitespace(_) => trivia.push(Trivia::Whitespace(Arc::clone(token))),
                TokenKind::Tab(_) => trivia.push(Trivia::Tab(Arc::clone(token))),
                TokenKind::Comment(_) => trivia.push(Trivia::Comment(Arc::clone(token))),
                TokenKind::NewLine => trivia.push(Trivia::NewLine),
                _ => break,
            };
            self.tokens.next();
        }

        trivia
    }

    #[inline(always)]
    fn parse_trailing_trivia(&mut self) -> Vec<Trivia> {
        let mut trivia = Vec::with_capacity(10);

        while let Some(token) = self.tokens.peek() {
            match &token.kind {
                TokenKind::Whitespace(_) => trivia.push(Trivia::Whitespace(Arc::clone(token))),
                TokenKind::Tab(_) => trivia.push(Trivia::Tab(Arc::clone(token))),
                _ => break,
            }
            self.tokens.next();
        }

        trivia
    }

    #[inline(always)]
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
                | TokenKind::Eof => return,
                _ => {
                    self.tokens.next();
                }
            }
        }
    }

    #[inline(always)]
    fn next_token(
        &mut self,
        match_token_kind: fn(&TokenKind) -> bool,
    ) -> Result<Arc<Token>, ParseError> {
        let token = self
            .tokens
            .peek()
            .cloned()
            .ok_or_else(|| ParseError::UnexpectedEOFDetected)?;

        if match_token_kind(&token.kind) {
            self.tokens.next();
            Ok(Arc::clone(token))
        } else {
            Err(ParseError::UnexpectedToken(Arc::clone(token)))
        }
    }

    #[inline(always)]
    fn next_node(
        &mut self,
        expected_token: fn(&TokenKind) -> bool,
        node_kind: NodeKind,
    ) -> Result<Arc<Node>, ParseError> {
        let leading_trivia = self.parse_leading_trivia();
        let token = self.next_token(expected_token)?;
        let trailing_trivia = self.parse_trailing_trivia();

        Ok(Arc::new(Node {
            kind: node_kind,
            token: Some(Arc::clone(&token)),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::range::Range;

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
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::Comment("test comment".into()))),
            Arc::new(token(TokenKind::Def)),
            Arc::new(token(TokenKind::NewLine)),
            Arc::new(token(TokenKind::Comment("test comment2".into()))),
            Arc::new(token(TokenKind::Ident("foo".into()))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::StringLiteral("test".into()))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Def,
                    token: Some(Arc::new(token(TokenKind::Def))),
                    leading_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1)))),
                                         Trivia::Comment(Arc::new(token(TokenKind::Comment("test comment".into()))))],
                    trailing_trivia: vec![],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("foo".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Arc::new(token(TokenKind::Comment("test comment2".into()))))],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::LParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::RParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::Colon))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Arc::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::let_(
        vec![
            Arc::new(token(TokenKind::Whitespace(4))),
            Arc::new(token(TokenKind::Let)),
            Arc::new(token(TokenKind::Whitespace(4))),
            Arc::new(token(TokenKind::Ident("x".into()))),
            Arc::new(token(TokenKind::Equal)),
            Arc::new(token(TokenKind::NumberLiteral(42.into()))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Let,
                    token: Some(Arc::new(token(TokenKind::Let))),
                    leading_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(4))))],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(4))))],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::Equal))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Arc::new(token(TokenKind::NumberLiteral(42.into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::unexpected_token(
        vec![
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::Comment("test comment".into()))),
            Arc::new(token(TokenKind::Def)),
            Arc::new(token(TokenKind::NewLine)),
            Arc::new(token(TokenKind::Comment("test comment2".into()))),
            Arc::new(token(TokenKind::Ident("foo".into()))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::StringLiteral("test".into()))),
            Arc::new(token(TokenKind::Comma)), // Unexpected token
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Def,
                    token: Some(Arc::new(token(TokenKind::Def))),
                    leading_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1)))),
                                         Trivia::Comment(Arc::new(token(TokenKind::Comment("test comment".into())))) ],
                    trailing_trivia: vec![],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("foo".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Arc::new(token(TokenKind::Comment("test comment2".into()))))],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::LParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::RParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::Colon))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Arc::new(token(TokenKind::StringLiteral("test".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![ParseError::UnexpectedToken(Arc::new(token(TokenKind::Comma)))], max_errors: 100 }
        )
    )]
    #[case::unexpected_eof(
        vec![
            Arc::new(token(TokenKind::Whitespace(4))),
            Arc::new(token(TokenKind::Let)),
            Arc::new(token(TokenKind::Whitespace(4))),
            Arc::new(token(TokenKind::Ident("x".into()))),
            Arc::new(token(TokenKind::Equal)),
            // Missing expression token
        ],
        (
            vec![],
            ErrorReporter { errors: vec![ParseError::UnexpectedEOFDetected], max_errors: 100 }
        )
    )]
    #[case::if_(
        vec![
            Arc::new(token(TokenKind::If)),
            Arc::new(token(TokenKind::Whitespace(2))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::Ident("condition".into()))),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::Ident("then_branch".into()))),
            Arc::new(token(TokenKind::Else)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::Ident("else_branch".into()))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::If,
                    token: Some(Arc::new(token(TokenKind::If))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(2))))],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::LParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::RParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::Colon))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("then_branch".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Else,
                            token: Some(Arc::new(token(TokenKind::Else))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![
                                Arc::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Arc::new(token(TokenKind::Colon))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                                Arc::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Arc::new(token(TokenKind::Ident("else_branch".into())))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                })],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::if_elif_else(
        vec![
            Arc::new(token(TokenKind::If)),
            Arc::new(token(TokenKind::Whitespace(2))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::Ident("condition1".into()))),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::Ident("then_branch1".into()))),
            Arc::new(token(TokenKind::Elif)),
            Arc::new(token(TokenKind::Whitespace(2))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::Ident("condition2".into()))),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::Ident("then_branch2".into()))),
            Arc::new(token(TokenKind::Else)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::Ident("else_branch".into()))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::If,
                    token: Some(Arc::new(token(TokenKind::If))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(2))))],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::LParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("condition1".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::RParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::Colon))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("then_branch1".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Elif,
                            token: Some(Arc::new(token(TokenKind::Elif))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(2))))],
                            children: vec![
                                Arc::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Arc::new(token(TokenKind::LParen))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                                Arc::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Arc::new(token(TokenKind::Ident("condition2".into())))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                                Arc::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Arc::new(token(TokenKind::RParen))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                                Arc::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Arc::new(token(TokenKind::Colon))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                                Arc::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Arc::new(token(TokenKind::Ident("then_branch2".into())))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                            ],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Else,
                            token: Some(Arc::new(token(TokenKind::Else))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![
                                Arc::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Arc::new(token(TokenKind::Colon))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                                Arc::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Arc::new(token(TokenKind::Ident("else_branch".into())))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                            ],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::multiple_expr_with_trivia(
        vec![
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::Ident("x".into()))),
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::Pipe)),
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::Ident("y".into()))),
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Ident,
                    token: Some(Arc::new(token(TokenKind::Ident("x".into())))),
                    leading_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1))))],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1))))],
                    children: vec![],
                }),
                Arc::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Arc::new(token(TokenKind::Pipe))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1))))],
                    children: vec![],
                }),
                Arc::new(Node {
                    kind: NodeKind::Ident,
                    token: Some(Arc::new(token(TokenKind::Ident("y".into())))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1))))],
                    children: vec![],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::args_with_function(
        vec![
            Arc::new(token(TokenKind::Ident("foo".into()))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::Ident("bar".into()))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Call,
                    token: Some(Arc::new(token(TokenKind::Ident("foo".into())))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::LParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Call,
                            token: Some(Arc::new(token(TokenKind::Ident("bar".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![
                                Arc::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Arc::new(token(TokenKind::LParen))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                                Arc::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Arc::new(token(TokenKind::RParen))),
                                    leading_trivia: vec![],
                                    trailing_trivia: vec![],
                                    children: vec![],
                                }),
                            ],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::RParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::foreach(
        vec![
            Arc::new(token(TokenKind::Foreach)),
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::Ident("item".into()))),
            Arc::new(token(TokenKind::Comma)),
            Arc::new(token(TokenKind::Ident("collection".into()))),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::NewLine)),
            Arc::new(token(TokenKind::Comment("comment".into()))),
            Arc::new(token(TokenKind::NewLine)),
            Arc::new(token(TokenKind::Ident("body".into()))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Foreach,
                    token: Some(Arc::new(token(TokenKind::Foreach))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::LParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("item".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::Comma))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("collection".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::RParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::Colon))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Arc::new(token(TokenKind::Comment("comment".into())))), Trivia::NewLine],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::while_(
        vec![
            Arc::new(token(TokenKind::While)),
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::LParen)),
            Arc::new(token(TokenKind::Ident("condition".into()))),
            Arc::new(token(TokenKind::RParen)),
            Arc::new(token(TokenKind::Colon)),
            Arc::new(token(TokenKind::NewLine)),
            Arc::new(token(TokenKind::Comment("comment".into()))),
            Arc::new(token(TokenKind::NewLine)),
            Arc::new(token(TokenKind::Ident("body".into()))),
            Arc::new(token(TokenKind::Whitespace(4))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::While,
                    token: Some(Arc::new(token(TokenKind::While))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::LParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("condition".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::RParen))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::Colon))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Arc::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Arc::new(token(TokenKind::Comment("comment".into())))), Trivia::NewLine],
                            trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(4))))],
                            children: vec![],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::selector1(
        vec![
            Arc::new(token(TokenKind::Selector(".#(2)".into()))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Arc::new(token(TokenKind::Selector(".#(2)".into())))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![],
                    children: vec![],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::selector2(
        vec![
            Arc::new(token(TokenKind::Selector(".".into()))),
            Arc::new(token(TokenKind::LBracket)),
            Arc::new(token(TokenKind::NumberLiteral(2.into()))),
            Arc::new(token(TokenKind::RBracket)),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Arc::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::LBracket))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Arc::new(token(TokenKind::NumberLiteral(2.into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                        Arc::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Arc::new(token(TokenKind::RBracket))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::include(
        vec![
            Arc::new(token(TokenKind::Include)),
            Arc::new(token(TokenKind::Whitespace(1))),
            Arc::new(token(TokenKind::StringLiteral("module".into()))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Include,
                    token: Some(Arc::new(token(TokenKind::Include))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![Trivia::Whitespace(Arc::new(token(TokenKind::Whitespace(1))))],
                    children: vec![
                        Arc::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Arc::new(token(TokenKind::StringLiteral("module".into())))),
                            leading_trivia: vec![],
                            trailing_trivia: vec![],
                            children: vec![],
                        }),
                    ],
                }),
            ],
            ErrorReporter { errors: vec![], max_errors: 100 }
        )
    )]
    #[case::root_with_token_after_semicolon(
        vec![
            Arc::new(token(TokenKind::Ident("x".into()))),
            Arc::new(token(TokenKind::SemiColon)),
            Arc::new(token(TokenKind::Ident("y".into()))),
            Arc::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Arc::new(Node {
                    kind: NodeKind::Ident,
                    token: Some(Arc::new(token(TokenKind::Ident("x".into())))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![],
                    children: vec![],
                }),
                Arc::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Arc::new(token(TokenKind::SemiColon))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![],
                    children: vec![],
                }),
            ],
            ErrorReporter { errors: vec![ParseError::UnexpectedEOFDetected], max_errors: 100 }
        )
    )]
    fn test(#[case] input: Vec<Arc<Token>>, #[case] expected: (Vec<Arc<Node>>, ErrorReporter)) {
        let (nodes, errors) = Parser::new(input.iter()).parse();
        assert_eq!(errors, expected.1);
        assert_eq!(nodes, expected.0);
    }
}
