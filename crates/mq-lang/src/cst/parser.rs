use std::{collections::BTreeSet, fmt::Display};

use crate::{
    Position, Range, Shared, Token, TokenKind,
    ast::constants,
    cst::node::{ArgList, BinaryOp, Elifs, UnaryOp},
    selector::Selector,
};
use smol_str::SmolStr;

use super::{
    error::ParseError,
    node::{Node, NodeKind, Trivia, TriviaList},
};
use itertools::Itertools;
use thiserror::Error;

/// Collects parse errors, capped at a maximum count.
#[derive(Debug, Clone, PartialEq, Error)]
pub struct ErrorReporter {
    errors: BTreeSet<ParseError>,
    max_errors: usize,
}

impl Display for ErrorReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.errors.iter().join(", "))
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
    /// Creates a reporter that keeps at most `max_errors` errors.
    pub fn new(max_errors: usize) -> Self {
        Self {
            errors: BTreeSet::new(),
            max_errors,
        }
    }

    /// Creates a reporter pre-filled with `errors`.
    pub fn with_error(errors: Vec<ParseError>, max_errors: usize) -> Self {
        let mut h = BTreeSet::new();
        h.extend(errors);

        Self { errors: h, max_errors }
    }

    /// Records `error` unless the limit is reached.
    pub fn report(&mut self, error: ParseError) {
        if self.errors.len() < self.max_errors {
            self.errors.insert(error);
        }
    }

    /// Returns the collected errors as diagnostics.
    pub fn to_vec(&self) -> Vec<ParseError> {
        self.errors
            .iter()
            .sorted_by(|a, b| match (a.range(), b.range()) {
                (Some(a), Some(b)) => a
                    .start
                    .line
                    .cmp(&b.start.line)
                    .then_with(|| a.start.column.cmp(&b.start.column)),
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .cloned()
            .collect()
    }

    /// Whether any error was reported.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    fn has_error_at(&self, range: &Range) -> bool {
        self.errors.iter().any(|error| error.range().as_ref() == Some(range))
    }

    /// Like [`error_ranges`](Self::error_ranges), but keeps each error's hint text
    /// (e.g. "did you mean" suggestions) instead of discarding it, so consumers
    /// that render diagnostics to users (e.g. `mq-lsp`) can show the same guidance
    /// as the CLI's AST-level errors.
    pub fn diagnostics(&self, text: &str) -> Vec<crate::Diagnostic> {
        self.to_vec()
            .iter()
            .map(|e| crate::Diagnostic {
                message: e.to_string(),
                range: Some(Self::error_range(e, text)),
                hints: e.hint().into_iter().collect(),
            })
            .collect::<Vec<_>>()
    }

    /// Returns each error message with its source range in `text`.
    pub fn error_ranges(&self, text: &str) -> Vec<(String, Range)> {
        self.to_vec()
            .iter()
            .map(|e| (e.to_string(), Self::error_range(e, text)))
            .collect::<Vec<_>>()
    }

    /// Range of `error`, falling back to the end of `text` for end-of-input errors.
    fn error_range(error: &ParseError, text: &str) -> Range {
        error.range().unwrap_or_else(|| {
            let end = Position {
                line: text.lines().count() as u32,
                column: text.lines().last().map(|line| line.len()).unwrap_or(0),
            };
            Range { start: end, end }
        })
    }
}

/// How a program ends.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProgramKind {
    /// Top level, ends at EOF.
    Root,
    /// Nested body, ends at `;` or `end`, or before a `,` or closing bracket owned by the enclosing construct.
    Block,
}

/// Error-recovering parser that builds a lossless CST from tokens.
pub struct Parser<'a> {
    tokens: &'a [Shared<Token>],
    pos: usize,
    errors: ErrorReporter,
    /// Set while parsing the selector that follows an attribute selector, which ends the chain.
    in_selector_tail: bool,
}

