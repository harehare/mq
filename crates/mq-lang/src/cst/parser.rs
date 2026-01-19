use std::{collections::BTreeSet, fmt::Display, iter::Peekable};

use crate::{
    Position, Range, Shared, Token, TokenKind,
    ast::constants,
    cst::node::{BinaryOp, UnaryOp},
    selector::{self, Selector},
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

        Self { errors: h, max_errors }
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
                    ParseError::UnknownSelector(selector::UnknownSelector(token)) => &token.range,
                    ParseError::UnexpectedEOFDetected => return std::cmp::Ordering::Greater,
                };

                let b_range = match b {
                    ParseError::UnexpectedToken(token) => &token.range,
                    ParseError::InsufficientTokens(token) => &token.range,
                    ParseError::ExpectedClosingBracket(token) => &token.range,
                    ParseError::UnknownSelector(selector::UnknownSelector(token)) => &token.range,
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
                        ParseError::UnexpectedToken(token) => token.range,
                        ParseError::InsufficientTokens(token) => token.range,
                        ParseError::ExpectedClosingBracket(token) => token.range,
                        ParseError::UnknownSelector(selector::UnknownSelector(token)) => token.range,
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

    #[inline(always)]
    fn token_kind_to_binary_op(kind: &TokenKind) -> Option<BinaryOp> {
        match kind {
            TokenKind::And => Some(BinaryOp::And),
            TokenKind::Asterisk => Some(BinaryOp::Multiplication),
            TokenKind::Coalesce => Some(BinaryOp::Coalesce),
            TokenKind::Equal => Some(BinaryOp::Assign),
            TokenKind::EqEq => Some(BinaryOp::Equal),
            TokenKind::Gte => Some(BinaryOp::Gte),
            TokenKind::Gt => Some(BinaryOp::Gt),
            TokenKind::Lte => Some(BinaryOp::Lte),
            TokenKind::Lt => Some(BinaryOp::Lt),
            TokenKind::Minus => Some(BinaryOp::Minus),
            TokenKind::NeEq => Some(BinaryOp::NotEqual),
            TokenKind::Or => Some(BinaryOp::Or),
            TokenKind::Percent => Some(BinaryOp::Modulo),
            TokenKind::Plus => Some(BinaryOp::Plus),
            TokenKind::RangeOp => Some(BinaryOp::RangeOp),
            TokenKind::Slash => Some(BinaryOp::Division),
            _ => None,
        }
    }

    fn parse_program(&mut self, root: bool, in_loop: bool) -> (Vec<Shared<Node>>, ErrorReporter) {
        let mut nodes: Vec<Shared<Node>> = Vec::with_capacity(self.tokens.len() / 4);
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

            match &token.kind {
                TokenKind::Eof if root => {
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
                TokenKind::Pipe => {
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
                TokenKind::SemiColon | TokenKind::End => {
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
                                self.tokens.next();

                                nodes.push(Shared::new(Node {
                                    kind: NodeKind::Eof,
                                    token: Some(Shared::clone(token)),
                                    leading_trivia,
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }));
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
                                self.errors.report(ParseError::UnexpectedToken(Shared::clone(token)));
                            }
                        }
                    }

                    break;
                }
                TokenKind::Def => {}
                TokenKind::Macro => {}
                _ => {
                    self.errors
                        .report(ParseError::UnexpectedToken(Shared::new((*token).clone())));
                    break;
                }
            }
        }

        if nodes.is_empty() {
            match self.tokens.peek() {
                Some(token) => self.errors.report(ParseError::UnexpectedToken(Shared::clone(token))),
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

        while self.try_next_token(|kind| Self::token_kind_to_binary_op(kind).is_some()) {
            let leading_trivia = self.parse_leading_trivia();
            let operator_token = self.tokens.next().unwrap();
            let binary_op = Self::token_kind_to_binary_op(&operator_token.kind).unwrap();

            let node_kind = if binary_op == BinaryOp::Assign {
                NodeKind::Assign
            } else {
                NodeKind::BinaryOp(binary_op)
            };

            let mut op = Node {
                kind: node_kind,
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
        let token = self.tokens.peek().ok_or(ParseError::UnexpectedEOFDetected)?;

        match &token.kind {
            TokenKind::Def => self.parse_def(leading_trivia),
            TokenKind::Macro => self.parse_macro(leading_trivia),
            TokenKind::Do => self.parse_block(leading_trivia, in_loop),
            TokenKind::Fn => self.parse_fn(leading_trivia, in_loop),
            TokenKind::If => self.parse_if(leading_trivia, in_loop),
            TokenKind::Foreach => self.parse_foreach(leading_trivia),
            TokenKind::Include => self.parse_include(leading_trivia),
            TokenKind::Import => self.parse_import(leading_trivia),
            TokenKind::Module => self.parse_module(leading_trivia),
            TokenKind::While => self.parse_while(leading_trivia),
            TokenKind::Loop => self.parse_loop(leading_trivia),
            TokenKind::Try => self.parse_try(leading_trivia),
            TokenKind::Match => self.parse_match(leading_trivia),
            TokenKind::Ident(_) => self.parse_ident(leading_trivia),
            TokenKind::Self_ => self.parse_self(leading_trivia),
            TokenKind::Let | TokenKind::Var => self.parse_var_decl(leading_trivia, in_loop),
            TokenKind::Selector(_) => self.parse_selector(leading_trivia),
            TokenKind::StringLiteral(_) | TokenKind::NumberLiteral(_) | TokenKind::BoolLiteral(_) | TokenKind::None => {
                self.parse_node(NodeKind::Literal, leading_trivia)
            }
            TokenKind::InterpolatedString(_) => self.parse_interpolated_string(leading_trivia),
            TokenKind::LBracket => self.parse_array(leading_trivia),
            TokenKind::LBrace => self.parse_dict(leading_trivia),
            TokenKind::LParen => self.parse_group_expr(leading_trivia, root, in_loop),
            TokenKind::Nodes if root => self.parse_nodes(leading_trivia),
            TokenKind::Env(_) => self.parse_node(NodeKind::Env, leading_trivia),
            TokenKind::Not | TokenKind::Minus => self.parse_unary_op(leading_trivia, root),
            TokenKind::Break if in_loop => self.parse_break(leading_trivia, in_loop),
            TokenKind::Continue if in_loop => self.parse_node(NodeKind::Continue, leading_trivia),
            TokenKind::Colon => self.parse_symbol(leading_trivia),
            TokenKind::Quote => self.parse_quote(leading_trivia),
            TokenKind::Unquote => self.parse_unquote(leading_trivia),
            TokenKind::Eof => {
                self.tokens.next();
                Err(ParseError::UnexpectedEOFDetected)
            }
            _ => {
                let token = self.tokens.next().unwrap();
                Err(ParseError::UnexpectedToken(Shared::clone(token)))
            }
        }
    }

    fn parse_quote(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut node = Node {
            kind: NodeKind::Quote,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        self.push_colon_token_if_present(&mut node.children)?;

        let leading_trivia = self.parse_leading_trivia();
        let expr = self.parse_expr(leading_trivia, false, false)?;

        node.children.push(expr);

        Ok(Shared::new(node))
    }

    fn parse_unquote(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut node = Node {
            kind: NodeKind::Unquote,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::with_capacity(1),
        };

        let leading_trivia = self.parse_leading_trivia();
        let expr = self.parse_expr(leading_trivia, false, false)?;
        node.children.push(expr);

        Ok(Shared::new(node))
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

        let mut children: Vec<Shared<Node>> = Vec::with_capacity(3);

        children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LParen), NodeKind::Token)?);

        match self.tokens.peek() {
            Some(token) => Shared::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        let leading_trivia = self.parse_leading_trivia();

        children.push(self.parse_expr(leading_trivia, root, in_loop)?);
        children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::RParen), NodeKind::Token)?);

        node.children = children;

        Ok(Shared::new(node))
    }

    fn parse_ident(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

        // Check for qualified access (module::function, module::value, or module::module2::method)
        if let Some(next_token) = self.tokens.peek()
            && matches!(next_token.kind, TokenKind::DoubleColon)
        {
            let mut node = Node {
                kind: NodeKind::QualifiedAccess,
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia: Vec::new(),
                children: Vec::new(),
            };

            // Parse all :: separated identifiers
            while let Some(peek_token) = self.tokens.peek()
                && matches!(peek_token.kind, TokenKind::DoubleColon)
            {
                // Add double colon token
                children.push(self.next_node(|kind| matches!(kind, TokenKind::DoubleColon), NodeKind::Token)?);

                if let Some(next_token) = self.tokens.peek()
                    && !matches!(next_token.kind, TokenKind::Ident(_))
                {
                    node.children = children;
                    return Ok(Shared::new(node));
                }

                // Add identifier
                children.push(self.next_node(|kind| matches!(kind, TokenKind::Ident(_)), NodeKind::Ident)?);

                // Check if there's another :: or if we're at a function call
                if let Some(next_peek) = self.tokens.peek() {
                    if matches!(next_peek.kind, TokenKind::LParen) {
                        // This is a function call, parse args and break
                        let mut args = self.parse_args()?;
                        children.append(&mut args);
                        break;
                    }
                    // Otherwise, continue the loop to check for more ::
                } else {
                    break;
                }
            }

            node.children = children;
            return Ok(Shared::new(node));
        }

        let mut node = Node {
            kind: NodeKind::Ident,
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
            children: Vec::with_capacity(16),
        };

        // Check for attribute access: ident.attr -> attr(ident, "attr")
        if let Some(attr_node) = self.try_parse_attribute_access(&mut node) {
            return Ok(attr_node);
        }

        match self.tokens.peek() {
            Some(token) if matches!(token.kind, TokenKind::LParen) => {
                let mut args = self.parse_args()?;
                children.append(&mut args);

                if self.try_next_token(|kind| matches!(kind, TokenKind::Question)) {
                    children
                        .push(self.next_node(|token_kind| matches!(token_kind, TokenKind::Question), NodeKind::Token)?);
                }

                node.kind = NodeKind::Call;
                node.children = children;

                // Check for macro call (e.g., foo(args) do ...)
                if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Do)) {
                    node.kind = NodeKind::MacroCall;

                    let leading_trivia = self.parse_leading_trivia();
                    let block = self.parse_block(leading_trivia, false)?;
                    node.children.push(block);

                    return Ok(Shared::new(node));
                }

                // Check for bracket access after function call (e.g., foo()[0])
                if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                    self.parse_bracket_access(node)
                } else if let Some(attr_node) = self.try_parse_attribute_access(&mut node) {
                    Ok(attr_node)
                } else {
                    Ok(Shared::new(node))
                }
            }
            Some(token) if matches!(token.kind, TokenKind::LBracket) => self.parse_bracket_access(node),
            _ => Ok(Shared::new(node)),
        }
    }

    fn try_parse_attribute_access(&mut self, node: &mut Node) -> Option<Shared<Node>> {
        if let Some(peek_token) = self.tokens.peek()
            && matches!(&peek_token.kind, TokenKind::Selector(s) if s.len() > 1)
        {
            let selector_token = Shared::clone(peek_token);

            // Consume the selector token
            self.tokens.next();

            // Create a Call node for attr(node, "attr")
            let attr_node = Node {
                kind: NodeKind::Selector,
                token: Some(Shared::clone(&selector_token)),
                leading_trivia: node.leading_trivia.clone(),
                trailing_trivia: Vec::new(),
                children: Vec::new(),
            };

            // Add the original node as the first argument
            node.children.push(Shared::new(attr_node));

            Some(Shared::new(std::mem::take(node)))
        } else {
            None
        }
    }

    // Parses bracket access operations recursively to handle nested access like arr[0][1][2]
    fn parse_bracket_access(&mut self, mut node: Node) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = node.children;

        // Parse bracket access: ident[key] -> get(ident, key) or ident[start:end] -> slice(ident, start, end)
        children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LBracket), NodeKind::Token)?);

        // Parse the first expression
        let first_expr = self.parse_expr(Vec::new(), false, false)?;
        children.push(first_expr);

        // Check if this is a slice operation (contains ':')
        let is_slice = matches!(self.tokens.peek(), Some(token) if matches!(token.kind, TokenKind::Colon));

        if is_slice {
            // Add the colon token
            children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::Colon), NodeKind::Token)?);

            if !self.try_next_token(|kind| *kind == TokenKind::RBracket) {
                // Parse the second expression (end index)
                let second_expr = self.parse_expr(Vec::new(), false, false)?;
                children.push(second_expr);
            }
        }

        // Expect closing bracket
        match self.tokens.peek() {
            Some(token) if matches!(token.kind, TokenKind::RBracket) => {
                children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::RBracket), NodeKind::Token)?);
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
        let final_node = if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
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
        let mut nodes: Vec<Shared<Node>> = Vec::with_capacity(8);

        nodes.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LParen), NodeKind::Token)?);

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

        match &token.kind {
            TokenKind::Ident(_)
            | TokenKind::StringLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::NumberLiteral(_)
            | TokenKind::None
            | TokenKind::Selector(_)
            | TokenKind::InterpolatedString(_)
            | TokenKind::Self_
            | TokenKind::LBracket
            | TokenKind::Not
            | TokenKind::Minus
            | TokenKind::Fn
            | TokenKind::Foreach
            | TokenKind::While
            | TokenKind::Loop
            | TokenKind::If
            | TokenKind::LParen
            | TokenKind::Do
            | TokenKind::Colon
            | TokenKind::Unquote
            | TokenKind::Quote
            | TokenKind::LBrace => self.parse_expr(leading_trivia, false, false),
            _ => Err(ParseError::UnexpectedToken(Shared::clone(&token))),
        }
    }

    fn parse_def(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(12);

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

        self.push_colon_or_do_token_if_present(&mut children)?;

        let (mut program, _) = self.parse_program(false, false);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_macro(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(12);

        let mut node = Node {
            kind: NodeKind::Macro,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        children.push(self.next_node(|kind| matches!(kind, TokenKind::Ident(_)), NodeKind::Ident)?);

        let mut params = self.parse_params()?;
        children.append(&mut params);

        self.push_colon_token_if_present(&mut children)?;

        let leading_trivia = self.parse_leading_trivia();

        let expr = self.parse_expr(leading_trivia, false, false)?;

        children.push(expr);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_fn(&mut self, leading_trivia: Vec<Trivia>, in_loop: bool) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

        let mut node = Node {
            kind: NodeKind::Fn,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        let mut params = self.parse_params()?;
        children.append(&mut params);

        self.push_colon_or_do_token_if_present(&mut children)?;

        let (mut program, _) = self.parse_program(false, in_loop);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_block(&mut self, leading_trivia: Vec<Trivia>, in_loop: bool) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let (program, _) = self.parse_program(false, in_loop);

        Ok(Shared::new(Node {
            kind: NodeKind::Block,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: program,
        }))
    }

    fn parse_selector(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();

        if token.to_string() == "." && !self.try_next_token(|kind| matches!(kind, TokenKind::LBracket)) {
            return Ok(Shared::new(Node {
                kind: NodeKind::Self_,
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia,
                children: Vec::new(),
            }));
        }

        let mut node = Node {
            kind: NodeKind::Selector,
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        match &token.kind {
            TokenKind::Selector(s) if s == "." => {
                let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

                // []
                children.push(self.next_node(|kind| matches!(kind, TokenKind::LBracket), NodeKind::Token)?);

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                if matches!(token.kind, TokenKind::NumberLiteral(_)) {
                    children
                        .push(self.next_node(|kind| matches!(kind, TokenKind::NumberLiteral(_)), NodeKind::Literal)?);
                }

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                if token.kind == TokenKind::RBracket {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::RBracket), NodeKind::Token)?);
                } else {
                    return Err(ParseError::UnexpectedToken(Shared::clone(&token)));
                }

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                // [][]
                if token.kind == TokenKind::LBracket {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::LBracket), NodeKind::Token)?);
                } else {
                    node.children = children;
                    return Ok(Shared::new(node));
                }

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                if matches!(token.kind, TokenKind::NumberLiteral(_)) {
                    children
                        .push(self.next_node(|kind| matches!(kind, TokenKind::NumberLiteral(_)), NodeKind::Literal)?);
                }

                let token = match self.tokens.peek() {
                    Some(token) => Shared::clone(token),
                    None => return Err(ParseError::UnexpectedEOFDetected),
                };

                if token.kind == TokenKind::RBracket {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::RBracket), NodeKind::Token)?);
                } else {
                    return Err(ParseError::UnexpectedToken(Shared::clone(&token)));
                }

                node.children = children;
                Ok(Shared::new(node))
            }
            _ => {
                Selector::try_from(&**token).map_err(ParseError::UnknownSelector)?;

                if let Some(attr_token) = self.tokens.peek()
                    && attr_token.is_selector()
                    && Selector::try_from(&***attr_token)
                        .map_err(ParseError::UnknownSelector)?
                        .is_attribute_selector()
                {
                    node.children =
                        vec![self.next_node(|kind| matches!(kind, TokenKind::Selector(_)), NodeKind::Selector)?];
                }

                Ok(Shared::new(node))
            }
        }
    }

    fn parse_include(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let child = self.next_node(|kind| matches!(kind, TokenKind::StringLiteral(_)), NodeKind::Literal)?;

        Ok(Shared::new(Node {
            kind: NodeKind::Include,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: vec![child],
        }))
    }

    fn parse_import(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let child = self.next_node(|kind| matches!(kind, TokenKind::StringLiteral(_)), NodeKind::Literal)?;

        Ok(Shared::new(Node {
            kind: NodeKind::Import,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: vec![child],
        }))
    }

    fn parse_module(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(16);

        let mut node = Node {
            kind: NodeKind::Module,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        // Parse module name (identifier)
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Ident(_)), NodeKind::Ident)?);

        self.push_colon_or_do_token_if_present(&mut children)?;

        // Parse program block (contains let, def, or module statements)
        let (program_nodes, errors) = self.parse_program(false, false);

        // Merge errors from parse_program into self.errors
        for error in errors.to_vec() {
            self.errors.report(error);
        }

        children.extend(program_nodes);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_if(&mut self, leading_trivia: Vec<Trivia>, in_loop: bool) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

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

        self.push_colon_token_if_present(&mut children)?;

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

    fn parse_elif(&mut self, leading_trivia: Vec<Trivia>, in_loop: bool) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

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

        self.push_colon_token_if_present(&mut children)?;

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, in_loop)?);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_else(&mut self, leading_trivia: Vec<Trivia>, in_loop: bool) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

        let mut node = Node {
            kind: NodeKind::Else,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        self.push_colon_token_if_present(&mut children)?;

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, in_loop)?);

        node.children = children;
        Ok(Shared::new(node))
    }

    #[inline(always)]
    fn parse_node(&mut self, node_kind: NodeKind, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
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

    fn parse_break(&mut self, leading_trivia: Vec<Trivia>, in_loop: bool) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut node = Node {
            kind: NodeKind::Break,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        // Optionally parse colon and expression (break: expr)
        if self.try_next_token(|kind| matches!(kind, TokenKind::Colon)) {
            self.push_colon_token_if_present(&mut node.children)?;
            let leading_trivia = self.parse_leading_trivia();
            node.children.push(self.parse_expr(leading_trivia, false, in_loop)?);
        }

        Ok(Shared::new(node))
    }

    fn parse_array(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(12);

        children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LBracket), NodeKind::Token)?);

        loop {
            if self.try_next_token(|kind| matches!(kind, TokenKind::RBracket)) {
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
                break;
            }

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

    #[inline(always)]
    fn parse_dict_key(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = match self.tokens.peek() {
            Some(token) => Shared::clone(token),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        match &token.kind {
            TokenKind::Ident(_) | TokenKind::Colon | TokenKind::StringLiteral(_) => {
                self.parse_expr(leading_trivia, false, false)
            }
            _ => Err(ParseError::UnexpectedToken(Shared::clone(&token))),
        }
    }

    fn parse_dict(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(12);

        children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LBrace), NodeKind::Token)?);

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
            let leading_trivia = self.parse_leading_trivia();
            let mut dict_entry = Node {
                kind: NodeKind::DictEntry,
                token: None,
                leading_trivia,
                trailing_trivia: Vec::new(),
                children: Vec::new(),
            };
            let mut entry: Vec<Shared<Node>> = Vec::with_capacity(3);
            let key_node = {
                let leading_trivia = self.parse_leading_trivia();
                self.parse_dict_key(leading_trivia)
            }?;

            let colon_node = self.next_node(|token_kind| matches!(token_kind, TokenKind::Colon), NodeKind::Token)?;

            let value_node = {
                let leading_trivia = self.parse_leading_trivia();
                self.parse_expr(leading_trivia, false, false)
            }?;

            entry.push(key_node);
            entry.push(colon_node);
            entry.push(value_node);
            dict_entry.children = entry;
            children.push(Shared::new(dict_entry));

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

    fn parse_interpolated_string(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();

        if let TokenKind::InterpolatedString(_) = &token.kind {
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

    fn parse_var_decl(&mut self, leading_trivia: Vec<Trivia>, in_loop: bool) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

        // Determine NodeKind based on token type
        let node_kind = match &token.as_ref().unwrap().kind {
            TokenKind::Var => NodeKind::Var,
            _ => NodeKind::Let,
        };

        let mut node = Node {
            kind: node_kind,
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

        let mut node = Node {
            kind: NodeKind::Self_,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::with_capacity(16),
        };

        // Check for attribute access: ..attr -> attr(., "attr")
        if let Some(attr_node) = self.try_parse_attribute_access(&mut node) {
            return Ok(attr_node);
        }

        match self.tokens.peek() {
            Some(token) if matches!(token.kind, TokenKind::LBracket) => self.parse_bracket_access(node),
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

    fn parse_symbol(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(2);

        // Parse the colon token
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        // Parse the identifier or string literal that follows
        let symbol_leading_trivia = self.parse_leading_trivia();
        let symbol_token = match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::Ident(_) | TokenKind::StringLiteral(_) => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();
                    Shared::new(Node {
                        kind: NodeKind::Literal,
                        token: Some(Shared::clone(token)),
                        leading_trivia: symbol_leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    })
                }
                _ => return Err(ParseError::UnexpectedToken(Shared::clone(token))),
            },
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        children.push(symbol_token);

        Ok(Shared::new(Node {
            kind: NodeKind::Literal,
            token: None,
            leading_trivia,
            trailing_trivia: Vec::new(),
            children,
        }))
    }

    fn parse_foreach(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

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

        children.push(self.parse_expr(leading_trivia, false, false)?);
        children.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);

        self.push_colon_or_do_token_if_present(&mut children)?;

        let (mut program, _) = self.parse_program(false, true);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_while(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

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

        self.push_colon_or_do_token_if_present(&mut children)?;

        let (mut program, _) = self.parse_program(false, true);

        children.append(&mut program);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_loop(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(4);

        let mut node = Node {
            kind: NodeKind::Loop,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        self.push_colon_or_do_token_if_present(&mut children)?;

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

        self.push_colon_or_do_token_if_present(&mut children)?;

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

        self.push_colon_or_do_token_if_present(&mut children)?;

        let leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(leading_trivia, false, false)?);

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_match(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = self.tokens.next();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        let mut node = Node {
            kind: NodeKind::Match,
            token: Some(Shared::clone(token.unwrap())),
            leading_trivia,
            trailing_trivia,
            children: Vec::new(),
        };

        // Parse (value)
        let mut args = self.parse_args()?;
        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
            return Err(ParseError::UnexpectedToken(Shared::clone(token.unwrap())));
        }
        children.append(&mut args);

        self.push_colon_or_do_token_if_present(&mut children)?;

        // Parse match arms
        loop {
            let leading_trivia = self.parse_leading_trivia();

            match self.tokens.peek() {
                Some(token) if matches!(token.kind, TokenKind::End) => {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::End), NodeKind::End)?);
                    break;
                }
                Some(token) if matches!(token.kind, TokenKind::Eof) => {
                    break;
                }
                Some(_) => {}
                None => {
                    break;
                }
            }

            // Parse match arm (| pattern: body)
            children.push(self.parse_match_arm(leading_trivia)?);
        }

        node.children = children;
        Ok(Shared::new(node))
    }

    fn parse_match_arm(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        // Parse pipe |
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Pipe), NodeKind::Token)?);

        // Parse pattern
        let pattern_leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_pattern(pattern_leading_trivia)?);

        // Check for guard (if condition)
        if let Some(token) = self.tokens.peek()
            && matches!(token.kind, TokenKind::If)
        {
            children.push(self.next_node(|kind| matches!(kind, TokenKind::If), NodeKind::Token)?);

            // Parse guard expression (as args)
            let mut guard_args = self.parse_args()?;
            children.append(&mut guard_args);
        }

        // Parse colon
        children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

        // Parse body expression
        let body_leading_trivia = self.parse_leading_trivia();
        children.push(self.parse_expr(body_leading_trivia, false, false)?);

        Ok(Shared::new(Node {
            kind: NodeKind::MatchArm,
            token: None,
            leading_trivia,
            trailing_trivia: Vec::new(),
            children,
        }))
    }

    fn parse_pattern(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let token = match self.tokens.peek() {
            Some(t) => Shared::clone(t),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        match &token.kind {
            // Wildcard pattern: _
            TokenKind::Ident(name) if name == constants::PATTERN_MATCH_WILDCARD => {
                self.tokens.next();
                let trailing_trivia = self.parse_trailing_trivia();
                Ok(Shared::new(Node {
                    kind: NodeKind::Pattern,
                    token: Some(Shared::clone(&token)),
                    leading_trivia,
                    trailing_trivia,
                    children: Vec::new(),
                }))
            }
            // Type pattern: :type_name
            TokenKind::Colon => {
                let children = vec![
                    self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?,
                    self.next_node(|kind| matches!(kind, TokenKind::Ident(_)), NodeKind::Ident)?,
                ];

                Ok(Shared::new(Node {
                    kind: NodeKind::Pattern,
                    token: None,
                    leading_trivia,
                    trailing_trivia: Vec::new(),
                    children,
                }))
            }
            // Literal patterns (string, number, bool, none)
            TokenKind::StringLiteral(_) | TokenKind::NumberLiteral(_) | TokenKind::BoolLiteral(_) | TokenKind::None => {
                self.tokens.next();
                let trailing_trivia = self.parse_trailing_trivia();
                Ok(Shared::new(Node {
                    kind: NodeKind::Pattern,
                    token: Some(Shared::clone(&token)),
                    leading_trivia,
                    trailing_trivia,
                    children: Vec::new(),
                }))
            }
            // Array pattern: [pattern, pattern, ...]
            TokenKind::LBracket => self.parse_array_pattern(leading_trivia),
            // Dict pattern: {key: pattern, key}
            TokenKind::LBrace => self.parse_dict_pattern(leading_trivia),
            // Identifier pattern (binding)
            TokenKind::Ident(_) => {
                self.tokens.next();
                let trailing_trivia = self.parse_trailing_trivia();
                Ok(Shared::new(Node {
                    kind: NodeKind::Pattern,
                    token: Some(Shared::clone(&token)),
                    leading_trivia,
                    trailing_trivia,
                    children: Vec::new(),
                }))
            }
            _ => {
                self.tokens.next();
                Err(ParseError::UnexpectedToken(token))
            }
        }
    }

    fn parse_array_pattern(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        // Parse [
        children.push(self.next_node(|kind| matches!(kind, TokenKind::LBracket), NodeKind::Token)?);

        // Parse patterns
        loop {
            let leading_trivia = self.parse_leading_trivia();

            // Check for ]
            if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::RBracket) {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::RBracket), NodeKind::Token)?);
                    break;
                }

                // Check for rest pattern: ..rest
                if matches!(token.kind, TokenKind::RangeOp) {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::RangeOp), NodeKind::Token)?);

                    // Parse rest identifier
                    let rest_leading_trivia = self.parse_leading_trivia();
                    children.push(self.parse_pattern(rest_leading_trivia)?);

                    // Expect ]
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::RBracket), NodeKind::Token)?);
                    break;
                }
            }

            // Parse pattern
            children.push(self.parse_pattern(leading_trivia)?);

            // Check for comma or ]
            if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::Comma) {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?);
                } else if matches!(token.kind, TokenKind::RBracket) {
                    // Will be consumed in next iteration
                    continue;
                }
            }
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Pattern,
            token: None,
            leading_trivia,
            trailing_trivia: Vec::new(),
            children,
        }))
    }

    fn parse_dict_pattern(&mut self, leading_trivia: Vec<Trivia>) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        // Parse {
        children.push(self.next_node(|kind| matches!(kind, TokenKind::LBrace), NodeKind::Token)?);

        // Parse fields
        loop {
            let _leading_trivia = self.parse_leading_trivia();

            // Check for }
            if let Some(token) = self.tokens.peek()
                && matches!(token.kind, TokenKind::RBrace)
            {
                children.push(self.next_node(|kind| matches!(kind, TokenKind::RBrace), NodeKind::Token)?);
                break;
            }

            // Parse key (identifier)
            children.push(self.next_node(|kind| matches!(kind, TokenKind::Ident(_)), NodeKind::Ident)?);

            // Check for colon (key: pattern) or just key
            if let Some(token) = self.tokens.peek()
                && matches!(token.kind, TokenKind::Colon)
            {
                children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

                let pattern_leading_trivia = self.parse_leading_trivia();
                children.push(self.parse_pattern(pattern_leading_trivia)?);
            }

            // Check for comma or }
            if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::Comma) {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?);
                } else if matches!(token.kind, TokenKind::RBrace) {
                    // Will be consumed in next iteration
                    continue;
                }
            }
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Pattern,
            token: None,
            leading_trivia,
            trailing_trivia: Vec::new(),
            children,
        }))
    }

    fn parse_unary_op(&mut self, leading_trivia: Vec<Trivia>, root: bool) -> Result<Shared<Node>, ParseError> {
        let operator_token = self.tokens.next().unwrap();
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(2);

        let mut node = Node {
            kind: match &operator_token.kind {
                TokenKind::Not => NodeKind::UnaryOp(UnaryOp::Not),
                TokenKind::Minus => NodeKind::UnaryOp(UnaryOp::Negate),
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

        nodes.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LParen), NodeKind::Token)?);

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

        // Parse parameter name (identifier)
        let param_ident = match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::Ident(_) => {
                    let token = self.tokens.next().unwrap();
                    let trailing_trivia = self.parse_trailing_trivia();
                    Shared::new(Node {
                        kind: NodeKind::Ident,
                        token: Some(Shared::clone(token)),
                        leading_trivia,
                        trailing_trivia,
                        children: Vec::new(),
                    })
                }
                _ => return Err(ParseError::UnexpectedToken(Shared::clone(token))),
            },
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        if self.try_next_token(|kind| matches!(kind, TokenKind::Equal)) {
            // Save param_ident info before moving it
            let param_token = param_ident.token.clone();
            let param_leading_trivia = param_ident.leading_trivia.clone();

            // Create a node with param_name, '=', and default_value as children
            let mut children = vec![param_ident];

            // Add '=' token
            let eq_leading_trivia = self.parse_leading_trivia();
            let equal_token = self.tokens.next().unwrap();
            let eq_trailing_trivia = self.parse_trailing_trivia();
            children.push(Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(equal_token)),
                leading_trivia: eq_leading_trivia,
                trailing_trivia: eq_trailing_trivia,
                children: Vec::new(),
            }));

            // Parse default value
            let default_leading_trivia = self.parse_leading_trivia();
            let default_value = self.parse_expr(default_leading_trivia, false, false)?;
            children.push(default_value);

            let trailing_trivia = self.parse_trailing_trivia();

            // Return a node containing all three parts (ident, '=', default_value)
            Ok(Shared::new(Node {
                kind: NodeKind::Ident,
                token: param_token,
                leading_trivia: param_leading_trivia,
                trailing_trivia,
                children,
            }))
        } else {
            Ok(param_ident)
        }
    }

    #[inline(always)]
    fn parse_leading_trivia(&mut self) -> Vec<Trivia> {
        let mut trivia = Vec::with_capacity(4);

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

    fn skip_leading_trivia(tokens: &mut Peekable<core::slice::Iter<'a, Shared<Token>>>) {
        while let Some(token) = tokens.peek() {
            if matches!(
                token.kind,
                TokenKind::Whitespace(_) | TokenKind::Tab(_) | TokenKind::Comment(_) | TokenKind::NewLine
            ) {
                tokens.next();
            } else {
                break;
            }
        }
    }

    #[inline(always)]
    fn try_next_token(&mut self, match_token_kind: fn(&TokenKind) -> bool) -> bool {
        let tokens = &mut self.tokens.clone();
        Self::skip_leading_trivia(tokens);

        let token = tokens.peek().ok_or(ParseError::UnexpectedEOFDetected);

        if token.is_err() {
            return false;
        }

        match_token_kind(&token.unwrap().kind)
    }

    #[inline(always)]
    fn parse_trailing_trivia(&mut self) -> Vec<Trivia> {
        let mut trivia = Vec::with_capacity(2);

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
                | TokenKind::Loop
                | TokenKind::Foreach
                | TokenKind::Let
                | TokenKind::Var
                | TokenKind::Def
                | TokenKind::Ident(_)
                | TokenKind::Pipe
                | TokenKind::SemiColon
                | TokenKind::Do
                | TokenKind::Try
                | TokenKind::LParen
                | TokenKind::LBrace
                | TokenKind::End
                | TokenKind::Eof => return,
                _ => {
                    self.tokens.next();
                }
            }
        }
    }

    #[inline(always)]
    fn next_token(&mut self, match_token_kind: fn(&TokenKind) -> bool) -> Result<Shared<Token>, ParseError> {
        let token = self.tokens.peek().cloned().ok_or(ParseError::UnexpectedEOFDetected)?;

        if match_token_kind(&token.kind) {
            self.tokens.next();
            Ok(Shared::clone(token))
        } else {
            Err(ParseError::UnexpectedToken(Shared::clone(token)))
        }
    }

    #[inline(always)]
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

    #[inline(always)]
    fn push_colon_token_if_present(&mut self, children: &mut Vec<Shared<Node>>) -> Result<(), ParseError> {
        if self.try_next_token(|kind| matches!(kind, TokenKind::Colon)) {
            children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);
        }

        Ok(())
    }

    #[inline(always)]
    fn push_colon_or_do_token_if_present(&mut self, children: &mut Vec<Shared<Node>>) -> Result<(), ParseError> {
        // Check for 'do' keyword
        if self.try_next_token(|kind| matches!(kind, TokenKind::Do)) {
            children.push(self.next_node(|kind| matches!(kind, TokenKind::Do), NodeKind::Do)?);
        } else {
            self.push_colon_token_if_present(children)?;
        }

        Ok(())
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
            Shared::new(token(TokenKind::End)),
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
                            kind: NodeKind::End,
                            token: Some(Shared::new(token(TokenKind::End))),
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
    #[case::def_without_colon1(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::StringLiteral("test".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def,
                    token: Some(Shared::new(token(TokenKind::Def))),
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
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("test".into())))),
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
    #[case::def_without_colon2(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("baz".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::NumberLiteral(42.into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def,
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("baz".into())))),
                            leading_trivia: Vec::new(),
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
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(42.into())))),
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
    #[case::def_without_colon_with_args(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("func".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def,
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("func".into())))),
                            leading_trivia: Vec::new(),
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
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
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
    #[case::foreach_do_end(
        vec![
            Shared::new(token(TokenKind::Foreach)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("item".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("collection".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::End)),
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
                            kind: NodeKind::Do,
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: vec![Trivia::NewLine],
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
    #[case::foreach_value_is_function_call(
        vec![
            Shared::new(token(TokenKind::Foreach)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("item".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("get_items".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("arg".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
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
                            kind: NodeKind::Call,
                            token: Some(Shared::new(token(TokenKind::Ident("get_items".into())))),
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
                                    token: Some(Shared::new(token(TokenKind::Ident("arg".into())))),
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
    #[case::while_do_end(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::Whitespace(1))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("condition".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::End)),
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
                            kind: NodeKind::Do,
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: vec![Trivia::NewLine],
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
    #[case::loop_(
        vec![
            Shared::new(token(TokenKind::Loop)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Comment("loop comment".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Ident("body".into()))),
            Shared::new(token(TokenKind::Whitespace(2))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Loop,
                    token: Some(Shared::new(token(TokenKind::Loop))),
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
                            token: Some(Shared::new(token(TokenKind::Ident("body".into())))),
                            leading_trivia: vec![Trivia::NewLine, Trivia::Comment(Shared::new(token(TokenKind::Comment("loop comment".into())))), Trivia::NewLine],
                            trailing_trivia: vec![Trivia::Whitespace(Shared::new(token(TokenKind::Whitespace(2))))],
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
            Shared::new(token(TokenKind::Selector(".h1".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Shared::new(token(TokenKind::Selector(".h1".into())))),
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
            Shared::new(token(TokenKind::Selector(".list".into()))),
            Shared::new(token(TokenKind::Selector(".checked".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Shared::new(token(TokenKind::Selector(".list".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Selector,
                            token: Some(Shared::new(token(TokenKind::Selector(".checked".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        })
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::ident_attribute_access(
        vec![
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::Selector(".lang".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Ident,
                    token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Selector,
                            token: Some(Shared::new(token(TokenKind::Selector(".lang".into())))),
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
    #[case::self_attribute_access(
        vec![
            Shared::new(token(TokenKind::Self_)),
            Shared::new(token(TokenKind::Selector(".name".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_,
                    token: Some(Shared::new(token(TokenKind::Self_))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Selector,
                            token: Some(Shared::new(token(TokenKind::Selector(".name".into())))),
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
            ErrorReporter::with_error(vec![ParseError::UnexpectedToken(Shared::new(token(TokenKind::Ident("y".into()))))], 100)
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
            Shared::new(token(TokenKind::InterpolatedString(vec![StringSegment::Expr("val".into(), Range::default()), StringSegment::Text("hello".into(), Range::default())]))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::InterpolatedString,
                    token: Some(Shared::new(token(TokenKind::InterpolatedString(vec![StringSegment::Expr("val".into(), Range::default()), StringSegment::Text("hello".into(), Range::default())])))),
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
                            kind: NodeKind::DictEntry,
                            token: None,
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
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
                            ],
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
                            kind: NodeKind::DictEntry,
                            token: None,
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
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
                            kind: NodeKind::DictEntry,
                            token: None,
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
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
                            ],
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
    #[case::symbol_with_ident(
        vec![
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("foo".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Literal,
                    token: None,
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
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
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
    #[case::symbol_with_string(
        vec![
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("bar".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Literal,
                    token: None,
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
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("bar".into())))),
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
    #[case::break_in_loop_with_value(
        vec![
            Shared::new(token(TokenKind::While)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::BoolLiteral(true))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Break)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::BoolLiteral(false))),
            Shared::new(token(TokenKind::SemiColon)),
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
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Colon))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Literal,
                                    token: Some(Shared::new(token(TokenKind::BoolLiteral(false)))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::SemiColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        })
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
    #[case::match_simple(
        vec![
            Shared::new(token(TokenKind::Match)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("one".into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("_".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("other".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Match,
                    token: Some(Shared::new(token(TokenKind::Match))),
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
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::MatchArm,
                            token: None,
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Pattern,
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
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("one".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::MatchArm,
                            token: None,
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Pattern,
                                    token: Some(Shared::new(token(TokenKind::Ident("_".into())))),
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
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("other".into())))),
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
    #[case::match_do_end(
        vec![
            Shared::new(token(TokenKind::Match)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("one".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("_".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("other".into()))),
            Shared::new(token(TokenKind::NewLine)),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Match,
                    token: Some(Shared::new(token(TokenKind::Match))),
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
                        Shared::new(Node {
                            kind: NodeKind::Do,
                            token: Some(Shared::new(token(TokenKind::Do))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::MatchArm,
                            token: None,
                            leading_trivia: vec![Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Pattern,
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
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("one".into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::MatchArm,
                            token: None,
                            leading_trivia: vec![Trivia::NewLine],
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(Shared::new(token(TokenKind::Pipe))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                                Shared::new(Node {
                                    kind: NodeKind::Pattern,
                                    token: Some(Shared::new(token(TokenKind::Ident("_".into())))),
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
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("other".into())))),
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
    #[case::qualified_access_simple(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("value".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess,
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("value".into())))),
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
    #[case::qualified_access_multi_level(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("mod2".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("value".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess,
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("mod2".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("value".into())))),
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
    #[case::qualified_access_with_call(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("func".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::StringLiteral("arg".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess,
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("func".into())))),
                            leading_trivia: Vec::new(),
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
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::StringLiteral("arg".into())))),
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
    #[case::qualified_access_multi_level_with_call(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("mod2".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Ident("func".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess,
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("mod2".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("func".into())))),
                            leading_trivia: Vec::new(),
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
    #[case::qualified_access::empty(
        vec![
            Shared::new(token(TokenKind::Ident("mod1".into()))),
            Shared::new(token(TokenKind::DoubleColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::QualifiedAccess,
                    token: Some(Shared::new(token(TokenKind::Ident("mod1".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::DoubleColon))),
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
    #[case::module_simple(
        vec![
            Shared::new(token(TokenKind::Module)),
            Shared::new(token(TokenKind::Ident("modname1".into()))),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::StringLiteral("bar".into()))),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::End)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Module,
                    token: Some(Shared::new(token(TokenKind::Module))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("modname1".into())))),
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
                            kind: NodeKind::Let,
                            token: Some(Shared::new(token(TokenKind::Let))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
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
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                                    leading_trivia: Vec::new(),
                                    trailing_trivia: Vec::new(),
                                    children: Vec::new(),
                                }),
                            ],
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Def,
                            token: Some(Shared::new(token(TokenKind::Def))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("foo".into())))),
                                    leading_trivia: Vec::new(),
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
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("bar".into())))),
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
    #[case::slice_access_with_start_only(
        vec![
            Shared::new(token(TokenKind::Ident("arr".into()))),
            Shared::new(token(TokenKind::LBracket)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Colon)),
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
    #[case::selector_existing_and_non_existing(
        vec![
            Shared::new(token(TokenKind::Selector(".h".into()))),
            Shared::new(token(TokenKind::Selector(".value".into()))),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Selector,
                    token: Some(Shared::new(token(TokenKind::Selector(".h".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Selector,
                            token: Some(Shared::new(token(TokenKind::Selector(".value".into())))),
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
    #[case::selector_non_existing(
        vec![
            Shared::new(token(TokenKind::Selector(".notfound".into()))),
        ],
        (
            vec![],
            ErrorReporter::with_error(vec![ParseError::UnexpectedEOFDetected, ParseError::UnknownSelector(selector::UnknownSelector(token(TokenKind::Selector(".notfound".into()))))], 100)
        )
    )]
    #[case::call_with_selector_attribute(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Selector(".level".into()))),
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
                            kind: NodeKind::Selector,
                            token: Some(Shared::new(token(TokenKind::Selector(".level".into())))),
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
    #[rstest]
    #[case::selector_dot_followed_by_pipe(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::Pipe)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_,
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
                Shared::new(Node {
                    kind: NodeKind::Token,
                    token: Some(Shared::new(token(TokenKind::Pipe))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: Vec::new(),
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::selector_dot_followed_by_semicolon(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::SemiColon)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_,
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
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
            ErrorReporter::default()
        )
    )]
    #[case::selector_dot_followed_by_end(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::End)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Self_,
                    token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
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
            ErrorReporter::default()
        )
    )]
    #[case::selector_dot_followed_by_binary_op(
        vec![
            Shared::new(token(TokenKind::Selector(".".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("x".into()))),
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
                            kind: NodeKind::Self_,
                            token: Some(Shared::new(token(TokenKind::Selector(".".into())))),
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
                    ],
                }),
            ],
            ErrorReporter::default()
        )
    )]
    #[case::let_followed_by_let_without_pipe(
        vec![
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Let)),
            Shared::new(token(TokenKind::Ident("y".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Let,
                    token: Some(Shared::new(token(TokenKind::Let))),
                    leading_trivia: vec![],
                    trailing_trivia: vec![],
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
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(1.into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                    ],
                }),
            ],
            ErrorReporter::with_error(
                vec![
                    ParseError::UnexpectedToken(Shared::new(token(TokenKind::Let))),
                ],
                100
            )
        )
    )]
    #[case::call_with_do_block_argument(
        vec![
            Shared::new(token(TokenKind::Ident("foo".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Do)),
            Shared::new(token(TokenKind::Ident("bar".into()))),
            Shared::new(token(TokenKind::End)),
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
                            kind: NodeKind::Block,
                            token: Some(Shared::new(token(TokenKind::Do))),
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
    #[case::assign(
        vec![
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(10.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Assign,
                    token: Some(Shared::new(token(TokenKind::Equal))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(10.into())))),
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
    #[case::var_then_assign(
        vec![
            Shared::new(token(TokenKind::Var)),
            Shared::new(token(TokenKind::Ident("count".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(0.into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("count".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(10.into()))),
            Shared::new(token(TokenKind::Pipe)),
            Shared::new(token(TokenKind::Ident("count".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Var,
                    token: Some(Shared::new(token(TokenKind::Var))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("count".into())))),
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
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(0.into())))),
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
                    kind: NodeKind::Assign,
                    token: Some(Shared::new(token(TokenKind::Equal))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("count".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: Vec::new(),
                        }),
                        Shared::new(Node {
                            kind: NodeKind::Literal,
                            token: Some(Shared::new(token(TokenKind::NumberLiteral(10.into())))),
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
                    token: Some(Shared::new(token(TokenKind::Ident("count".into())))),
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
            ErrorReporter::default()
        )
    )]
    #[case::macro_(
        vec![
            Shared::new(token(TokenKind::Macro)),
            Shared::new(token(TokenKind::Ident("double".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Macro,
                    token: Some(Shared::new(token(TokenKind::Macro))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("double".into())))),
                            leading_trivia: Vec::new(),
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
                        Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::new(token(TokenKind::Colon))),
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
                                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
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
                            ],
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
    #[case::quote(
        vec![
            Shared::new(token(TokenKind::Quote)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Quote,
                    token: Some(Shared::new(token(TokenKind::Quote))),
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
                                    token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
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
                            ],
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
    #[case::unquote(
        vec![
            Shared::new(token(TokenKind::Unquote)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Unquote,
                    token: Some(Shared::new(token(TokenKind::Unquote))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
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
    #[case::quote_with_unquote(
        vec![
            Shared::new(token(TokenKind::Quote)),
            Shared::new(token(TokenKind::Unquote)),
            Shared::new(token(TokenKind::Ident("x".into()))),
            Shared::new(token(TokenKind::Plus)),
            Shared::new(token(TokenKind::NumberLiteral(1.into()))),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Quote,
                    token: Some(Shared::new(token(TokenKind::Quote))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Unquote,
                            token: Some(Shared::new(token(TokenKind::Unquote))),
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
                                            token: Some(Shared::new(token(TokenKind::Ident("x".into())))),
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
                                    ],
                                }),
                            ],
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
    #[case::def_with_default_param(
        vec![
            Shared::new(token(TokenKind::Def)),
            Shared::new(token(TokenKind::Ident("greet".into()))),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("name".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("greeting".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::StringLiteral("Hello".into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("greeting".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
        ],
        (
            vec![
                Shared::new(Node {
                    kind: NodeKind::Def,
                    token: Some(Shared::new(token(TokenKind::Def))),
                    leading_trivia: Vec::new(),
                    trailing_trivia: Vec::new(),
                    children: vec![
                        Shared::new(Node {
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("greet".into())))),
                            leading_trivia: Vec::new(),
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
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("name".into())))),
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
                            token: Some(Shared::new(token(TokenKind::Ident("greeting".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("greeting".into())))),
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
                                    token: Some(Shared::new(token(TokenKind::StringLiteral("Hello".into())))),
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
                            token: Some(Shared::new(token(TokenKind::Ident("greeting".into())))),
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
    #[case::fn_with_multiple_default_params(
        vec![
            Shared::new(token(TokenKind::Fn)),
            Shared::new(token(TokenKind::LParen)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("b".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(2.into()))),
            Shared::new(token(TokenKind::Comma)),
            Shared::new(token(TokenKind::Ident("c".into()))),
            Shared::new(token(TokenKind::Equal)),
            Shared::new(token(TokenKind::NumberLiteral(3.into()))),
            Shared::new(token(TokenKind::RParen)),
            Shared::new(token(TokenKind::Colon)),
            Shared::new(token(TokenKind::Ident("a".into()))),
            Shared::new(token(TokenKind::SemiColon)),
            Shared::new(token(TokenKind::Eof)),
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
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
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
                            token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("b".into())))),
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
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(2.into())))),
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
                            kind: NodeKind::Ident,
                            token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
                            leading_trivia: Vec::new(),
                            trailing_trivia: Vec::new(),
                            children: vec![
                                Shared::new(Node {
                                    kind: NodeKind::Ident,
                                    token: Some(Shared::new(token(TokenKind::Ident("c".into())))),
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
                                    token: Some(Shared::new(token(TokenKind::NumberLiteral(3.into())))),
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
                            token: Some(Shared::new(token(TokenKind::Ident("a".into())))),
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
    fn test_parse(#[case] input: Vec<Shared<Token>>, #[case] expected: (Vec<Shared<Node>>, ErrorReporter)) {
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
        reporter.report(ParseError::UnexpectedToken(Shared::clone(&unexpected_token)));

        let insufficient_token = Shared::new(Token {
            range: Range {
                start: Position { line: 1, column: 4 },
                end: Position { line: 1, column: 7 },
            },
            kind: TokenKind::Ident("bar".into()),
            module_id: 1.into(),
        });
        reporter.report(ParseError::InsufficientTokens(Shared::clone(&insufficient_token)));

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