impl<'a> Parser<'a> {
    /// Creates a parser over `tokens`.
    pub fn new(tokens: &'a [Shared<Token>]) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: ErrorReporter::new(100),
            in_selector_tail: false,
        }
    }

    /// Returns the current parse position (token index).
    #[inline(always)]
    pub fn current_pos(&self) -> usize {
        self.pos
    }

    /// Peeks at the current token without consuming it.
    #[inline(always)]
    fn peek(&self) -> Option<&'a Shared<Token>> {
        self.tokens.get(self.pos)
    }

    /// Consumes the current token and returns a reference to it with the slice lifetime.
    #[inline(always)]
    fn advance(&mut self) -> Option<&'a Shared<Token>> {
        let t = self.tokens.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    /// Consumes the current token, failing at end of input.
    #[inline(always)]
    fn advance_or_eof(&mut self) -> Result<&'a Shared<Token>, ParseError> {
        self.advance().ok_or(ParseError::UnexpectedEOFDetected)
    }

    /// Parses the tokens into top-level nodes and the errors found along the way.
    pub fn parse(&mut self) -> (Vec<Shared<Node>>, ErrorReporter) {
        let nodes = self.parse_program(ProgramKind::Root);
        (nodes, std::mem::take(&mut self.errors))
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
            TokenKind::DoubleDot => Some(BinaryOp::RangeOp),
            TokenKind::Slash => Some(BinaryOp::Division),
            TokenKind::PlusEqual => Some(BinaryOp::PlusEqual),
            TokenKind::MinusEqual => Some(BinaryOp::MinusEqual),
            TokenKind::SlashEqual => Some(BinaryOp::DivisionEqual),
            TokenKind::StarEqual => Some(BinaryOp::MultiplicationEqual),
            TokenKind::DoubleSlashEqual => Some(BinaryOp::DoubleDivisionEqual),
            TokenKind::PercentEqual => Some(BinaryOp::ModuloEqual),
            TokenKind::PipeEqual => Some(BinaryOp::PipeEqual),
            TokenKind::TildeEqual => Some(BinaryOp::RegexMatch),
            TokenKind::NotTildeEqual => Some(BinaryOp::NotRegexMatch),
            TokenKind::LeftShift => Some(BinaryOp::LeftShift),
            TokenKind::RightShift => Some(BinaryOp::RightShift),
            TokenKind::Convert => Some(BinaryOp::Convert),
            _ => None,
        }
    }

    fn parse_program(&mut self, kind: ProgramKind) -> Vec<Shared<Node>> {
        let root = kind == ProgramKind::Root;
        // Nested programs usually contain only a few nodes. Reserving for the
        // entire remaining token stream at every nesting level wastes memory.
        let mut nodes: Vec<Shared<Node>> = if root {
            Vec::with_capacity((self.tokens.len() - self.pos) / 4)
        } else {
            Vec::new()
        };
        let mut leading_trivia = self.parse_leading_trivia();

        // Set after a root-level `;`/`end` so a following one is handled as a terminator, not a statement.
        let mut after_terminator = false;
        // Whether the statement just parsed needs a `|` before the next one.
        let mut needs_separator = false;
        // The previous statement continued a selector chain, so an attribute there ends the chain.
        let mut in_chain_continuation = false;
        let mut chain_continuation_pending = false;

        while self.peek().is_some() {
            if !(after_terminator
                && self
                    .peek()
                    .is_some_and(|token| matches!(token.kind, TokenKind::SemiColon | TokenKind::End)))
            {
                let start = self.pos;
                let saved_trivia = leading_trivia.clone();
                needs_separator = false;
                in_chain_continuation = std::mem::take(&mut chain_continuation_pending);
                match self.parse_expr(leading_trivia, root) {
                    Ok(node) => {
                        needs_separator = !Self::closes_itself(&node);
                        nodes.push(node);
                    }
                    Err(e) => {
                        self.errors.report(e);
                        nodes.extend(self.recover_error(start, saved_trivia, &|kind, has_children| {
                            Self::is_statement_boundary(kind, has_children, root)
                        }));
                    }
                }

                leading_trivia = self.parse_leading_trivia();
            }
            after_terminator = false;

            let token = match self.peek() {
                Some(token) => Shared::clone(token),
                None => break,
            };

            match &token.kind {
                TokenKind::Eof if root => {
                    if !nodes.is_empty() {
                        self.pos += 1;

                        nodes.push(Shared::new(Node {
                            kind: NodeKind::Eof,
                            token: Some(Shared::clone(&token)),
                            leading_trivia,
                            trailing_trivia: TriviaList::new(),
                        }));
                    }

                    break;
                }
                TokenKind::Pipe => {
                    self.pos += 1;
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(&token)),
                        leading_trivia,
                        trailing_trivia,
                    }));
                    leading_trivia = self.parse_leading_trivia();

                    continue;
                }
                TokenKind::SemiColon | TokenKind::End => {
                    let is_end = matches!(token.kind, TokenKind::End);
                    let end_or_semi_token = Shared::clone(&token);
                    self.pos += 1;
                    let trailing_trivia = self.parse_trailing_trivia();

                    nodes.push(Shared::new(Node {
                        kind: if is_end { NodeKind::End } else { NodeKind::Token },
                        token: Some(Shared::clone(&end_or_semi_token)),
                        leading_trivia,
                        trailing_trivia,
                    }));

                    if root {
                        leading_trivia = self.parse_leading_trivia();

                        if let Some(next_token) = self.peek() {
                            let next_token = Shared::clone(next_token);
                            if matches!(next_token.kind, TokenKind::Eof) {
                                self.pos += 1;

                                nodes.push(Shared::new(Node {
                                    kind: NodeKind::Eof,
                                    token: Some(next_token),
                                    leading_trivia,
                                    trailing_trivia: TriviaList::new(),
                                }));
                                break;
                            } else if matches!(next_token.kind, TokenKind::Pipe) {
                                // A `;` or `end` that closes nothing cannot be followed by a pipe.
                                self.errors.report(if is_end {
                                    ParseError::UnmatchedEnd(end_or_semi_token)
                                } else {
                                    ParseError::UnexpectedToken(Shared::clone(&next_token))
                                });
                                self.pos += 1;
                                nodes.push(Shared::new(Node {
                                    kind: NodeKind::Token,
                                    token: Some(next_token),
                                    leading_trivia,
                                    trailing_trivia: self.parse_trailing_trivia(),
                                }));
                                leading_trivia = self.parse_leading_trivia();
                                continue;
                            }

                            self.errors.report(if is_end {
                                ParseError::UnmatchedEnd(end_or_semi_token)
                            } else {
                                ParseError::UnexpectedToken(next_token)
                            });
                        }
                        after_terminator = true;
                        continue;
                    }

                    break;
                }
                // The `,` or closing bracket of the enclosing construct (or EOF, which it reports) ends the body.
                TokenKind::Comma | TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace | TokenKind::Eof
                    if !root =>
                {
                    // Leave the trivia for the enclosing program, which owns this token.
                    self.pos -= leading_trivia.len();
                    break;
                }
                _ if Self::starts_expression(&token.kind, root) => {
                    chain_continuation_pending =
                        self.check_adjacent_statement(nodes.last(), &token, needs_separator, in_chain_continuation);
                }
                _ if root => {
                    self.report_unexpected(&token);
                    let start = self.pos;
                    nodes.extend(self.recover_error(start, leading_trivia, &|kind, has_children| {
                        Self::is_statement_boundary(kind, has_children, true)
                    }));
                    leading_trivia = self.parse_leading_trivia();

                    if let Some(eof) = self.peek().filter(|token| matches!(token.kind, TokenKind::Eof)) {
                        self.pos += 1;
                        nodes.push(Shared::new(Node {
                            kind: NodeKind::Eof,
                            token: Some(Shared::clone(eof)),
                            leading_trivia,
                            trailing_trivia: TriviaList::new(),
                        }));
                        break;
                    }
                }
                _ => {
                    self.report_unexpected(&token);
                    // Leave the trivia for the enclosing program, which owns this token.
                    self.pos -= leading_trivia.len();
                    break;
                }
            }
        }

        if nodes.is_empty() {
            match self.peek() {
                Some(token) => self.report_unexpected(token),
                None => self.errors.report(ParseError::UnexpectedEOFDetected),
            };
        }

        nodes
    }

    fn parse_expr(&mut self, leading_trivia: TriviaList, root: bool) -> Result<Shared<Node>, ParseError> {
        self.parse_equality_expr(leading_trivia, root)
    }

    /// Precedence of the binary operator following the trivia at the cursor, if any.
    fn peek_binary_op_precedence(&self) -> Option<u8> {
        self.tokens
            .get(self.pos..)?
            .iter()
            .find(|t| !t.kind.is_trivia())
            .and_then(|t| t.kind.binary_op_precedence())
    }

    /// Precedence climbing over binary operators, left-associative like the AST parser.
    fn parse_binary_expr(
        &mut self,
        mut lhs: Shared<Node>,
        min_prec: u8,
        root: bool,
    ) -> Result<Shared<Node>, ParseError> {
        while let Some(prec) = self.peek_binary_op_precedence() {
            if prec < min_prec {
                break;
            }

            // `nodes` is a statement of its own, not an operand.
            if matches!(lhs.kind, NodeKind::Nodes)
                && let Some(operator) = self.peek_non_trivia()
            {
                self.report_unexpected(operator);
            }

            let leading_trivia = self.parse_leading_trivia();
            let operator_token = self.advance_or_eof()?;
            let binary_op = Self::token_kind_to_binary_op(&operator_token.kind)
                .ok_or_else(|| ParseError::UnexpectedToken(Shared::clone(operator_token)))?;
            let is_assignment = binary_op.is_assignment();
            let op_trailing_trivia = self.parse_trailing_trivia();

            let rhs_leading_trivia = self.parse_leading_trivia();
            let mut rhs = self.parse_primary_expr(rhs_leading_trivia, root)?;
            if matches!(rhs.kind, NodeKind::Nodes)
                && let Some(token) = &rhs.token
            {
                self.report_unexpected(token);
            }

            while let Some(next_prec) = self.peek_binary_op_precedence() {
                if next_prec <= prec {
                    break;
                }
                rhs = self.parse_binary_expr(rhs, next_prec, root)?;
            }

            if is_assignment {
                if binary_op == BinaryOp::PipeEqual {
                    // `|=` sets an attribute, so it must follow one (`x.url |= v`).
                    if !Self::ends_with_attribute(&lhs) {
                        self.report_unexpected(operator_token);
                    }
                } else if !Self::is_assignable(&lhs) {
                    let found = lhs.token.as_ref().unwrap_or(operator_token);
                    self.errors
                        .report(ParseError::InvalidAssignmentTarget(Shared::clone(found)));
                }
            }

            let node_kind = if is_assignment {
                NodeKind::Assign {
                    op: binary_op,
                    lhs,
                    rhs,
                }
            } else {
                NodeKind::BinaryOp {
                    op: binary_op,
                    lhs,
                    rhs,
                }
            };

            lhs = Shared::new(Node {
                kind: node_kind,
                token: Some(Shared::clone(operator_token)),
                leading_trivia,
                trailing_trivia: op_trailing_trivia,
            });
        }

        Ok(lhs)
    }

    fn parse_equality_expr(&mut self, leading_trivia: TriviaList, root: bool) -> Result<Shared<Node>, ParseError> {
        let lhs = self.parse_primary_expr(leading_trivia, root)?;
        let lhs = self.parse_binary_expr(lhs, 0, root)?;

        if self.try_next_token(|kind| matches!(kind, TokenKind::As)) {
            let as_leading_trivia = self.parse_leading_trivia();
            let as_token = self.advance_or_eof()?;
            let as_trailing_trivia = self.parse_trailing_trivia();

            let name_leading_trivia = self.parse_leading_trivia();
            let name_node = match self.peek() {
                Some(token) if matches!(token.kind, TokenKind::Ident(_)) => self.parse_ident(name_leading_trivia)?,
                Some(token) => return Err(ParseError::UnexpectedToken(Shared::clone(token))),
                None => return Err(ParseError::UnexpectedEOFDetected),
            };
            // The bound name is a plain identifier, not a call or an access.
            if !matches!(name_node.kind, NodeKind::Ident { attr: None })
                && let Some(token) = &name_node.token
            {
                self.report_unexpected(token);
            }

            let as_node = Node {
                kind: NodeKind::As {
                    expr: lhs,
                    name: name_node,
                },
                token: Some(Shared::clone(as_token)),
                leading_trivia: as_leading_trivia,
                trailing_trivia: as_trailing_trivia,
            };
            return Ok(Shared::new(as_node));
        }

        Ok(lhs)
    }

    fn parse_primary_expr(&mut self, leading_trivia: TriviaList, root: bool) -> Result<Shared<Node>, ParseError> {
        let token = self.peek().ok_or(ParseError::UnexpectedEOFDetected)?;

        match &token.kind {
            TokenKind::Def => self.parse_def(leading_trivia),
            TokenKind::Do => self.parse_block(leading_trivia),
            TokenKind::Fn | TokenKind::Arrow => self.parse_fn(leading_trivia),
            TokenKind::If => self.parse_if(leading_trivia),
            TokenKind::Foreach => self.parse_foreach(leading_trivia),
            TokenKind::Include => self.parse_include(leading_trivia),
            TokenKind::Import => self.parse_import(leading_trivia),
            TokenKind::Module => self.parse_module(leading_trivia),
            TokenKind::While => self.parse_while(leading_trivia),
            TokenKind::Loop => self.parse_loop(leading_trivia),
            TokenKind::Until => self.parse_until(leading_trivia),
            TokenKind::Unless => self.parse_unless(leading_trivia),
            TokenKind::Try => self.parse_try(leading_trivia),
            TokenKind::Match => self.parse_match(leading_trivia),
            TokenKind::Ident(_) => self.parse_ident(leading_trivia),
            TokenKind::Self_ => self.parse_self(leading_trivia),
            TokenKind::Let | TokenKind::Var => self.parse_var_decl(leading_trivia),
            TokenKind::Selector(_) | TokenKind::DoubleDot => self.parse_selector(leading_trivia),
            TokenKind::StringLiteral(_)
            | TokenKind::BytesLiteral(_)
            | TokenKind::NumberLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::None => {
                let literal = self.parse_node(NodeKind::Literal, leading_trivia)?;
                // A literal can be indexed, but not called.
                if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                    self.parse_postfix_chain(literal)
                } else {
                    Ok(literal)
                }
            }
            TokenKind::InterpolatedString(_) => {
                let node = self.parse_interpolated_string(leading_trivia)?;
                self.parse_postfix_chain(node)
            }
            TokenKind::LBracket => self.parse_array(leading_trivia),
            TokenKind::LBrace => self.parse_dict(leading_trivia),
            TokenKind::LParen => self.parse_group_expr(leading_trivia, root),
            TokenKind::Nodes if root => self.parse_nodes(leading_trivia),
            TokenKind::Env(_) => self.parse_node(NodeKind::Env, leading_trivia),
            TokenKind::Not | TokenKind::Minus => self.parse_unary_op(leading_trivia, root),
            TokenKind::Break => self.parse_break(leading_trivia),
            TokenKind::Continue => self.parse_node(NodeKind::Continue, leading_trivia),
            // Placement is validated later (HIR/compiler), not here, so the diagnostic is reachable.
            TokenKind::Yield => self.parse_yield(leading_trivia),
            TokenKind::Colon => self.parse_symbol(leading_trivia),
            TokenKind::Eof => {
                self.advance();
                Err(ParseError::UnexpectedEOFDetected)
            }
            _ => {
                let token = self.advance_or_eof()?;
                Err(ParseError::UnexpectedToken(Shared::clone(token)))
            }
        }
    }

    fn parse_group_expr(&mut self, leading_trivia: TriviaList, root: bool) -> Result<Shared<Node>, ParseError> {
        let lparen = self.next_node(|token_kind| matches!(token_kind, TokenKind::LParen), NodeKind::Token)?;

        self.peek_token()?;

        let expr_leading_trivia = self.parse_leading_trivia();
        let expr = self.parse_expr(expr_leading_trivia, root)?;
        let rparen = self.expect_node(
            |token_kind| matches!(token_kind, TokenKind::RParen),
            NodeKind::Token,
            "`)`",
        );

        let node = Node {
            kind: NodeKind::Group { lparen, expr, rparen },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        };

        // Handle postfix operations: (expr)(args), (expr)[N], (expr)(args)[N], etc.
        self.parse_postfix_chain(Shared::new(node))
    }

    fn parse_ident(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

        // Check for qualified access (module::function, module::value, or module::module2::method)
        if let Some(next_token) = self.peek_non_trivia()
            && matches!(next_token.kind, TokenKind::DoubleColon)
        {
            let mut node = Node {
                kind: NodeKind::QualifiedAccess { items: ArgList::new() },
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia: TriviaList::new(),
            };

            // Parse all :: separated identifiers
            while let Some(peek_token) = self.peek_non_trivia()
                && matches!(peek_token.kind, TokenKind::DoubleColon)
            {
                // Add double colon token
                children.push(self.next_node(|kind| matches!(kind, TokenKind::DoubleColon), NodeKind::Token)?);

                if let Some(next_token) = self.peek_non_trivia()
                    && !matches!(next_token.kind, TokenKind::Ident(_))
                {
                    // Kept as a node so completion works after `module::`, but it is still an error.
                    self.report_unexpected(next_token);
                    node.kind = NodeKind::QualifiedAccess { items: children.into() };
                    return Ok(Shared::new(node));
                }

                // Add identifier
                children.push(self.next_node(
                    |kind| matches!(kind, TokenKind::Ident(_)),
                    NodeKind::Ident { attr: None },
                )?);

                // Check if there's another :: or if we're at a function call
                if let Some(next_peek) = self.peek_non_trivia() {
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

            node.kind = NodeKind::QualifiedAccess { items: children.into() };
            // Check for bracket access after qualified access (e.g., module::func()[:1])
            if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                return self.parse_bracket_access(node);
            }
            return Ok(Shared::new(node));
        }

        let mut node = Node {
            kind: NodeKind::Ident { attr: None },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        };

        // Check for attribute access: ident.attr -> attr(ident, "attr")
        if let Some(attr_node) = self.try_parse_attribute_access(&mut node)? {
            return Ok(attr_node);
        }

        match self.peek_non_trivia() {
            Some(token) if matches!(token.kind, TokenKind::LParen) => {
                let mut args = self.parse_args()?;
                children.append(&mut args);

                let has_question = self.try_next_token(|kind| matches!(kind, TokenKind::Question));
                if has_question {
                    children
                        .push(self.next_node(|token_kind| matches!(token_kind, TokenKind::Question), NodeKind::Token)?);
                }

                node.kind = NodeKind::Call { args: children.into() };

                // `f(x)?` ends the expression: no block, index or attribute follows it.
                if has_question {
                    return Ok(Shared::new(node));
                }

                // Check for a call with a trailing do-block argument (e.g., foo(args) do ...)
                if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::Do)) {
                    let leading_trivia = self.parse_leading_trivia();
                    let block = self.parse_block(leading_trivia)?;
                    let NodeKind::Call { args } = &mut node.kind else {
                        unreachable!("node.kind was just set to Call above");
                    };
                    args.push(block);

                    return Ok(Shared::new(node));
                }

                // Check for bracket access after function call (e.g., foo()[0])
                if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                    let result = self.parse_bracket_access(node)?;
                    self.parse_postfix_chain(result)
                } else if let Some(attr_node) = self.try_parse_attribute_access(&mut node)? {
                    Ok(attr_node)
                } else {
                    Ok(Shared::new(node))
                }
            }
            Some(token) if matches!(token.kind, TokenKind::LBracket) => {
                let result = self.parse_bracket_access(node)?;
                self.parse_postfix_chain(result)
            }
            _ => Ok(Shared::new(node)),
        }
    }

    fn try_parse_attribute_access(&mut self, node: &mut Node) -> Result<Option<Shared<Node>>, ParseError> {
        if let Some(peek_token) = self.peek_non_trivia()
            && matches!(&peek_token.kind, TokenKind::Selector(s) if s.len() > 1)
        {
            let selector_token = Shared::clone(peek_token);

            // Only attribute selectors (`.url`, `.level`) can follow a value.
            match Selector::try_from(&*selector_token) {
                Ok(selector) if selector.is_attribute_selector() => {}
                Err(error) => return Err(ParseError::UnknownSelector(error)),
                Ok(_) => return Err(ParseError::UnexpectedToken(selector_token)),
            }

            // The attribute may start on the next line; the trivia in between stays on its node.
            let adjacent = matches!(self.peek().map(|t| &t.kind), Some(TokenKind::Selector(_)));
            let leading_trivia = if adjacent {
                node.leading_trivia.clone()
            } else {
                self.parse_leading_trivia()
            };

            // Consume the selector token
            self.advance();

            // Create a Call node for attr(node, "attr")
            let attr_node = Shared::new(Node {
                kind: NodeKind::Selector { items: ArgList::new() },
                token: Some(Shared::clone(&selector_token)),
                leading_trivia,
                trailing_trivia: TriviaList::new(),
            });

            // Add the original node as the first argument
            match &mut node.kind {
                NodeKind::Call { args } => args.push(attr_node),
                NodeKind::Ident { attr } | NodeKind::Self_ { attr } => *attr = Some(attr_node),
                _ => unreachable!("attribute access only follows an ident, `self` or a call"),
            }

            Ok(Some(Shared::new(std::mem::take(node))))
        } else {
            Ok(None)
        }
    }

    // Parses bracket access operations recursively to handle nested access like arr[0][1][2]
    fn parse_bracket_access(&mut self, mut node: Node) -> Result<Shared<Node>, ParseError> {
        // A qualified access (`module::func()[0]`) stays whole as the first child of a nameless
        // `Call`, as in `parse_postfix_chain`. Merging its items into the `Call` would turn
        // `module` into a call name and lose the `module::member` structure that name
        // resolution relies on.
        if matches!(node.kind, NodeKind::QualifiedAccess { .. }) {
            node = Node {
                kind: NodeKind::Call {
                    args: std::iter::once(Shared::new(node)).collect(),
                },
                token: None,
                leading_trivia: TriviaList::new(),
                trailing_trivia: TriviaList::new(),
            };
        }

        // The node may already be a `Call` (e.g. from `parse_postfix_chain`'s wrapper, or
        // a preceding bracket access being extended by another `[...]`), in which case its
        // existing children live in `args` rather than the legacy `children` field.
        let mut children: Vec<Shared<Node>> = match &mut node.kind {
            NodeKind::Call { args } => std::mem::take(args).into_vec(),
            _ => Vec::new(),
        };

        // Parse bracket access: ident[key] -> get(ident, key) or ident[start:end] -> slice(ident, start, end)
        children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LBracket), NodeKind::Token)?);

        // Check for [:N] or [:] style slice (empty start index).
        // [:ident] and [:string] are dict key accesses using a symbol, not slices.
        let is_slice_from_start =
            self.try_next_token(|kind| matches!(kind, TokenKind::Colon)) && !self.is_symbol_after_colon();

        if !is_slice_from_start {
            // Parse the first expression
            let first_leading_trivia = self.parse_leading_trivia();
            let first_expr = self.parse_expr(first_leading_trivia, false)?;
            children.push(first_expr);
        }

        // Check if this is a slice operation (contains ':')
        let is_slice =
            is_slice_from_start || matches!(self.peek(), Some(token) if matches!(token.kind, TokenKind::Colon));

        if is_slice {
            // Add the colon token
            children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::Colon), NodeKind::Token)?);

            if !self.try_next_token(|kind| *kind == TokenKind::RBracket) {
                // Parse the second expression (end index)
                let second_leading_trivia = self.parse_leading_trivia();
                let second_expr = self.parse_expr(second_leading_trivia, false)?;
                children.push(second_expr);
            }
        }

        children.push(self.expect_node(
            |token_kind| matches!(token_kind, TokenKind::RBracket),
            NodeKind::Token,
            "`]`",
        ));

        node.kind = NodeKind::Call { args: children.into() };

        // Attribute access after indexing (`arr[0].attr`), possibly followed by more postfix operations.
        if let Some(with_attr) = self.try_parse_attribute_access(&mut node)? {
            return self.parse_postfix_chain(with_attr);
        }

        // Check for additional bracket access (nested indexing)
        let final_node = if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::LBracket)) {
            self.parse_bracket_access(node)?
        } else {
            Shared::new(node)
        };

        // Check for function call after bracket access (e.g., arr[0]())
        // and handle further postfix operations (e.g., arr[0]()[1])
        if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::LParen)) {
            let args = self.parse_args()?;
            let dynamic_call_node = Node {
                token: final_node.token.clone(),
                leading_trivia: final_node.leading_trivia.clone(),
                trailing_trivia: TriviaList::new(),
                kind: NodeKind::CallDynamic {
                    callee: final_node,
                    args: args.into(),
                },
            };
            self.parse_postfix_chain(Shared::new(dynamic_call_node))
        } else {
            Ok(final_node)
        }
    }

    /// Parses postfix operations (dynamic calls and bracket access) after a primary expression.
    /// Handles chains like `(expr)(args)[N]`, `expr[N](args)[M]`, etc.
    fn parse_postfix_chain(&mut self, mut current: Shared<Node>) -> Result<Shared<Node>, ParseError> {
        loop {
            if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::LParen)) {
                let token = current.token.clone();
                let leading_trivia = current.leading_trivia.clone();
                let args = self.parse_args()?;
                current = Shared::new(Node {
                    kind: NodeKind::CallDynamic {
                        callee: current,
                        args: args.into(),
                    },
                    token,
                    leading_trivia,
                    trailing_trivia: TriviaList::new(),
                });
            } else if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                // Wrap current as first child of a new Call node, then parse bracket access
                let wrapper = Node {
                    kind: NodeKind::Call {
                        args: std::iter::once(current).collect(),
                    },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                };
                current = self.parse_bracket_access(wrapper)?;
            } else if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::Selector(s)) if s.len() > 1) {
                // Attribute access on the expression: `[1].row`, `{"a": 1}.attr`.
                let mut wrapper = Node {
                    kind: NodeKind::Call {
                        args: std::iter::once(current).collect(),
                    },
                    token: None,
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                };
                let Some(with_attr) = self.try_parse_attribute_access(&mut wrapper)? else {
                    unreachable!("a selector token was just seen");
                };
                current = with_attr;
            } else {
                break;
            }
        }
        Ok(current)
    }

    fn parse_args(&mut self) -> Result<Vec<Shared<Node>>, ParseError> {
        let mut nodes: Vec<Shared<Node>> = Vec::with_capacity(4);

        nodes.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LParen), NodeKind::Token)?);

        // Arguments start their own selector chains, independent of an enclosing selector tail.
        let outer_in_selector_tail = std::mem::replace(&mut self.in_selector_tail, false);
        self.parse_separated_list(
            &mut nodes,
            |kind| matches!(kind, TokenKind::RParen),
            "`)`",
            |parser| parser.parse_arg(),
        );
        self.in_selector_tail = outer_in_selector_tail;

        Ok(nodes)
    }

    fn parse_arg(&mut self) -> Result<Shared<Node>, ParseError> {
        let leading_trivia = self.parse_leading_trivia();
        let token = Shared::clone(self.peek_token()?);

        // Any expression can be an argument, including `let` and `match`.
        if !Self::starts_expression(&token.kind, false) {
            return Err(ParseError::UnexpectedToken(token));
        }

        let expr = self.parse_expr(leading_trivia, false)?;
        self.parse_implicit_pipeline(expr)
    }

    /// If the next token is `|`, collects remaining pipeline steps and wraps them
    /// together with `first_expr` in an implicit `Block` node (no `do`/`end` tokens).
    /// This allows `foo(a | b)` as a shorthand for `foo(do a | b end)`.
    fn parse_implicit_pipeline(&mut self, first_expr: Shared<Node>) -> Result<Shared<Node>, ParseError> {
        if !self.try_next_token(|kind| matches!(kind, TokenKind::Pipe)) {
            return Ok(first_expr);
        }

        let mut children: Vec<Shared<Node>> = vec![first_expr];

        while self.try_next_token(|kind| matches!(kind, TokenKind::Pipe)) {
            let pipe_leading_trivia = self.parse_leading_trivia();
            let pipe_token = self.advance_or_eof()?;
            let pipe_trailing_trivia = self.parse_trailing_trivia();

            children.push(Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(pipe_token)),
                leading_trivia: pipe_leading_trivia,
                trailing_trivia: pipe_trailing_trivia,
            }));

            let next_leading_trivia = self.parse_leading_trivia();
            let next_expr = self.parse_expr(next_leading_trivia, false)?;
            children.push(next_expr);
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Block {
                program: children.into(),
            },
            token: None,
            leading_trivia: TriviaList::new(),
            trailing_trivia: TriviaList::new(),
        }))
    }

    fn parse_def(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let name = self.next_node(
            |kind| matches!(kind, TokenKind::Ident(_)),
            NodeKind::Ident { attr: None },
        )?;

        let params = if self.try_next_token(|kind| matches!(kind, TokenKind::Colon | TokenKind::Do)) {
            None
        } else {
            Some(self.parse_params()?.into())
        };

        let colon_or_do = self.parse_colon_or_do_token_if_present()?;

        let program = self.parse_program(ProgramKind::Block);

        Ok(Shared::new(Node {
            kind: NodeKind::Def {
                name,
                params,
                colon_or_do,
                program: program.into(),
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_fn(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        // `fn: body` has no parameter list.
        let params =
            if matches!(token.kind, TokenKind::Fn) && self.try_next_token(|kind| matches!(kind, TokenKind::Colon)) {
                Vec::new()
            } else {
                self.parse_params()?
            };
        let colon_or_do = self.parse_colon_or_do_token_if_present()?;
        let program = self.parse_program(ProgramKind::Block);

        let node = Node {
            kind: NodeKind::Fn {
                params: params.into(),
                colon_or_do,
                program: program.into(),
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        };

        // Handle postfix operations: fn(...): ... end(args), fn(...): ... end(args)[N], etc.
        self.parse_postfix_chain(Shared::new(node))
    }

    fn parse_block(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();
        let program = self.parse_program(ProgramKind::Block);

        Ok(Shared::new(Node {
            kind: NodeKind::Block {
                program: program.into(),
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_selector(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        if matches!(&token.kind, TokenKind::Selector(s) if s == ".")
            && !self.try_next_token(|kind| matches!(kind, TokenKind::LBracket))
        {
            let mut node = Node {
                kind: NodeKind::Self_ { attr: None },
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia,
            };

            // `. .level` reads the attribute of the current node, like `.level`. Other selectors
            // after `.` are left for the descendant chain.
            let attribute_follows = self
                .peek_non_trivia()
                .is_some_and(|next| Self::is_attribute_token(next));
            if attribute_follows && let Some(with_attr) = self.try_parse_attribute_access(&mut node)? {
                return Ok(with_attr);
            }

            // `. .code` is a descendant chain starting at the current node.
            let selector_follows = !self.in_selector_tail
                && self.peek_non_trivia().is_some_and(|next| {
                    next.is_selector() && !matches!(&next.kind, TokenKind::Selector(s) if s == ".")
                });
            if !selector_follows {
                return Ok(Shared::new(node));
            }

            self.in_selector_tail = true;
            let tail_leading_trivia = self.parse_leading_trivia();
            let tail = self.parse_selector(tail_leading_trivia);
            self.in_selector_tail = false;

            // Shaped like the other selector chains: the block carries the first token and trivia.
            return Ok(Shared::new(Node {
                token: node.token.clone(),
                leading_trivia: node.leading_trivia.clone(),
                trailing_trivia: node.trailing_trivia.clone(),
                kind: NodeKind::Block {
                    program: vec![Shared::new(node), tail?].into(),
                },
            }));
        }

        let mut node = Node {
            kind: NodeKind::Selector { items: ArgList::new() },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        };

        match &token.kind {
            TokenKind::Selector(s) if s == "." => {
                let mut children: Vec<Shared<Node>> = Vec::with_capacity(6);

                // []
                self.parse_selector_bracket(&mut children)?;

                let token = Shared::clone(self.peek_non_trivia().ok_or(ParseError::UnexpectedEOFDetected)?);

                // [][]
                if token.kind == TokenKind::LBracket {
                    self.parse_selector_bracket(&mut children)?;
                } else {
                    node.kind = NodeKind::Selector { items: children.into() };
                    return Ok(Shared::new(node));
                }

                node.kind = NodeKind::Selector { items: children.into() };
                Ok(Shared::new(node))
            }
            TokenKind::DoubleDot => {
                // `..h` and `.."key"` are one expression, as in the AST parser.
                if !self.peek().is_some_and(|next| Self::is_recursive_name(&next.kind)) {
                    return Ok(Shared::new(node));
                }

                let name_kind = if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::StringLiteral(_))) {
                    NodeKind::Literal
                } else {
                    NodeKind::Selector { items: ArgList::new() }
                };
                let name = self.next_node(
                    |kind| matches!(kind, TokenKind::Ident(_) | TokenKind::StringLiteral(_)),
                    name_kind,
                )?;

                let mut program = vec![Shared::new(node), name];

                // `..h.level` and `..h .code`: an attribute or a descendant step continues the expression.
                let tail_follows = !self.in_selector_tail
                    && self.peek_non_trivia().is_some_and(|next| {
                        next.is_selector() && !matches!(&next.kind, TokenKind::Selector(s) if s == ".")
                    });
                if tail_follows {
                    self.in_selector_tail = true;
                    let tail_leading_trivia = self.parse_leading_trivia();
                    let tail = self.parse_selector(tail_leading_trivia);
                    self.in_selector_tail = false;
                    program.push(tail?);
                }

                // The formatter prints a block whose token is a selector as a selector chain.
                let marker = Shared::new(Token {
                    kind: TokenKind::Selector(SmolStr::new("..")),
                    range: token.range,
                    module_id: token.module_id,
                });
                Ok(Shared::new(Node {
                    kind: NodeKind::Block {
                        program: program.into(),
                    },
                    token: Some(marker),
                    leading_trivia: TriviaList::new(),
                    trailing_trivia: TriviaList::new(),
                }))
            }
            _ => {
                let selector = Selector::try_from(&**token).map_err(ParseError::UnknownSelector)?;

                if selector.is_attribute_selector() {
                    node.kind = NodeKind::SelfAttr;
                    let attribute = Shared::new(node);

                    // `.value .code` and `.value .value` are one expression: a selector after an
                    // attribute continues it, and a second attribute ends it.
                    let selector_follows = !self.in_selector_tail
                        && self.peek_non_trivia().is_some_and(|next| {
                            next.is_selector() && !matches!(&next.kind, TokenKind::Selector(s) if s == ".")
                        });
                    if !selector_follows {
                        return Ok(attribute);
                    }

                    self.in_selector_tail = true;
                    let tail_leading_trivia = self.parse_leading_trivia();
                    let tail = self.parse_selector(tail_leading_trivia);
                    self.in_selector_tail = false;

                    // Shaped like the other selector chains: the block carries the first token and trivia.
                    return Ok(Shared::new(Node {
                        token: attribute.token.clone(),
                        leading_trivia: attribute.leading_trivia.clone(),
                        trailing_trivia: attribute.trailing_trivia.clone(),
                        kind: NodeKind::Block {
                            program: vec![attribute, tail?].into(),
                        },
                    }));
                }

                // Check for selector call args: `.h(...)`, `.code(...)`
                if self.try_next_token(|kind| matches!(kind, TokenKind::LParen)) {
                    let mut args: ArgList = self.parse_args()?.into();
                    // Check for attribute access after SelectorCall: `.h(1).level`
                    if let Some(peek_token) = self.peek()
                        && peek_token.is_selector()
                        && Selector::try_from(&**peek_token)
                            .map_err(ParseError::UnknownSelector)?
                            .is_attribute_selector()
                    {
                        let attr_child = self.next_node(
                            |kind| matches!(kind, TokenKind::Selector(_)),
                            NodeKind::Selector { items: ArgList::new() },
                        )?;
                        args.push(attr_child);
                    }
                    node.kind = NodeKind::SelectorCall { args };
                    return Ok(Shared::new(node));
                }

                // Handle chained property selectors: .a.b.c → Block([Selector(.a), Selector(.b), Selector(.c)])
                if matches!(selector, Selector::Property(_))
                    && let Some(peek_token) = self.peek()
                    && peek_token.is_selector()
                    && matches!(Selector::try_from(&**peek_token), Ok(Selector::Property(_)))
                {
                    let first_token = Shared::clone(token);
                    let block_leading = node.leading_trivia.clone();
                    let block_trailing = node.trailing_trivia.clone();
                    let mut nodes = vec![Shared::new(node)];
                    while let Some(peek_token) = self.peek()
                        && peek_token.is_selector()
                        && matches!(Selector::try_from(&**peek_token), Ok(Selector::Property(_)))
                    {
                        nodes.push(self.next_node(
                            |kind| matches!(kind, TokenKind::Selector(_)),
                            NodeKind::Selector { items: ArgList::new() },
                        )?);
                    }
                    // ."a"."b"[] or ."a"."b"[n]: array iterator/indexer after chained properties
                    if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                        nodes.push(self.build_list_selector_node()?);
                    }
                    return Ok(Shared::new(Node {
                        kind: NodeKind::Block { program: nodes.into() },
                        token: Some(first_token),
                        leading_trivia: block_leading,
                        trailing_trivia: block_trailing,
                    }));
                }

                // ."key"[] or ."key"[n]: property + array iterator/indexer
                if matches!(selector, Selector::Property(_))
                    && matches!(self.peek().map(|t| &t.kind), Some(TokenKind::LBracket))
                {
                    let first_token = Shared::clone(token);
                    let block_leading = node.leading_trivia.clone();
                    let block_trailing = node.trailing_trivia.clone();
                    let list_node = self.build_list_selector_node()?;
                    return Ok(Shared::new(Node {
                        kind: NodeKind::Block {
                            program: vec![Shared::new(node), list_node].into(),
                        },
                        token: Some(first_token),
                        leading_trivia: block_leading,
                        trailing_trivia: block_trailing,
                    }));
                }

                if let Some(attr_token) = self.peek()
                    && attr_token.is_selector()
                    && Selector::try_from(&**attr_token)
                        .map_err(ParseError::UnknownSelector)?
                        .is_attribute_selector()
                {
                    node.kind = NodeKind::Selector {
                        items: vec![self.next_node(
                            |kind| matches!(kind, TokenKind::Selector(_)),
                            NodeKind::Selector { items: ArgList::new() },
                        )?]
                        .into(),
                    };
                    return Ok(Shared::new(node));
                }

                if let Some(next_token) = self.peek()
                    && next_token.is_selector()
                    && matches!(Selector::try_from(&**next_token), Ok(sel) if !sel.is_attribute_selector())
                {
                    let first_token = Shared::clone(token);
                    let block_leading = node.leading_trivia.clone();
                    let mut nodes = vec![Shared::new(node)];

                    while let Some(step_token) = self.peek() {
                        if !step_token.is_selector() {
                            break;
                        }

                        match Selector::try_from(&**step_token) {
                            Ok(sel) if sel.is_attribute_selector() => {
                                nodes.push(self.next_node(
                                    |kind| matches!(kind, TokenKind::Selector(_)),
                                    NodeKind::Selector { items: ArgList::new() },
                                )?);
                                break;
                            }
                            Ok(_) => {
                                let leading_trivia = self.parse_leading_trivia();
                                let step_src_token = self.next_token(|kind| matches!(kind, TokenKind::Selector(_)))?;
                                let trailing_trivia = self.parse_trailing_trivia();
                                let mut step_node = Node {
                                    kind: NodeKind::Selector { items: ArgList::new() },
                                    token: Some(Shared::clone(&step_src_token)),
                                    leading_trivia,
                                    trailing_trivia,
                                };
                                if self.try_next_token(|kind| matches!(kind, TokenKind::LParen)) {
                                    step_node.kind = NodeKind::SelectorCall {
                                        args: self.parse_args()?.into(),
                                    };
                                }
                                nodes.push(Shared::new(step_node));
                            }
                            Err(_) => break,
                        }
                    }

                    return Ok(Shared::new(Node {
                        kind: NodeKind::Block { program: nodes.into() },
                        token: Some(first_token),
                        leading_trivia: block_leading,
                        trailing_trivia: TriviaList::new(),
                    }));
                }

                Ok(Shared::new(node))
            }
        }
    }

    fn build_list_selector_node(&mut self) -> Result<Shared<Node>, ParseError> {
        let (range, module_id) = {
            let bracket = self.peek().ok_or(ParseError::UnexpectedEOFDetected)?;
            (bracket.range, bracket.module_id)
        };
        let dot_token = Shared::new(Token {
            kind: TokenKind::Selector(SmolStr::new(".")),
            range,
            module_id,
        });
        let mut bracket_children: Vec<Shared<Node>> = Vec::new();
        self.parse_selector_bracket(&mut bracket_children)?;
        Ok(Shared::new(Node {
            kind: NodeKind::Selector {
                items: bracket_children.into(),
            },
            token: Some(dot_token),
            leading_trivia: TriviaList::new(),
            trailing_trivia: TriviaList::new(),
        }))
    }

    /// Parses a single bracket index `[N]` for selector expressions, pushing tokens onto `children`.
    fn parse_selector_bracket(&mut self, children: &mut Vec<Shared<Node>>) -> Result<(), ParseError> {
        children.push(self.next_node(|kind| matches!(kind, TokenKind::LBracket), NodeKind::Token)?);

        // The index is any expression: `.[0]`, `.["key"]`, `.[n + 1]`.
        let next = self.peek_non_trivia().ok_or(ParseError::UnexpectedEOFDetected)?;
        if next.kind != TokenKind::RBracket {
            let leading_trivia = self.parse_leading_trivia();
            children.push(self.parse_expr(leading_trivia, false)?);
        }

        let token = self.peek_non_trivia().ok_or(ParseError::UnexpectedEOFDetected)?;

        if token.kind == TokenKind::RBracket {
            children.push(self.next_node(|kind| matches!(kind, TokenKind::RBracket), NodeKind::Token)?);
        } else {
            return Err(ParseError::UnexpectedToken(Shared::clone(token)));
        }

        Ok(())
    }

    fn parse_include(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();
        let path = self.next_node(|kind| matches!(kind, TokenKind::StringLiteral(_)), NodeKind::Literal)?;

        Ok(Shared::new(Node {
            kind: NodeKind::Include { path },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_import(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();
        let path = self.next_node(|kind| matches!(kind, TokenKind::StringLiteral(_)), NodeKind::Literal)?;

        let (as_token, alias) = if self.try_next_token(|kind| matches!(kind, TokenKind::As)) {
            (
                Some(self.next_node(|kind| matches!(kind, TokenKind::As), NodeKind::Token)?),
                Some(self.next_node(
                    |kind| matches!(kind, TokenKind::Ident(_)),
                    NodeKind::Ident { attr: None },
                )?),
            )
        } else {
            (None, None)
        };

        Ok(Shared::new(Node {
            kind: NodeKind::Import { path, as_token, alias },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_module(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();
        let name = self.next_node(
            |kind| matches!(kind, TokenKind::Ident(_)),
            NodeKind::Ident { attr: None },
        )?;
        let colon_or_do = self.parse_colon_or_do_token_if_present()?;

        // Parse program block (contains let, def, or module statements).
        // parse_program reports errors directly into self.errors, so no merging is needed here.
        let program = self.parse_program(ProgramKind::Block);

        // Only `let`, `def`, `module` and `import` can appear in a module body.
        for node in &program {
            if !matches!(
                node.kind,
                NodeKind::Let { .. }
                    | NodeKind::Def { .. }
                    | NodeKind::Module { .. }
                    | NodeKind::Import { .. }
                    | NodeKind::Token
                    | NodeKind::End
                    | NodeKind::Eof
                    | NodeKind::Error { .. }
                    | NodeKind::Missing { .. }
            ) && let Some(token) = Self::first_token(node)
            {
                self.report_unexpected(&token);
            }
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Module {
                name,
                colon_or_do,
                program: program.into(),
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_if(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let args = self.parse_args()?;
        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
            return Err(ParseError::UnexpectedToken(Shared::clone(token)));
        }

        let colon = self.parse_colon_token_if_present()?;

        let then_leading_trivia = self.parse_leading_trivia();
        let then_branch = self.parse_expr(then_leading_trivia, false)?;

        let mut elifs: Elifs = Elifs::new();
        while self.try_next_token(|kind| matches!(kind, TokenKind::Elif)) {
            let elif_leading_trivia = self.parse_leading_trivia();
            elifs.push(self.parse_elif(elif_leading_trivia)?);
        }

        let else_branch = if self.try_next_token(|kind| matches!(kind, TokenKind::Else)) {
            let else_leading_trivia = self.parse_leading_trivia();
            Some(self.parse_else(else_leading_trivia)?)
        } else {
            None
        };

        Ok(Shared::new(Node {
            kind: NodeKind::If {
                args: args.into(),
                colon,
                then_branch,
                elifs,
                else_branch,
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_elif(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let args = self.parse_args()?;
        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
            return Err(ParseError::UnexpectedToken(Shared::clone(token)));
        }

        let colon = self.parse_colon_token_if_present()?;

        let then_leading_trivia = self.parse_leading_trivia();
        let then_branch = self.parse_expr(then_leading_trivia, false)?;

        Ok(Shared::new(Node {
            kind: NodeKind::Elif {
                args: args.into(),
                colon,
                then_branch,
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_else(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let colon = self.parse_colon_token_if_present()?;

        let then_leading_trivia = self.parse_leading_trivia();
        let then_branch = self.parse_expr(then_leading_trivia, false)?;

        Ok(Shared::new(Node {
            kind: NodeKind::Else { colon, then_branch },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    #[inline]
    fn parse_node(&mut self, node_kind: NodeKind, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        Ok(Shared::new(Node {
            kind: node_kind,
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_break(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();
        // Optionally parse colon and expression (break: expr)
        let (colon, value) = if self.try_next_token(|kind| matches!(kind, TokenKind::Colon)) {
            let colon = self.parse_colon_token_if_present()?;
            let value_leading_trivia = self.parse_leading_trivia();
            (colon, Some(self.parse_expr(value_leading_trivia, false)?))
        } else {
            (None, None)
        };

        Ok(Shared::new(Node {
            kind: NodeKind::Break { colon, value },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_yield(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();
        // Optionally parse colon and expression (yield: expr)
        let (colon, value) = if self.try_next_token(|kind| matches!(kind, TokenKind::Colon)) {
            let colon = self.parse_colon_token_if_present()?;
            let value_leading_trivia = self.parse_leading_trivia();
            (colon, Some(self.parse_expr(value_leading_trivia, false)?))
        } else {
            (None, None)
        };

        Ok(Shared::new(Node {
            kind: NodeKind::Yield { colon, value },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    /// Parses a `...expr` spread element inside an array or dict literal, mirroring
    /// `parse_unary_op`'s shape: the `...` token lives on the node itself, and the
    /// spread target is the sole child.
    fn parse_spread(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let dots_token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let operand_leading_trivia = self.parse_leading_trivia();
        let operand = self.parse_expr(operand_leading_trivia, false)?;

        Ok(Shared::new(Node {
            kind: NodeKind::Spread { operand },
            token: Some(Shared::clone(dots_token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_array(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(12);

        children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LBracket), NodeKind::Token)?);

        self.parse_separated_list(
            &mut children,
            |kind| matches!(kind, TokenKind::RBracket),
            "`]`",
            |parser| {
                let leading_trivia = parser.parse_leading_trivia();
                if matches!(parser.peek_token()?.kind, TokenKind::DotDotDot) {
                    parser.parse_spread(leading_trivia)
                } else {
                    parser.parse_expr(leading_trivia, false)
                }
            },
        );

        let array_node = Shared::new(Node {
            kind: NodeKind::Array { items: children.into() },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        });

        // Handle postfix bracket access: [1,2,3][0], [1,2,3][0][1], etc.
        self.parse_postfix_chain(array_node)
    }

    #[inline]
    fn parse_dict_key(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = Shared::clone(self.peek_token()?);

        match &token.kind {
            TokenKind::Ident(_) | TokenKind::StringLiteral(_) => {
                let key = self.parse_expr(leading_trivia, false)?;
                // A key is a bare name or string, not an expression such as `a = b`.
                let plain = matches!(
                    (&key.kind, key.token.as_ref().map(|t| &t.kind)),
                    (NodeKind::Ident { attr: None }, Some(TokenKind::Ident(_)))
                        | (NodeKind::Literal, Some(TokenKind::StringLiteral(_)))
                );
                if !plain && let Some(token) = Self::first_token(&key) {
                    self.report_unexpected(&token);
                }
                Ok(key)
            }
            _ => Err(ParseError::UnexpectedToken(Shared::clone(&token))),
        }
    }

    fn parse_dict(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(12);

        children.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LBrace), NodeKind::Token)?);

        self.parse_separated_list(
            &mut children,
            |kind| matches!(kind, TokenKind::RBrace),
            "`}`",
            |parser| {
                let leading_trivia = parser.parse_leading_trivia();

                if matches!(parser.peek_token()?.kind, TokenKind::DotDotDot) {
                    return parser.parse_spread(leading_trivia);
                }

                let key = parser.parse_dict_key(TriviaList::new())?;
                let colon = parser.next_node(|token_kind| matches!(token_kind, TokenKind::Colon), NodeKind::Token)?;
                let value_leading_trivia = parser.parse_leading_trivia();
                let value = parser.parse_expr(value_leading_trivia, false)?;

                Ok(Shared::new(Node {
                    kind: NodeKind::DictEntry { key, colon, value },
                    token: None,
                    leading_trivia,
                    trailing_trivia: TriviaList::new(),
                }))
            },
        );

        let dict_node = Shared::new(Node {
            kind: NodeKind::Dict {
                entries: children.into(),
            },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        });

        self.parse_postfix_chain(dict_node)
    }

    fn parse_interpolated_string(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        if let TokenKind::InterpolatedString(_) = &token.kind {
            self.validate_interpolations(token);
            Ok(Shared::new(Node {
                kind: NodeKind::InterpolatedString,
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia,
            }))
        } else {
            Err(ParseError::UnexpectedToken(Shared::clone(token)))
        }
    }

    /// Parses each `${...}` expression of an interpolated string and reports its errors, as the AST parser does.
    fn validate_interpolations(&mut self, token: &Shared<Token>) {
        let TokenKind::InterpolatedString(segments) = &token.kind else {
            return;
        };

        for segment in segments {
            let crate::lexer::token::StringSegment::Expr(source, range) = segment else {
                continue;
            };
            let trimmed = source.trim();
            if trimmed == constants::identifiers::SELF || trimmed.starts_with('$') {
                continue;
            }

            let start = Position {
                line: range.start.line,
                column: range.start.column + 2,
            };
            let lexer = crate::Lexer::new(crate::lexer::Options {
                ignore_errors: false,
                include_spaces: true,
            });
            let tokens = match lexer.tokenize_from(source, token.module_id, start) {
                Ok(tokens) => tokens,
                Err(error) => {
                    self.errors
                        .report(error.token().map_or(ParseError::UnexpectedEOFDetected, |token| {
                            ParseError::UnexpectedToken(Shared::new(token.clone()))
                        }));
                    continue;
                }
            };

            if tokens.iter().all(|token| token.kind.is_trivia() || token.is_eof()) {
                self.errors.report(ParseError::UnexpectedEOFDetected);
                continue;
            }

            let tokens: Vec<Shared<Token>> = tokens.into_iter().map(Shared::new).collect();
            let (nodes, errors) = Parser::new(&tokens).parse();
            for error in errors.to_vec() {
                self.errors.report(error);
            }
            // The segment is an expression or a pipeline: expressions alternate with pipes.
            let is_pipe = |node: &Shared<Node>| {
                matches!(node.kind, NodeKind::Token) && node.token.as_ref().is_some_and(|t| t.kind == TokenKind::Pipe)
            };
            let parts = nodes.iter().filter(|node| !matches!(node.kind, NodeKind::Eof));
            if let Some((_, misplaced)) = parts.enumerate().find(|(i, node)| is_pipe(node) != (i % 2 == 1))
                && let Some(token) = Self::first_token(misplaced)
            {
                self.report_unexpected(&token);
            }
        }
    }

    fn parse_var_decl(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let is_var = matches!(token.kind, TokenKind::Var);
        let trailing_trivia = self.parse_trailing_trivia();

        let lhs_leading_trivia = self.parse_leading_trivia();
        let lhs = match self.peek().map(|t| &t.kind) {
            Some(TokenKind::LBracket) => self.parse_array_pattern(lhs_leading_trivia)?,
            Some(TokenKind::LBrace) => self.parse_dict_pattern(lhs_leading_trivia)?,
            _ => {
                let lhs = self.parse_ident(lhs_leading_trivia)?;
                // The bound name is a plain identifier.
                let plain = matches!(lhs.kind, NodeKind::Ident { attr: None })
                    && lhs
                        .token
                        .as_ref()
                        .is_some_and(|t| matches!(t.kind, TokenKind::Ident(_)));
                if !plain && let Some(token) = &lhs.token {
                    self.report_unexpected(token);
                }
                lhs
            }
        };

        let eq_token = self.next_node(|kind| matches!(kind, TokenKind::Equal), NodeKind::Token)?;

        let rhs_leading_trivia = self.parse_leading_trivia();
        // A binding cannot be bound to another binding: `let x = let y = 1`.
        if let Some(next) = self.peek()
            && matches!(next.kind, TokenKind::Let | TokenKind::Var)
        {
            self.report_unexpected(next);
        }
        let rhs = self.parse_expr(rhs_leading_trivia, false)?;

        // A binding is followed by a pipe or the end of its block.
        if let Some(next) = self.peek_non_trivia()
            && !matches!(
                next.kind,
                TokenKind::Pipe | TokenKind::Eof | TokenKind::SemiColon | TokenKind::End
            )
        {
            self.report_unexpected(next);
        }

        let kind = if is_var {
            NodeKind::Var { lhs, eq_token, rhs }
        } else {
            NodeKind::Let { lhs, eq_token, rhs }
        };

        Ok(Shared::new(Node {
            kind,
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_self(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let mut node = Node {
            kind: NodeKind::Self_ { attr: None },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        };

        // Check for attribute access: ..attr -> attr(., "attr")
        if let Some(attr_node) = self.try_parse_attribute_access(&mut node)? {
            return Ok(attr_node);
        }

        match self.peek_non_trivia() {
            Some(token) if matches!(token.kind, TokenKind::LBracket) => self.parse_bracket_access(node),
            _ => Ok(Shared::new(node)),
        }
    }

    fn parse_nodes(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        Ok(Shared::new(Node {
            kind: NodeKind::Nodes,
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_symbol(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let colon = self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?;

        // Parse the identifier or string literal that follows
        let name_leading_trivia = self.parse_leading_trivia();
        let name = match self.peek() {
            Some(token) => match &token.kind {
                TokenKind::Ident(_) | TokenKind::StringLiteral(_) => {
                    let token = self.advance_or_eof()?;
                    let trailing_trivia = self.parse_trailing_trivia();
                    Shared::new(Node {
                        kind: NodeKind::Literal,
                        token: Some(Shared::clone(token)),
                        leading_trivia: name_leading_trivia,
                        trailing_trivia,
                    })
                }
                _ => return Err(ParseError::UnexpectedToken(Shared::clone(token))),
            },
            None => return Err(ParseError::UnexpectedEOFDetected),
        };

        Ok(Shared::new(Node {
            kind: NodeKind::Symbol { colon, name },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        }))
    }

    fn parse_foreach(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let mut args: ArgList = ArgList::new();
        args.push(self.next_node(|kind| matches!(kind, TokenKind::LParen), NodeKind::Token)?);
        args.push(self.next_node(
            |kind| matches!(kind, TokenKind::Ident(_)),
            NodeKind::Ident { attr: None },
        )?);
        args.push(self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?);
        args.push(self.parse_arg()?);
        // A trailing comma is allowed, as in any argument list.
        if matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::Comma)) {
            args.push(self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?);
        }
        args.push(self.next_node(|kind| matches!(kind, TokenKind::RParen), NodeKind::Token)?);

        let colon_or_do = self.parse_colon_or_do_token_if_present()?;
        let program = self.parse_program(ProgramKind::Block);

        Ok(Shared::new(Node {
            kind: NodeKind::Foreach {
                args,
                colon_or_do,
                program: program.into(),
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_while(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let (token, trailing_trivia, args, colon_or_do, program) = self.parse_cond_loop()?;
        Ok(Shared::new(Node {
            kind: NodeKind::While {
                args,
                colon_or_do,
                program,
            },
            token: Some(token),
            leading_trivia,
            trailing_trivia,
        }))
    }

    /// Shared `keyword (cond) [:|do] program` parsing for `while`/`until`.
    #[allow(clippy::type_complexity)]
    fn parse_cond_loop(
        &mut self,
    ) -> Result<(Shared<Token>, TriviaList, ArgList, Option<Shared<Node>>, ArgList), ParseError> {
        let token = Shared::clone(self.advance_or_eof()?);
        let trailing_trivia = self.parse_trailing_trivia();

        let args = self.parse_args()?;
        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
            return Err(ParseError::UnexpectedToken(token));
        }
        let args: ArgList = args.into();

        let colon_or_do = self.parse_colon_or_do_token_if_present()?;
        let program = self.parse_program(ProgramKind::Block);

        Ok((token, trailing_trivia, args, colon_or_do, program.into()))
    }

    fn parse_loop(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let colon_or_do = self.parse_colon_or_do_token_if_present()?;
        let program = self.parse_program(ProgramKind::Block);

        Ok(Shared::new(Node {
            kind: NodeKind::Loop {
                colon_or_do,
                program: program.into(),
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_until(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let (token, trailing_trivia, args, colon_or_do, program) = self.parse_cond_loop()?;
        Ok(Shared::new(Node {
            kind: NodeKind::Until {
                args,
                colon_or_do,
                program,
            },
            token: Some(token),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_unless(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let args = self.parse_args()?;
        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
            return Err(ParseError::UnexpectedToken(Shared::clone(token)));
        }

        let colon = self.parse_colon_token_if_present()?;

        let then_leading_trivia = self.parse_leading_trivia();
        let then_branch = self.parse_expr(then_leading_trivia, false)?;

        Ok(Shared::new(Node {
            kind: NodeKind::Unless {
                args: args.into(),
                colon,
                then_branch,
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_try(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let colon_or_do = self.parse_colon_or_do_token_if_present()?;

        let body_leading_trivia = self.parse_leading_trivia();
        let body = self.parse_expr(body_leading_trivia, false)?;

        let catch = if self.try_next_token(|kind| matches!(kind, TokenKind::Catch)) {
            let catch_leading_trivia = self.parse_leading_trivia();
            Some(self.parse_catch(catch_leading_trivia)?)
        } else {
            None
        };

        Ok(Shared::new(Node {
            kind: NodeKind::Try {
                colon_or_do,
                body,
                catch,
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_catch(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        // Optional error binder: `catch(e):`
        let params = if self.peek().is_some_and(|t| matches!(t.kind, TokenKind::LParen)) {
            let params = self.parse_params()?;
            // The binder is a single plain name.
            let binders: Vec<&Shared<Node>> = params.iter().filter(|param| !param.is_token()).collect();
            match binders.as_slice() {
                [binder]
                    if matches!(
                        binder.kind,
                        NodeKind::Param {
                            asterisk: None,
                            eq_token: None,
                            ..
                        }
                    ) => {}
                [binder] => {
                    if let Some(token) = &binder.token {
                        self.report_unexpected(token);
                    }
                }
                _ => self.report_unexpected(token),
            }
            Some(params.into())
        } else {
            None
        };

        let colon_or_do = self.parse_colon_or_do_token_if_present()?;

        let body_leading_trivia = self.parse_leading_trivia();
        let body = self.parse_expr(body_leading_trivia, false)?;

        Ok(Shared::new(Node {
            kind: NodeKind::Catch {
                params,
                colon_or_do,
                body,
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_match(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        // Parse (value)
        let args = self.parse_args()?;
        if args.iter().filter(|arg| !arg.is_token()).count() != 1 {
            return Err(ParseError::UnexpectedToken(Shared::clone(token)));
        }

        let colon_or_do = self.parse_colon_or_do_token_if_present()?;

        // Parse match arms
        let mut arms: ArgList = ArgList::new();
        let mut end_token = None;
        loop {
            let leading_trivia = self.parse_leading_trivia();

            match self.peek() {
                Some(token) if matches!(token.kind, TokenKind::End) => {
                    end_token = Some(self.next_node(|kind| matches!(kind, TokenKind::End), NodeKind::End)?);
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
            arms.push(self.parse_match_arm(leading_trivia)?);
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Match {
                args: args.into(),
                colon_or_do,
                arms,
                end_token,
            },
            token: Some(Shared::clone(token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_match_arm(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let pipe = self.next_node(|kind| matches!(kind, TokenKind::Pipe), NodeKind::Token)?;

        let pattern_leading_trivia = self.parse_leading_trivia();
        let pattern = self.parse_pattern(pattern_leading_trivia)?;

        // Check for guard (if condition)
        let (if_token, guard_args) = if let Some(token) = self.peek_non_trivia()
            && matches!(token.kind, TokenKind::If)
        {
            let if_token = self.next_node(|kind| matches!(kind, TokenKind::If), NodeKind::Token)?;
            let guard_args = self.parse_args()?;
            // A guard is a single condition.
            if guard_args.iter().filter(|arg| !arg.is_token()).count() != 1
                && let Some(token) = &if_token.token
            {
                self.report_unexpected(token);
            }
            (Some(if_token), Some(guard_args.into()))
        } else {
            (None, None)
        };

        let colon = self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?;

        let body_leading_trivia = self.parse_leading_trivia();
        let body = self.parse_expr(body_leading_trivia, false)?;

        Ok(Shared::new(Node {
            kind: NodeKind::MatchArm {
                pipe,
                pattern,
                if_token,
                guard_args,
                colon,
                body,
            },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        }))
    }

    fn parse_pattern(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let first = self.parse_single_pattern(leading_trivia)?;

        // Check for or-pattern: p1 || p2 || ...
        if !matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::Or)) {
            return Ok(first);
        }

        let mut children: Vec<Shared<Node>> = vec![first];
        while matches!(self.peek_non_trivia().map(|t| &t.kind), Some(TokenKind::Or)) {
            children.push(self.next_node(|kind| matches!(kind, TokenKind::Or), NodeKind::Token)?);
            let alt_leading_trivia = self.parse_leading_trivia();
            children.push(self.parse_single_pattern(alt_leading_trivia)?);
        }

        Ok(Shared::new(Node {
            kind: NodeKind::OrPattern { items: children.into() },
            token: None,
            leading_trivia: TriviaList::new(),
            trailing_trivia: TriviaList::new(),
        }))
    }

    fn parse_single_pattern(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let token = Shared::clone(self.peek_token()?);

        match &token.kind {
            // Wildcard pattern: _
            TokenKind::Ident(name) if name == constants::identifiers::PATTERN_MATCH_WILDCARD => {
                self.advance();
                let trailing_trivia = self.parse_trailing_trivia();
                Ok(Shared::new(Node {
                    kind: NodeKind::Pattern { items: ArgList::new() },
                    token: Some(Shared::clone(&token)),
                    leading_trivia,
                    trailing_trivia,
                }))
            }
            // Type pattern: :type_name
            TokenKind::Colon => {
                let children = vec![
                    self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?,
                    self.next_node(
                        |kind| matches!(kind, TokenKind::Ident(_)),
                        NodeKind::Ident { attr: None },
                    )?,
                ];

                Ok(Shared::new(Node {
                    kind: NodeKind::Pattern { items: children.into() },
                    token: None,
                    leading_trivia,
                    trailing_trivia: TriviaList::new(),
                }))
            }
            // Literal patterns (string, bytes, number, bool, none)
            TokenKind::StringLiteral(_)
            | TokenKind::BytesLiteral(_)
            | TokenKind::NumberLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::None => {
                self.advance();
                let trailing_trivia = self.parse_trailing_trivia();
                Ok(Shared::new(Node {
                    kind: NodeKind::Pattern { items: ArgList::new() },
                    token: Some(Shared::clone(&token)),
                    leading_trivia,
                    trailing_trivia,
                }))
            }
            // Array pattern: [pattern, pattern, ...]
            TokenKind::LBracket => self.parse_array_pattern(leading_trivia),
            // Dict pattern: {key: pattern, key}
            TokenKind::LBrace => self.parse_dict_pattern(leading_trivia),
            // Identifier pattern (binding)
            TokenKind::Ident(_) => {
                self.advance();
                let trailing_trivia = self.parse_trailing_trivia();
                Ok(Shared::new(Node {
                    kind: NodeKind::Pattern { items: ArgList::new() },
                    token: Some(Shared::clone(&token)),
                    leading_trivia,
                    trailing_trivia,
                }))
            }
            _ => {
                self.advance();
                Err(ParseError::UnexpectedToken(token))
            }
        }
    }

    fn parse_array_pattern(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        // Parse [
        children.push(self.next_node(|kind| matches!(kind, TokenKind::LBracket), NodeKind::Token)?);

        // Parse patterns
        loop {
            let leading_trivia = self.parse_leading_trivia();

            // Check for ]
            if let Some(token) = self.peek_non_trivia() {
                if matches!(token.kind, TokenKind::RBracket) {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::RBracket), NodeKind::Token)?);
                    break;
                }

                // Check for rest pattern: ..rest
                if matches!(token.kind, TokenKind::DoubleDot) {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::DoubleDot), NodeKind::Token)?);

                    // Parse rest identifier
                    let rest_leading_trivia = self.parse_leading_trivia();
                    if let Some(rest) = self.peek_non_trivia()
                        && !matches!(rest.kind, TokenKind::Ident(_))
                    {
                        return Err(ParseError::UnexpectedToken(Shared::clone(rest)));
                    }
                    children.push(self.parse_pattern(rest_leading_trivia)?);

                    // Expect ]
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::RBracket), NodeKind::Token)?);
                    break;
                }
            }

            // Parse pattern
            children.push(self.parse_pattern(leading_trivia)?);

            // Check for comma or ]
            if let Some(token) = self.peek_non_trivia() {
                if matches!(token.kind, TokenKind::Comma) {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?);
                } else if matches!(token.kind, TokenKind::RBracket) {
                    // Will be consumed in next iteration
                    continue;
                } else {
                    return Err(ParseError::UnexpectedToken(Shared::clone(token)));
                }
            }
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Pattern { items: children.into() },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        }))
    }

    fn parse_dict_pattern(&mut self, leading_trivia: TriviaList) -> Result<Shared<Node>, ParseError> {
        let mut children: Vec<Shared<Node>> = Vec::with_capacity(10);

        // Parse {
        children.push(self.next_node(|kind| matches!(kind, TokenKind::LBrace), NodeKind::Token)?);

        // Parse fields
        loop {
            let _leading_trivia = self.parse_leading_trivia();

            // Check for }
            if let Some(token) = self.peek_non_trivia()
                && matches!(token.kind, TokenKind::RBrace)
            {
                children.push(self.next_node(|kind| matches!(kind, TokenKind::RBrace), NodeKind::Token)?);
                break;
            }

            // Parse key (identifier)
            children.push(self.next_node(
                |kind| matches!(kind, TokenKind::Ident(_)),
                NodeKind::Ident { attr: None },
            )?);

            // Check for colon (key: pattern) or just key
            if let Some(token) = self.peek_non_trivia()
                && matches!(token.kind, TokenKind::Colon)
            {
                children.push(self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?);

                let pattern_leading_trivia = self.parse_leading_trivia();
                children.push(self.parse_pattern(pattern_leading_trivia)?);
            }

            // Check for comma or }
            if let Some(token) = self.peek_non_trivia() {
                if matches!(token.kind, TokenKind::Comma) {
                    children.push(self.next_node(|kind| matches!(kind, TokenKind::Comma), NodeKind::Token)?);
                } else if matches!(token.kind, TokenKind::RBrace) {
                    // Will be consumed in next iteration
                    continue;
                } else {
                    return Err(ParseError::UnexpectedToken(Shared::clone(token)));
                }
            }
        }

        Ok(Shared::new(Node {
            kind: NodeKind::Pattern { items: children.into() },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        }))
    }

    fn parse_unary_op(&mut self, leading_trivia: TriviaList, root: bool) -> Result<Shared<Node>, ParseError> {
        let operator_token = self.advance_or_eof()?;
        let trailing_trivia = self.parse_trailing_trivia();

        let op = match &operator_token.kind {
            TokenKind::Not => UnaryOp::Not,
            TokenKind::Minus => UnaryOp::Negate,
            _ => return Err(ParseError::UnexpectedToken(Shared::clone(operator_token))),
        };

        // Parse the operand expression
        let operand_leading_trivia = self.parse_leading_trivia();
        if let Some(next) = self.peek()
            && !matches!(next.kind, TokenKind::Eof)
            && !crate::AstParser::is_prefix_operand_start(&next.kind)
        {
            return Err(ParseError::UnexpectedToken(Shared::clone(next)));
        }
        let operand = self.parse_primary_expr(operand_leading_trivia, root)?;

        Ok(Shared::new(Node {
            kind: NodeKind::UnaryOp { op, operand },
            token: Some(Shared::clone(operator_token)),
            leading_trivia,
            trailing_trivia,
        }))
    }

    fn parse_params(&mut self) -> Result<Vec<Shared<Node>>, ParseError> {
        let mut nodes: Vec<Shared<Node>> = Vec::with_capacity(4);

        nodes.push(self.next_node(|token_kind| matches!(token_kind, TokenKind::LParen), NodeKind::Token)?);

        self.parse_separated_list(
            &mut nodes,
            |kind| matches!(kind, TokenKind::RParen),
            "`)`",
            |parser| parser.parse_param(),
        );

        let (mut seen_variadic, mut seen_default) = (false, false);
        for param in &nodes {
            let NodeKind::Param { asterisk, eq_token, .. } = &param.kind else {
                continue;
            };
            let Some(name) = &param.token else {
                continue;
            };

            if let Some(asterisk) = asterisk {
                // A variadic parameter takes no default value.
                if let Some(eq_token) = eq_token
                    && let Some(token) = &eq_token.token
                {
                    self.report_unexpected(token);
                }
                if seen_variadic {
                    let token = asterisk.token.as_ref().unwrap_or(name);
                    self.errors
                        .report(ParseError::MultipleVariadicParameters(Shared::clone(token)));
                }
                seen_variadic = true;
            } else if seen_variadic {
                self.errors
                    .report(ParseError::VariadicParameterMustBeLast(Shared::clone(name)));
            } else if eq_token.is_some() {
                seen_default = true;
            } else if seen_default {
                self.errors
                    .report(ParseError::ParameterWithoutDefaultAfterDefault(Shared::clone(name)));
            }
        }

        Ok(nodes)
    }

    fn parse_param(&mut self) -> Result<Shared<Node>, ParseError> {
        let leading_trivia = self.parse_leading_trivia();

        // Variadic parameter: *name
        let asterisk = if self.peek().is_some_and(|t| matches!(t.kind, TokenKind::Asterisk)) {
            let token = self.advance_or_eof()?;
            let inner_leading_trivia = self.parse_leading_trivia();
            Some(Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(token)),
                leading_trivia: inner_leading_trivia,
                trailing_trivia: TriviaList::new(),
            }))
        } else {
            None
        };

        let name_token = match self.peek() {
            Some(token) if matches!(token.kind, TokenKind::Ident(_)) => Shared::clone(self.advance_or_eof()?),
            Some(token) => return Err(ParseError::UnexpectedToken(Shared::clone(token))),
            None => return Err(ParseError::UnexpectedEOFDetected),
        };
        let mut trailing_trivia = self.parse_trailing_trivia();

        let (eq_token, default) = if self.try_next_token(|kind| matches!(kind, TokenKind::Equal)) {
            // Keep the whitespace between the name and `=` so the tree stays lossless.
            let mut eq_leading_trivia = std::mem::take(&mut trailing_trivia);
            eq_leading_trivia.extend(self.parse_leading_trivia());
            let equal_token = self.advance_or_eof()?;
            let eq_trailing_trivia = self.parse_trailing_trivia();
            let eq_token = Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(equal_token)),
                leading_trivia: eq_leading_trivia,
                trailing_trivia: eq_trailing_trivia,
            });

            let default_leading_trivia = self.parse_leading_trivia();
            let default_value = self.parse_expr(default_leading_trivia, false)?;
            trailing_trivia = self.parse_trailing_trivia();
            (Some(eq_token), Some(default_value))
        } else {
            (None, None)
        };

        Ok(Shared::new(Node {
            kind: NodeKind::Param {
                asterisk,
                eq_token,
                default,
            },
            token: Some(name_token),
            leading_trivia,
            trailing_trivia,
        }))
    }

    /// Parses a comma-separated list of items enclosed by the given closing token.
    /// The opening token must already be consumed. Handles empty lists, trailing commas
    /// in the form of early close, and produces Token nodes for commas and the closing delimiter.
    ///
    /// Never fails: a malformed item becomes an `Error` node and an absent closing
    /// delimiter becomes a `Missing` node, both reported to `self.errors`.
    fn parse_separated_list(
        &mut self,
        nodes: &mut Vec<Shared<Node>>,
        close_token: fn(&TokenKind) -> bool,
        close_label: &'static str,
        mut parse_item: impl FnMut(&mut Self) -> Result<Shared<Node>, ParseError>,
    ) {
        let is_terminator = |kind: &TokenKind| {
            matches!(
                kind,
                TokenKind::Eof
                    | TokenKind::SemiColon
                    | TokenKind::End
                    | TokenKind::RParen
                    | TokenKind::RBracket
                    | TokenKind::RBrace
            )
        };

        loop {
            // Check for close (empty list, or trailing comma then close)
            if self.try_next_token(close_token) {
                let leading_trivia = self.parse_leading_trivia();
                if let Some(close) = self.advance() {
                    let trailing_trivia = self.parse_trailing_trivia();
                    nodes.push(Shared::new(Node {
                        kind: NodeKind::Token,
                        token: Some(Shared::clone(close)),
                        leading_trivia,
                        trailing_trivia,
                    }));
                }
                return;
            }

            let item_start = self.pos;
            let item_node = match parse_item(self) {
                Ok(node) => Some(node),
                Err(e) => {
                    self.errors.report(e);
                    self.recover_error(item_start, TriviaList::new(), &|kind, _| {
                        matches!(kind, TokenKind::Comma) || is_terminator(kind)
                    })
                }
            };
            match item_node {
                Some(node) => nodes.push(node),
                // Nothing was consumed: the item is absent before the separator.
                None if self
                    .peek_non_trivia()
                    .is_some_and(|t| matches!(t.kind, TokenKind::Comma)) =>
                {
                    let leading_trivia = self.parse_leading_trivia();
                    let missing = self.missing_node("an item", TriviaList::new());
                    self.pos -= leading_trivia.len();
                    nodes.push(missing);
                }
                None => {}
            }

            loop {
                let leading_trivia = self.parse_leading_trivia();
                match self.peek() {
                    Some(token) if matches!(token.kind, TokenKind::Comma) || close_token(&token.kind) => {
                        let is_close = close_token(&token.kind);
                        self.pos += 1;
                        let trailing_trivia = self.parse_trailing_trivia();
                        nodes.push(Shared::new(Node {
                            kind: NodeKind::Token,
                            token: Some(Shared::clone(token)),
                            leading_trivia,
                            trailing_trivia,
                        }));
                        if is_close {
                            return;
                        }
                        break;
                    }
                    Some(token) if !is_terminator(&token.kind) => {
                        // Junk between items: keep it and resume at the next separator.
                        self.errors.report(ParseError::Missing {
                            expected: close_label,
                            found: Shared::clone(token),
                        });
                        let start = self.pos;
                        nodes.extend(self.recover_error(start, leading_trivia, &|kind, _| {
                            matches!(kind, TokenKind::Comma) || is_terminator(kind)
                        }));
                    }
                    _ => {
                        nodes.push(self.missing_node(close_label, leading_trivia));
                        return;
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn peek_token(&self) -> Result<&'a Shared<Token>, ParseError> {
        self.peek().ok_or(ParseError::UnexpectedEOFDetected)
    }

    #[inline]
    fn parse_leading_trivia(&mut self) -> TriviaList {
        let mut trivia = TriviaList::new();

        while let Some(token) = self.tokens.get(self.pos) {
            match &token.kind {
                TokenKind::Whitespace(_) => trivia.push(Trivia::Whitespace(Shared::clone(token))),
                TokenKind::Tab(_) => trivia.push(Trivia::Tab(Shared::clone(token))),
                TokenKind::Comment(_) => trivia.push(Trivia::Comment(Shared::clone(token))),
                TokenKind::NewLine => trivia.push(Trivia::NewLine),
                _ => break,
            };
            self.pos += 1;
        }

        trivia
    }

    #[inline]
    fn try_next_token(&mut self, match_token_kind: fn(&TokenKind) -> bool) -> bool {
        let saved_pos = self.pos;
        // Skip trivia
        while let Some(token) = self.tokens.get(self.pos) {
            if token.kind.is_trivia() {
                self.pos += 1;
            } else {
                break;
            }
        }
        let result = self
            .tokens
            .get(self.pos)
            .map(|t| match_token_kind(&t.kind))
            .unwrap_or(false);
        self.pos = saved_pos;
        result
    }

    /// Returns true if the token immediately following the next `:` is an identifier or string
    /// literal, indicating a dict key access using a symbol (e.g., `dict[:key]`).
    fn is_symbol_after_colon(&mut self) -> bool {
        let saved_pos = self.pos;
        // Skip leading trivia to reach ':'
        while let Some(token) = self.tokens.get(self.pos) {
            if token.kind.is_trivia() {
                self.pos += 1;
            } else {
                break;
            }
        }
        // Consume the ':' itself
        if matches!(self.tokens.get(self.pos).map(|t| &t.kind), Some(TokenKind::Colon)) {
            self.pos += 1;
        } else {
            self.pos = saved_pos;
            return false;
        }
        // Skip trivia after ':'
        while let Some(token) = self.tokens.get(self.pos) {
            if token.kind.is_trivia() {
                self.pos += 1;
            } else {
                break;
            }
        }
        let result = self
            .tokens
            .get(self.pos)
            .is_some_and(|t| matches!(t.kind, TokenKind::Ident(_) | TokenKind::StringLiteral(_)));
        self.pos = saved_pos;
        result
    }

    #[inline]
    fn parse_trailing_trivia(&mut self) -> TriviaList {
        let mut trivia = TriviaList::new();

        while let Some(token) = self.tokens.get(self.pos) {
            match &token.kind {
                TokenKind::Whitespace(_) => trivia.push(Trivia::Whitespace(Shared::clone(token))),
                TokenKind::Tab(_) => trivia.push(Trivia::Tab(Shared::clone(token))),
                _ => break,
            }
            self.pos += 1;
        }

        trivia
    }

    /// Whether `kind` ends a statement during error recovery. Closers and commas end it only
    /// inside nested programs, where the enclosing construct owns them.
    fn is_statement_boundary(kind: &TokenKind, has_children: bool, root: bool) -> bool {
        match kind {
            TokenKind::Pipe | TokenKind::SemiColon | TokenKind::End => true,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace | TokenKind::Comma => !root,
            TokenKind::Def
            | TokenKind::Module
            | TokenKind::Let
            | TokenKind::Var
            | TokenKind::Include
            | TokenKind::Import => has_children,
            _ => false,
        }
    }

    /// Whether a construct ends with its own `;`/`end` (or is a directive), so a statement may
    /// follow it without a `|`. Keep in sync with `expect_statement_end` in the AST parser.
    fn closes_itself(node: &Node) -> bool {
        match &node.kind {
            // Selector chains such as `.a.b` are also `Block`s, so only a `do` block counts.
            NodeKind::Block { .. } => node.token.as_ref().is_some_and(|t| matches!(t.kind, TokenKind::Do)),
            kind => matches!(
                kind,
                NodeKind::Def { .. }
                    | NodeKind::Fn { .. }
                    | NodeKind::Foreach { .. }
                    | NodeKind::While { .. }
                    | NodeKind::Until { .. }
                    | NodeKind::Loop { .. }
                    | NodeKind::Match { .. }
                    | NodeKind::Module { .. }
                    | NodeKind::Import { .. }
                    | NodeKind::Include { .. }
            ),
        }
    }

    /// Whether `token` is an attribute selector such as `.url` or `.level`.
    fn is_attribute_token(token: &Token) -> bool {
        token.is_selector() && Selector::try_from(token).is_ok_and(|selector| selector.is_attribute_selector())
    }

    /// The first token of `node`, looking into its children when it has none of its own.
    fn first_token(node: &Node) -> Option<Shared<Token>> {
        node.token
            .clone()
            .or_else(|| node.children().find_map(|child| Self::first_token(child)))
    }

    /// Whether `node` ends with an attribute access such as `x.url`, `x[0].row` or `.level`.
    fn ends_with_attribute(node: &Node) -> bool {
        let is_attribute = |n: &Shared<Node>| n.token.as_deref().is_some_and(Self::is_attribute_token);

        match &node.kind {
            NodeKind::Ident { attr } | NodeKind::Self_ { attr } => attr.is_some(),
            NodeKind::SelfAttr => true,
            NodeKind::Call { args } | NodeKind::SelectorCall { args } => args.last().is_some_and(is_attribute),
            NodeKind::Selector { items } => items.last().is_some_and(is_attribute),
            NodeKind::Block { program } => program.last().is_some_and(|last| Self::ends_with_attribute(last)),
            // `|=` binds to the last operand: `.a + b.url |= v` sets `b.url`.
            NodeKind::BinaryOp { rhs, .. } | NodeKind::Assign { rhs, .. } => Self::ends_with_attribute(rhs),
            NodeKind::UnaryOp { operand, .. } => Self::ends_with_attribute(operand),
            _ => false,
        }
    }

    /// Whether `node` can be assigned to: a variable or an indexed variable such as `x[i][j]`.
    fn is_assignable(node: &Node) -> bool {
        let is_token = |n: &Shared<Node>, kind: TokenKind| n.token.as_ref().is_some_and(|t| t.kind == kind);

        match &node.kind {
            NodeKind::Ident { attr } => attr.is_none(),
            NodeKind::Call { args } => {
                let is_ident = node
                    .token
                    .as_ref()
                    .is_some_and(|t| matches!(t.kind, TokenKind::Ident(_)));
                let is_index = |chunk: &[Shared<Node>]| {
                    is_token(&chunk[0], TokenKind::LBracket)
                        && !chunk[1].is_token()
                        && is_token(&chunk[2], TokenKind::RBracket)
                };
                // `x[i][j]`
                let is_index_chain =
                    is_ident && !args.is_empty() && args.len() % 3 == 0 && args.chunks(3).all(is_index);
                // `get(x, i)`, optionally followed by more indexes, is the same as `x[i]`.
                let is_get_call = node
                    .token
                    .as_ref()
                    .is_some_and(|t| matches!(&t.kind, TokenKind::Ident(name) if name == "get"))
                    && args.len() >= 5
                    && (args.len() - 5) % 3 == 0
                    && args[5..].chunks(3).all(is_index)
                    && is_token(&args[0], TokenKind::LParen)
                    && is_token(&args[2], TokenKind::Comma)
                    && is_token(&args[4], TokenKind::RParen)
                    && Self::is_assignable(&args[1]);
                is_index_chain || is_get_call
            }
            _ => false,
        }
    }

    /// Whether the last expression inside `node` is a call, which a following `do` block extends.
    fn ends_with_call(node: &Node) -> bool {
        match &node.kind {
            NodeKind::Call { .. } => true,
            NodeKind::If {
                then_branch,
                elifs,
                else_branch,
                ..
            } => else_branch
                .as_ref()
                .or(elifs.last())
                .map_or_else(|| Self::ends_with_call(then_branch), |last| Self::ends_with_call(last)),
            NodeKind::Unless { then_branch, .. }
            | NodeKind::Elif { then_branch, .. }
            | NodeKind::Else { then_branch, .. } => Self::ends_with_call(then_branch),
            NodeKind::Let { rhs, .. }
            | NodeKind::Var { rhs, .. }
            | NodeKind::Assign { rhs, .. }
            | NodeKind::BinaryOp { rhs, .. } => Self::ends_with_call(rhs),
            NodeKind::UnaryOp { operand, .. } => Self::ends_with_call(operand),
            _ => false,
        }
    }

    /// Whether a token can name the node type or key that follows `..`.
    fn is_recursive_name(kind: &TokenKind) -> bool {
        match kind {
            TokenKind::StringLiteral(_) => true,
            TokenKind::Ident(name) => Selector::from_selector_str(&format!(".{name}"))
                .is_some_and(|selector| !selector.is_attribute_selector()),
            _ => false,
        }
    }

    /// Handles a statement that starts right after `previous`. Only constructs that close themselves
    /// may be followed by a statement without a `|`, unless the AST parser reads both as one expression:
    /// a selector after an open selector chain, or the `do` block of a call. Reports an error otherwise.
    /// Returns whether `next` continues a selector chain.
    fn check_adjacent_statement(
        &mut self,
        previous: Option<&Shared<Node>>,
        next: &Shared<Token>,
        needs_separator: bool,
        after_chain_continuation: bool,
    ) -> bool {
        let continues_chain = next.is_selector()
            && !matches!(&next.kind, TokenKind::Selector(s) if s == ".")
            && previous.is_some_and(|node| Self::continues_selector_chain(node, after_chain_continuation));
        let trailing_do_block =
            matches!(next.kind, TokenKind::Do) && previous.is_some_and(|node| Self::ends_with_call(node));

        if needs_separator && !continues_chain && !trailing_do_block {
            self.report_unexpected(next);
        }
        continues_chain
    }

    /// Whether `node` is a selector chain that a following selector extends. A trailing attribute
    /// selector (`.h.level`) ends the chain.
    fn continues_selector_chain(node: &Node, is_continuation: bool) -> bool {
        let is_attribute = |n: &Shared<Node>| n.token.as_deref().is_some_and(Self::is_attribute_token);

        match &node.kind {
            // A bare `.` starts a chain: `. .li`.
            NodeKind::Self_ { attr } => attr.is_none(),
            // `.value` starts a chain, but as the tail of one it ends it.
            NodeKind::SelfAttr => !is_continuation,
            NodeKind::Selector { items } => {
                // A list selector (`.[0]`) takes no tail, and an attribute (`.code.lang`) ends the chain.
                let is_list = node
                    .token
                    .as_ref()
                    .is_some_and(|t| matches!(&t.kind, TokenKind::Selector(s) if s == "."));
                !is_list
                    && !items.last().is_some_and(is_attribute)
                    && !node.token.as_deref().is_some_and(Self::is_attribute_token)
            }
            NodeKind::SelectorCall { args } => !args.last().is_some_and(is_attribute),
            NodeKind::Block { program } if !node.token.as_ref().is_some_and(|t| matches!(t.kind, TokenKind::Do)) => {
                // Anything after the first element is the tail of the chain.
                program
                    .last()
                    .is_some_and(|last| Self::continues_selector_chain(last, is_continuation || program.len() > 1))
            }
            // The chain belongs to the last operand: `.a - .b .c`.
            NodeKind::BinaryOp { rhs, .. } | NodeKind::Assign { rhs, .. } => {
                Self::continues_selector_chain(rhs, is_continuation)
            }
            NodeKind::UnaryOp { operand, .. } => Self::continues_selector_chain(operand, is_continuation),
            _ => false,
        }
    }

    fn starts_expression(kind: &TokenKind, root: bool) -> bool {
        match kind {
            TokenKind::Nodes => root,
            TokenKind::Break | TokenKind::Continue => true,
            TokenKind::Def
            | TokenKind::Do
            | TokenKind::Fn
            | TokenKind::Arrow
            | TokenKind::If
            | TokenKind::Foreach
            | TokenKind::Include
            | TokenKind::Import
            | TokenKind::Module
            | TokenKind::While
            | TokenKind::Loop
            | TokenKind::Until
            | TokenKind::Unless
            | TokenKind::Try
            | TokenKind::Match
            | TokenKind::Ident(_)
            | TokenKind::Self_
            | TokenKind::Let
            | TokenKind::Var
            | TokenKind::Selector(_)
            | TokenKind::DoubleDot
            | TokenKind::StringLiteral(_)
            | TokenKind::BytesLiteral(_)
            | TokenKind::NumberLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::None
            | TokenKind::InterpolatedString(_)
            | TokenKind::LBracket
            | TokenKind::LBrace
            | TokenKind::LParen
            | TokenKind::Env(_)
            | TokenKind::Not
            | TokenKind::Minus
            | TokenKind::Yield
            | TokenKind::Colon => true,
            _ => false,
        }
    }

    /// Records an error for `token`, which is not allowed where it is.
    fn report_unexpected(&mut self, token: &Shared<Token>) {
        self.errors.report(ParseError::UnexpectedToken(Shared::clone(token)));
    }

    /// The first non-trivia token at or after the current position.
    fn peek_non_trivia(&self) -> Option<&'a Shared<Token>> {
        self.tokens[self.pos..].iter().find(|token| !token.kind.is_trivia())
    }

    /// Rewinds to `start` and wraps tokens in an `Error` node until `stop_at` matches at
    /// bracket depth 0 (the stop token is left unconsumed) or the input ends. `stop_at`
    /// also receives whether the node already holds a token. `leading_trivia` is the trivia
    /// already read just before `start`. Returns `None` if nothing was consumed, leaving
    /// that trivia unread.
    fn recover_error(
        &mut self,
        start: usize,
        leading_trivia: TriviaList,
        stop_at: &dyn Fn(&TokenKind, bool) -> bool,
    ) -> Option<Shared<Node>> {
        self.pos = start;
        let mut children = ArgList::new();
        let mut depth = 0usize;

        while let Some(token) = self.peek_non_trivia() {
            if matches!(token.kind, TokenKind::Eof) || (depth == 0 && stop_at(&token.kind, !children.is_empty())) {
                break;
            }

            match token.kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth = depth.saturating_sub(1),
                _ => {}
            }

            let leading_trivia = self.parse_leading_trivia();
            self.pos += 1;
            let trailing_trivia = self.parse_trailing_trivia();
            children.push(Shared::new(Node {
                kind: NodeKind::Token,
                token: Some(Shared::clone(token)),
                leading_trivia,
                trailing_trivia,
            }));
        }

        if children.is_empty() {
            // Give the already-read leading trivia back to the caller (one token per piece).
            self.pos = start - leading_trivia.len();
            return None;
        }

        Some(Shared::new(Node {
            kind: NodeKind::Error { children },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        }))
    }

    /// Reports `expected` as absent and returns a zero-width `Missing` node.
    fn missing_node(&mut self, expected: &'static str, leading_trivia: TriviaList) -> Shared<Node> {
        match self.peek() {
            // One diagnostic per token: the token may already have been reported as unexpected.
            Some(token) if self.errors.has_error_at(&token.range) => {}
            Some(token) => self.errors.report(ParseError::Missing {
                expected,
                found: Shared::clone(token),
            }),
            None => self.errors.report(ParseError::UnexpectedEOFDetected),
        }

        let position = self.peek().map(|token| token.range.start).unwrap_or_default();

        Shared::new(Node {
            kind: NodeKind::Missing { expected, position },
            token: None,
            leading_trivia,
            trailing_trivia: TriviaList::new(),
        })
    }

    /// Like [`next_node`](Self::next_node), but yields a `Missing` node instead of failing.
    fn expect_node(
        &mut self,
        expected_token: fn(&TokenKind) -> bool,
        node_kind: NodeKind,
        label: &'static str,
    ) -> Shared<Node> {
        let leading_trivia = self.parse_leading_trivia();
        match self.peek() {
            Some(token) if expected_token(&token.kind) => {
                self.pos += 1;
                let trailing_trivia = self.parse_trailing_trivia();
                Shared::new(Node {
                    kind: node_kind,
                    token: Some(Shared::clone(token)),
                    leading_trivia,
                    trailing_trivia,
                })
            }
            _ => self.missing_node(label, leading_trivia),
        }
    }

    #[inline]
    fn next_token(&mut self, match_token_kind: fn(&TokenKind) -> bool) -> Result<Shared<Token>, ParseError> {
        let token = self.peek().ok_or(ParseError::UnexpectedEOFDetected)?;

        if match_token_kind(&token.kind) {
            let token = Shared::clone(token);
            self.advance();
            Ok(token)
        } else {
            Err(ParseError::UnexpectedToken(Shared::clone(token)))
        }
    }

    #[inline]
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
        }))
    }

    #[inline]
    fn parse_colon_token_if_present(&mut self) -> Result<Option<Shared<Node>>, ParseError> {
        if self.try_next_token(|kind| matches!(kind, TokenKind::Colon)) {
            Ok(Some(
                self.next_node(|kind| matches!(kind, TokenKind::Colon), NodeKind::Token)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn parse_colon_or_do_token_if_present(&mut self) -> Result<Option<Shared<Node>>, ParseError> {
        if self.try_next_token(|kind| matches!(kind, TokenKind::Do)) {
            Ok(Some(
                self.next_node(|kind| matches!(kind, TokenKind::Do), NodeKind::Do)?,
            ))
        } else {
            self.parse_colon_token_if_present()
        }
    }
}

#[cfg(test)]
mod tests;
