use crate::arena::Arena;
use crate::ast::node::IdentWithToken;
use crate::eval::module::ModuleId;
use crate::lexer::token::{Token, TokenKind};
use crate::{Shared, SharedCell, get_token, token_alloc};
use smallvec::{SmallVec, smallvec};
use smol_str::SmolStr;
use std::iter::Peekable;

use super::constants;
use super::error::ParseError;
use super::node::{Args, Branches, Expr, Literal, Node, Selector};
use super::{Program, TokenId};

type IfExpr = (Option<Shared<Node>>, Shared<Node>);

#[derive(Debug)]
struct ArrayIndex(Option<usize>);

pub struct Parser<'a> {
    tokens: Peekable<core::slice::Iter<'a, Shared<Token>>>,
    token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
    module_id: ModuleId,
}

impl<'a> Parser<'a> {
    pub fn new(
        tokens: core::slice::Iter<'a, Shared<Token>>,
        token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
        module_id: ModuleId,
    ) -> Self {
        Self {
            tokens: tokens.peekable(),
            token_arena,
            module_id,
        }
    }

    pub fn parse(&mut self) -> Result<Program, ParseError> {
        self.parse_program(true)
    }

    fn parse_program(&mut self, root: bool) -> Result<Program, ParseError> {
        let mut asts = Vec::with_capacity(1_000);

        // Initial check for invalid starting tokens in a program.
        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::Pipe | TokenKind::SemiColon | TokenKind::End => {
                    return Err(ParseError::UnexpectedToken((***token).clone()));
                }
                _ => {}
            },
            None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        };

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::Pipe | TokenKind::Comment(_) => continue, // Skip pipes and comments.
                TokenKind::Eof => break, // End of file terminates the program.
                TokenKind::SemiColon | TokenKind::End => {
                    // Semicolons and 'end' keyword terminate sub-programs (e.g., in 'def', 'fn').
                    // In the root program, a semicolon/end is only allowed if followed by EOF or a comment then EOF.
                    // Otherwise, it's an unexpected EOF because more expressions were expected.
                    if root {
                        if let Some(token) = self.tokens.peek() {
                            if let TokenKind::Eof = &token.kind {
                                break;
                            } else if let TokenKind::Comment(_) = &token.kind {
                                // Allow comments before EOF after a semicolon/end
                                let _ = self.tokens.next(); // Consume comment
                                if matches!(
                                    self.tokens.peek().map(|t| &t.kind),
                                    Some(TokenKind::Eof) | None
                                ) {
                                    break;
                                } else {
                                    return Err(ParseError::UnexpectedEOFDetected(self.module_id));
                                }
                            } else {
                                return Err(ParseError::UnexpectedEOFDetected(self.module_id));
                            }
                        }
                    }
                    // For non-root programs (e.g. function bodies), a semicolon/end explicitly ends the program.
                    break;
                }
                TokenKind::Nodes if root => {
                    let ast = self.parse_all_nodes(Shared::clone(token))?;
                    asts.push(ast);
                }
                TokenKind::Nodes => {
                    return Err(ParseError::UnexpectedToken((**token).clone()));
                }
                TokenKind::NewLine | TokenKind::Tab(_) | TokenKind::Whitespace(_) => unreachable!(),
                _ => {
                    let ast = self.parse_expr(Shared::clone(token))?;
                    asts.push(ast);
                }
            }
        }

        if asts.is_empty() {
            return Err(ParseError::UnexpectedEOFDetected(self.module_id));
        }

        Ok(asts)
    }

    #[inline(always)]
    fn parse_expr(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        self.parse_equality_expr(token)
    }

    #[inline(always)]
    fn binary_op_precedence(kind: &TokenKind) -> u8 {
        match kind {
            TokenKind::Or => 1,
            TokenKind::And => 2,
            TokenKind::EqEq
            | TokenKind::NeEq
            | TokenKind::Gt
            | TokenKind::Gte
            | TokenKind::Lt
            | TokenKind::Lte => 3,
            TokenKind::Plus | TokenKind::Minus => 4,
            TokenKind::Asterisk | TokenKind::Slash | TokenKind::Percent => 5,
            TokenKind::RangeOp => 6,
            _ => 0,
        }
    }

    #[inline(always)]
    fn binary_op_function_name(kind: &TokenKind) -> &'static str {
        match kind {
            TokenKind::And => constants::AND,
            TokenKind::Asterisk => constants::MUL,
            TokenKind::EqEq => constants::EQ,
            TokenKind::Gte => constants::GTE,
            TokenKind::Gt => constants::GT,
            TokenKind::Lte => constants::LTE,
            TokenKind::Lt => constants::LT,
            TokenKind::Minus => constants::SUB,
            TokenKind::NeEq => constants::NE,
            TokenKind::Or => constants::OR,
            TokenKind::Percent => constants::MOD,
            TokenKind::Plus => constants::ADD,
            TokenKind::RangeOp => constants::RANGE,
            TokenKind::Slash => constants::DIV,
            _ => unreachable!(),
        }
    }

    fn parse_binary_op(
        parser: &mut Parser,
        min_prec: u8,
        mut lhs: Shared<Node>,
    ) -> Result<Shared<Node>, ParseError> {
        while let Some(peeked_token_rc) = parser.tokens.peek() {
            let kind = &peeked_token_rc.kind;
            if !Self::is_binary_op(kind) {
                break;
            }

            let prec = Self::binary_op_precedence(kind);

            if prec < min_prec {
                break;
            }

            let operator_token = parser.tokens.next().unwrap();
            let operator_token_id = token_alloc(&parser.token_arena, operator_token);
            let rhs_token = match parser.tokens.next() {
                Some(t) => t,
                None => return Err(ParseError::UnexpectedEOFDetected(parser.module_id)),
            };
            let mut rhs = parser.parse_primary_expr(Shared::clone(rhs_token))?;

            loop {
                let next_prec = if let Some(next_token) = parser.tokens.peek() {
                    if Self::is_binary_op(&next_token.kind) {
                        Self::binary_op_precedence(&next_token.kind)
                    } else {
                        0
                    }
                } else {
                    0
                };
                if next_prec > prec {
                    rhs = Self::parse_binary_op(parser, next_prec, rhs)?;
                } else {
                    break;
                }
            }

            lhs = Shared::new(Node {
                token_id: operator_token_id,
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(
                        Self::binary_op_function_name(kind),
                        Some(Shared::clone(operator_token)),
                    ),
                    smallvec![lhs, rhs],
                )),
            });
        }
        Ok(lhs)
    }

    fn parse_equality_expr(
        &mut self,
        initial_token: Shared<Token>,
    ) -> Result<Shared<Node>, ParseError> {
        let lhs = self.parse_primary_expr(initial_token)?;
        Self::parse_binary_op(self, 1, lhs)
    }

    fn parse_primary_expr(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        match &token.kind {
            TokenKind::Selector(_) => self.parse_selector(token),
            // Delegate parsing of 'let' expressions.
            TokenKind::Let => self.parse_expr_let(Shared::clone(&token)),
            // Delegate parsing of 'def' (function definition) expressions.
            TokenKind::Def => self.parse_expr_def(Shared::clone(&token)),
            // Delegate parsing of 'do' (block) expressions.
            TokenKind::Do => self.parse_expr_block(Shared::clone(&token)),
            TokenKind::Fn => self.parse_fn(token),
            TokenKind::While => self.parse_while(token),
            TokenKind::Until => self.parse_until(token),
            TokenKind::Foreach => self.parse_foreach(token),
            // Delegate parsing of 'if' expressions.
            TokenKind::If => self.parse_expr_if(Shared::clone(&token)),
            TokenKind::InterpolatedString(_) => self.parse_interpolated_string(token),
            TokenKind::Include => self.parse_include(token),
            TokenKind::Self_ => self.parse_self(token),
            TokenKind::Break => self.parse_break(token),
            TokenKind::Continue => self.parse_continue(token),
            TokenKind::Ident(name) => self.parse_ident(name, Shared::clone(&token)),
            TokenKind::BoolLiteral(_) => self.parse_literal(token),
            TokenKind::StringLiteral(_) => self.parse_literal(token),
            TokenKind::NumberLiteral(_) => self.parse_literal(token),
            TokenKind::LBracket => self.parse_array(token),
            TokenKind::LBrace => self.parse_dict(token),
            TokenKind::LParen => self.parse_paren(token),
            TokenKind::Not => self.parse_not(token),
            TokenKind::Minus => self.parse_negate(token),
            TokenKind::Env(_) => self.parse_env(token),
            TokenKind::None => self.parse_literal(token),
            TokenKind::Eof => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(ParseError::UnexpectedToken((*token).clone())),
        }
    }

    fn parse_paren(&mut self, lparen_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &lparen_token);
        let expr_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        };

        let expr_node = self.parse_expr(Shared::clone(expr_token))?;

        self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::RParen)
        })?;

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Paren(expr_node)),
        }))
    }

    fn parse_not(&mut self, not_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &not_token);

        let expr_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        };

        let expr_node = self.parse_primary_expr(Shared::clone(expr_token))?;

        // Convert ! to not() function call
        let not_ident =
            IdentWithToken::new_with_token(constants::NOT, Some(Shared::clone(&not_token)));
        let args = smallvec![expr_node];

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Call(not_ident, args)),
        }))
    }

    fn parse_negate(&mut self, minus_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &minus_token);

        let expr_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        };

        let expr_node = self.parse_primary_expr(Shared::clone(expr_token))?;

        let negate_ident =
            IdentWithToken::new_with_token(constants::NEGATE, Some(Shared::clone(&minus_token)));
        let args = smallvec![expr_node];

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Call(negate_ident, args)),
        }))
    }

    fn parse_dict(&mut self, lbrace_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &lbrace_token);
        let mut pairs = SmallVec::new();

        loop {
            match self.tokens.peek() {
                Some(token) if token.kind == TokenKind::RBrace => {
                    self.tokens.next();
                    break;
                }
                None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
                _ => {}
            }

            // Parse key
            let key_token = match self.tokens.next() {
                Some(t) => t,
                None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            };

            let key_node = match &key_token.kind {
                TokenKind::Ident(name) => Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, key_token),
                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(
                        name,
                        Some(Shared::clone(key_token)),
                    ))),
                }),
                TokenKind::StringLiteral(s) => Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, key_token),
                    expr: Shared::new(Expr::Literal(Literal::String(s.clone()))),
                }),
                _ => {
                    return Err(ParseError::UnexpectedToken((**key_token).clone()));
                }
            };

            // Expect Colon
            match self.tokens.next() {
                Some(token) if token.kind == TokenKind::Colon => {}
                Some(token) => return Err(ParseError::UnexpectedToken((**token).clone())),
                None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            }

            // Parse value
            let value_token = match self.tokens.next() {
                Some(t) => t,
                None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            };
            let value_node = self.parse_expr(Shared::clone(value_token))?;

            pairs.push(Shared::new(Node {
                token_id,
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(
                        constants::ARRAY,
                        Some(Shared::clone(key_token)),
                    ),
                    smallvec![key_node, value_node],
                )),
            }));

            // Peek for Comma or RBrace
            match self.tokens.peek() {
                Some(token) if token.kind == TokenKind::Comma => {
                    self.tokens.next(); // Consume Comma
                    // Check for trailing comma followed by RBrace
                    if let Some(next_token) = self.tokens.peek() {
                        if next_token.kind == TokenKind::RBrace {
                            self.tokens.next(); // Consume RBrace
                            break;
                        }
                    }
                }
                Some(token) if token.kind == TokenKind::RBrace => {
                    self.tokens.next(); // Consume RBrace
                    break;
                }
                Some(token) => {
                    return Err(ParseError::ExpectedClosingBrace((***token).clone()));
                }
                None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            }
        }

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Call(
                IdentWithToken::new_with_token(constants::DICT, Some(Shared::clone(&lbrace_token))),
                pairs,
            )),
        }))
    }

    fn parse_env(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        match &token.kind {
            TokenKind::Env(s) => Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: std::env::var(s)
                    .map_err(|_| ParseError::EnvNotFound((*token).clone(), SmolStr::new(s)))
                    .map(|s| Shared::new(Expr::Literal(Literal::String(s.to_owned()))))?,
            })),
            TokenKind::Eof => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(ParseError::UnexpectedToken((*token).clone())),
        }
    }

    fn parse_self(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let node = Shared::new(Node {
            token_id: token_alloc(&self.token_arena, &token),
            expr: Shared::new(Expr::Self_),
        });

        match self.tokens.peek().map(|t| &t.kind) {
            Some(TokenKind::LBracket) => self.parse_bracket_access(node, token),
            _ => Ok(node),
        }
    }

    fn parse_break(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        Ok(Shared::new(Node {
            token_id: token_alloc(&self.token_arena, &token),
            expr: Shared::new(Expr::Break),
        }))
    }

    fn parse_continue(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        Ok(Shared::new(Node {
            token_id: token_alloc(&self.token_arena, &token),
            expr: Shared::new(Expr::Continue),
        }))
    }

    fn parse_array(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &token);
        let mut elements: SmallVec<[Shared<Node>; 4]> = SmallVec::new();

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::RBracket => break,
                TokenKind::Comma => continue,
                _ => {
                    let expr = self.parse_expr(Shared::clone(token))?;
                    elements.push(expr);
                }
            }
        }

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Call(
                IdentWithToken::new_with_token(constants::ARRAY, Some(token)),
                elements,
            )),
        }))
    }

    fn parse_all_nodes(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        Ok(Shared::new(Node {
            token_id: token_alloc(&self.token_arena, &token),
            expr: Shared::new(Expr::Nodes),
        }))
    }

    #[inline(always)]
    fn is_binary_op(token_kind: &TokenKind) -> bool {
        matches!(
            token_kind,
            TokenKind::And
                | TokenKind::Asterisk
                | TokenKind::EqEq
                | TokenKind::Gte
                | TokenKind::Gt
                | TokenKind::Lte
                | TokenKind::Lt
                | TokenKind::Minus
                | TokenKind::NeEq
                | TokenKind::Or
                | TokenKind::Percent
                | TokenKind::Plus
                | TokenKind::RangeOp
                | TokenKind::Slash
        )
    }

    #[inline(always)]
    fn is_next_token_allowed(token_kind: Option<&TokenKind>) -> bool {
        matches!(
            token_kind,
            Some(TokenKind::And)
                | Some(TokenKind::Asterisk)
                | Some(TokenKind::Colon)
                | Some(TokenKind::Comma)
                | Some(TokenKind::Comment(_))
                | Some(TokenKind::Eof)
                | Some(TokenKind::Elif)
                | Some(TokenKind::Else)
                | Some(TokenKind::EqEq)
                | Some(TokenKind::Gte)
                | Some(TokenKind::Gt)
                | Some(TokenKind::Lte)
                | Some(TokenKind::Lt)
                | Some(TokenKind::Minus)
                | Some(TokenKind::NeEq)
                | Some(TokenKind::Or)
                | Some(TokenKind::Percent)
                | Some(TokenKind::Pipe)
                | Some(TokenKind::Plus)
                | Some(TokenKind::RangeOp)
                | Some(TokenKind::RBrace)
                | Some(TokenKind::RBracket)
                | Some(TokenKind::RParen)
                | Some(TokenKind::SemiColon)
                | Some(TokenKind::End)
                | Some(TokenKind::Slash)
                | None
        )
    }

    fn parse_literal(&mut self, literal_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let literal_node = match &literal_token.kind {
            TokenKind::BoolLiteral(b) => Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &literal_token),
                expr: Shared::new(Expr::Literal(Literal::Bool(*b))),
            })),
            TokenKind::StringLiteral(s) => Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &literal_token),
                expr: Shared::new(Expr::Literal(Literal::String(s.to_owned()))),
            })),
            TokenKind::NumberLiteral(n) => Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &literal_token),
                expr: Shared::new(Expr::Literal(Literal::Number(*n))),
            })),
            TokenKind::None => Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &literal_token),
                expr: Shared::new(Expr::Literal(Literal::None)),
            })),
            TokenKind::Eof => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(ParseError::UnexpectedToken((*literal_token).clone())),
        }?;

        let token = self.tokens.peek();

        if Self::is_next_token_allowed(token.as_ref().map(|t| &t.kind)) {
            Ok(literal_node)
        } else {
            Err(ParseError::UnexpectedToken((***token.unwrap()).clone()))
        }
    }

    fn parse_ident(
        &mut self,
        ident: &str,
        ident_token: Shared<Token>,
    ) -> Result<Shared<Node>, ParseError> {
        match self.tokens.peek().map(|t| &t.kind) {
            Some(TokenKind::LParen) => {
                let args = self.parse_args()?;
                let call_node = Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &ident_token),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(ident, Some(Shared::clone(&ident_token))),
                        args,
                    )),
                });

                // Check for bracket access after function call (e.g., foo()[0])
                if matches!(
                    self.tokens.peek().map(|t| &t.kind),
                    Some(TokenKind::LBracket)
                ) {
                    self.parse_bracket_access(call_node, ident_token)
                } else if Self::is_next_token_allowed(self.tokens.peek().map(|t| &t.kind)) {
                    Ok(call_node)
                } else {
                    Err(ParseError::UnexpectedToken(
                        (***self.tokens.peek().unwrap()).clone(),
                    ))
                }
            }
            Some(TokenKind::LBracket) => {
                let ident_node = Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &ident_token),
                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(
                        ident,
                        Some(Shared::clone(&ident_token)),
                    ))),
                });

                self.parse_bracket_access(ident_node, ident_token)
            }
            token if Self::is_next_token_allowed(token) => Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &ident_token),
                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(
                    ident,
                    Some(Shared::clone(&ident_token)),
                ))),
            })),
            _ => Err(ParseError::UnexpectedToken((*ident_token).clone())),
        }
    }

    // Parses bracket access operations recursively to handle nested access like arr[0][1][2]
    fn parse_bracket_access(
        &mut self,
        target_node: Shared<Node>,
        original_token: Shared<Token>,
    ) -> Result<Shared<Node>, ParseError> {
        let _ = self.tokens.next(); // consume '['

        // Parse the first expression
        let first_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        };

        let first_node = self.parse_expr(Shared::clone(first_token))?;

        // Check if this is a slice operation (contains ':')
        let is_slice =
            matches!(self.tokens.peek(), Some(token) if matches!(token.kind, TokenKind::Colon));

        let result_node = if is_slice {
            // Consume the colon
            let _ = self.tokens.next();

            // Parse the second expression (end index)
            let second_token = match self.tokens.next() {
                Some(t) => t,
                None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            };

            let second_node = self.parse_expr(Shared::clone(second_token))?;

            // Expect closing bracket
            match self.tokens.peek() {
                Some(token) if matches!(token.kind, TokenKind::RBracket) => {
                    let _ = self.tokens.next(); // consume ']'
                }
                Some(token) => {
                    return Err(ParseError::ExpectedClosingBracket((***token).clone()));
                }
                None => {
                    return Err(ParseError::ExpectedClosingBracket(Token {
                        range: original_token.range.clone(),
                        kind: TokenKind::Eof,
                        module_id: self.module_id,
                    }));
                }
            }

            Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &original_token),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(
                        constants::SLICE,
                        Some(Shared::clone(&original_token)),
                    ),
                    smallvec![target_node, first_node, second_node],
                )),
            })
        } else {
            // Expect closing bracket
            match self.tokens.peek() {
                Some(token) if matches!(token.kind, TokenKind::RBracket) => {
                    let _ = self.tokens.next(); // consume ']'
                }
                Some(token) => {
                    return Err(ParseError::ExpectedClosingBracket((***token).clone()));
                }
                None => {
                    return Err(ParseError::ExpectedClosingBracket(Token {
                        range: original_token.range.clone(),
                        kind: TokenKind::Eof,
                        module_id: self.module_id,
                    }));
                }
            }

            Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &original_token),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(
                        constants::GET,
                        Some(Shared::clone(&original_token)),
                    ),
                    smallvec![target_node, first_node],
                )),
            })
        };

        // Check for additional bracket access (nested indexing)
        if matches!(
            self.tokens.peek().map(|t| &t.kind),
            Some(TokenKind::LBracket)
        ) {
            self.parse_bracket_access(result_node, original_token)
        } else {
            Ok(result_node)
        }
    }

    // Parses a 'def' expression (function definition).
    // Syntax: def ident(arg1, arg2, ...): body_expr ;
    // Example: def my_func(a, b): add(a, b) ;
    fn parse_expr_def(&mut self, def_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let ident_token = self.tokens.next();
        let ident = match &ident_token {
            Some(token) => match &***token {
                Token {
                    range: _,
                    kind: TokenKind::Ident(ident),
                    module_id: _,
                } => Ok(ident),
                token => Err(ParseError::UnexpectedToken((*token).clone())),
            },
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }?;
        let def_token_id = token_alloc(&self.token_arena, &def_token);
        let args = self.parse_args()?;

        if !args.is_empty() && !args.iter().all(|a| matches!(&*a.expr, Expr::Ident(_))) {
            return Err(ParseError::UnexpectedToken(
                (*get_token(Shared::clone(&self.token_arena), def_token_id)).clone(),
            ));
        }

        let token_id = args
            .last()
            .map(|last| last.token_id)
            .unwrap_or(def_token_id);
        self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        let program = self.parse_program(false)?;

        Ok(Shared::new(Node {
            token_id: def_token_id,
            expr: Shared::new(Expr::Def(
                IdentWithToken::new_with_token(ident, ident_token.map(Shared::clone)),
                args,
                program,
            )),
        }))
    }

    fn parse_expr_block(&mut self, do_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let do_token_id = token_alloc(&self.token_arena, &do_token);

        let program = self.parse_program(false)?;

        // The End token is already consumed by parse_program when it encounters it
        // No need to expect another End token here

        Ok(Shared::new(Node {
            token_id: do_token_id,
            expr: Shared::new(Expr::Block(program)),
        }))
    }

    fn parse_fn(&mut self, fn_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let fn_token_id = token_alloc(&self.token_arena, &fn_token);
        let args = self.parse_args()?;

        if !args.is_empty() && !args.iter().all(|a| matches!(&*a.expr, Expr::Ident(_))) {
            return Err(ParseError::UnexpectedToken(
                (*get_token(Shared::clone(&self.token_arena), fn_token_id)).clone(),
            ));
        }

        let token_id = args.last().map(|last| last.token_id).unwrap_or(fn_token_id);
        self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        let program = self.parse_program(false)?;

        Ok(Shared::new(Node {
            token_id: fn_token_id,
            expr: Shared::new(Expr::Fn(args, program)),
        }))
    }

    fn parse_while(&mut self, while_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &while_token);
        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(ParseError::UnexpectedToken((*while_token).clone()));
        }

        self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        match self.tokens.peek() {
            Some(_) => {
                let cond = args.first().unwrap();
                let body_program = self.parse_program(false)?;

                Ok(Shared::new(Node {
                    token_id,
                    expr: Shared::new(Expr::While(
                        Shared::clone(cond),
                        body_program.iter().map(Shared::clone).collect(),
                    )),
                }))
            }
            None => Err(ParseError::UnexpectedToken((*while_token).clone())),
        }
    }

    fn parse_until(&mut self, until_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &until_token);
        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(ParseError::UnexpectedToken((*until_token).clone()));
        }

        self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        match self.tokens.peek() {
            Some(_) => {
                let cond = args.first().unwrap();
                let body_program = self.parse_program(false)?;

                Ok(Shared::new(Node {
                    token_id,
                    expr: Shared::new(Expr::Until(
                        Shared::clone(cond),
                        body_program.iter().map(Shared::clone).collect(),
                    )),
                }))
            }
            None => Err(ParseError::UnexpectedToken((*until_token).clone())),
        }
    }

    fn parse_foreach(&mut self, foreach_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &foreach_token);
        let args = self.parse_args()?;

        if args.len() != 2 {
            return Err(ParseError::UnexpectedToken((*foreach_token).clone()));
        }

        let first_arg = &*args.first().unwrap().expr;

        match first_arg {
            Expr::Ident(IdentWithToken {
                name: ident,
                token: ident_token,
            }) => {
                self.next_token(token_id, |token_kind| {
                    matches!(token_kind, TokenKind::Colon)
                })?;

                let each_values = Shared::clone(&args[1]);
                let body_program = self.parse_program(false)?;

                Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &foreach_token),
                    expr: Shared::new(Expr::Foreach(
                        IdentWithToken {
                            name: *ident,
                            token: ident_token.clone(),
                        },
                        Shared::clone(&each_values),
                        body_program.iter().map(Shared::clone).collect(),
                    )),
                }))
            }
            _ => Err(ParseError::UnexpectedToken((*foreach_token).clone())),
        }
    }

    // Parses an 'if' expression, including optional 'elif' and 'else' branches.
    // Syntax: if (condition): then_expr [ elif (condition): elif_expr ]* [ else: else_expr ]
    // Example: if (x > 10): "greater" else: "smaller_or_equal"
    fn parse_expr_if(&mut self, if_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &if_token);
        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(ParseError::UnexpectedToken(
                (*get_token(Shared::clone(&self.token_arena), token_id)).clone(),
            ));
        }
        let cond = args.first().unwrap();
        let token_id = self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;
        let then_expr = self.parse_next_expr(token_id)?;

        let mut branches: Branches = SmallVec::new();
        branches.push((Some(Shared::clone(cond)), then_expr));

        let elif_branches = self.parse_elif(token_id)?;
        branches.extend(elif_branches);

        if let Some(token) = self.tokens.peek() {
            if matches!(token.kind, TokenKind::Else) {
                let token_id =
                    self.next_token(token_id, |token_kind| matches!(token_kind, TokenKind::Else))?;
                let token_id = self.next_token(token_id, |token_kind| {
                    matches!(token_kind, TokenKind::Colon)
                })?;
                let else_expr = self.parse_next_expr(token_id)?;
                branches.push((None, else_expr));
            }
        }

        Ok(Shared::new(Node {
            token_id: token_alloc(&self.token_arena, &if_token),
            expr: Shared::new(Expr::If(branches)),
        }))
    }

    #[inline(always)]
    fn parse_next_expr(&mut self, token_id: TokenId) -> Result<Shared<Node>, ParseError> {
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(ParseError::UnexpectedToken(
                (*get_token(Shared::clone(&self.token_arena), token_id)).clone(),
            )),
        }?;

        self.parse_expr(Shared::clone(expr_token))
    }

    fn parse_elif(&mut self, token_id: TokenId) -> Result<Vec<IfExpr>, ParseError> {
        let mut nodes = Vec::with_capacity(8);

        while let Some(token) = self.tokens.peek() {
            if matches!(token.kind, TokenKind::Comment(_)) {
                self.tokens.next();
                continue;
            }

            if !matches!(token.kind, TokenKind::Elif) {
                break;
            }

            let token_id =
                self.next_token(token_id, |token_kind| matches!(token_kind, TokenKind::Elif))?;
            let args = self.parse_args()?;

            if args.len() != 1 {
                return Err(ParseError::UnexpectedToken(
                    (*get_token(Shared::clone(&self.token_arena), token_id)).clone(),
                ));
            }

            let token_id = self.next_token(token_id, |token_kind| {
                matches!(token_kind, TokenKind::Colon)
            })?;

            let expr_token = match self.tokens.next() {
                Some(token) => Ok(token),
                None => Err(ParseError::UnexpectedToken(
                    (*get_token(Shared::clone(&self.token_arena), token_id)).clone(),
                )),
            }?;

            let cond = args.first().unwrap();
            let then_expr = self.parse_expr(Shared::clone(expr_token))?;

            nodes.push((Some(Shared::clone(cond)), then_expr));
        }

        Ok(nodes)
    }

    fn parse_expr_let(&mut self, let_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        let ident_token = self.tokens.next();
        let ident = match &ident_token {
            Some(token) => match &***token {
                Token {
                    range: _,
                    kind: TokenKind::Ident(ident),
                    module_id: _,
                } => Ok(ident),
                token => Err(ParseError::UnexpectedToken((*token).clone())),
            },
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }?;

        let let_token_id = token_alloc(&self.token_arena, &let_token);
        self.next_token(let_token_id, |token_kind| {
            matches!(token_kind, TokenKind::Equal)
        })?;
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }?;

        let ast = self.parse_expr(Shared::clone(expr_token))?;

        self.next_token_with_eof(let_token_id, |token_kind| {
            matches!(
                token_kind,
                TokenKind::Pipe | TokenKind::Eof | TokenKind::Comment(_)
            )
        })?;

        Ok(Shared::new(Node {
            token_id: let_token_id,
            expr: Shared::new(Expr::Let(
                IdentWithToken::new_with_token(ident, ident_token.map(Shared::clone)),
                ast,
            )),
        }))
    }

    #[inline(always)]
    fn parse_include(&mut self, include_token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        match self.tokens.peek() {
            Some(token) => match &***token {
                Token {
                    range: _,
                    kind: TokenKind::StringLiteral(module),
                    module_id: _,
                } => {
                    self.tokens.next();
                    Ok(Shared::new(Node {
                        token_id: token_alloc(&self.token_arena, &include_token),
                        expr: Shared::new(Expr::Include(Literal::String(module.to_owned()))),
                    }))
                }
                token => Err(ParseError::InsufficientTokens((*token).clone())),
            },
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }
    }

    fn parse_interpolated_string(
        &mut self,
        token: Shared<Token>,
    ) -> Result<Shared<Node>, ParseError> {
        if let TokenKind::InterpolatedString(segments) = &token.kind {
            let segments = segments.iter().map(|seg| seg.into()).collect::<Vec<_>>();

            Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::InterpolatedString(segments)),
            }))
        } else {
            Err(ParseError::UnexpectedToken((*token).clone()))
        }
    }

    fn parse_args(&mut self) -> Result<Args, ParseError> {
        match self.tokens.peek() {
            Some(token) => match &***token {
                Token {
                    range: _,
                    kind: TokenKind::LParen,
                    module_id: _,
                } => {
                    self.tokens.next();
                }
                token => return Err(ParseError::UnexpectedToken((*token).clone())),
            },
            None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        };

        let mut args: Args = SmallVec::new();
        let mut prev_token: Option<&Token> = None;

        while let Some(token) = self.tokens.next() {
            match &**token {
                Token {
                    range: _,
                    kind: TokenKind::RParen,
                    module_id: _,
                } => match prev_token {
                    Some(Token {
                        range: _,
                        kind: TokenKind::Comma,
                        module_id: _,
                    }) => {
                        return Err(ParseError::UnexpectedToken((**token).clone()));
                    }
                    _ => break,
                },
                Token {
                    range: _,
                    kind: TokenKind::Eof,
                    module_id: _,
                } => match prev_token {
                    Some(Token {
                        range: _,
                        kind: TokenKind::RParen,
                        module_id: _,
                    }) => break,
                    Some(_) | None => {
                        return Err(ParseError::ExpectedClosingParen((**token).clone()));
                    }
                },
                Token {
                    range: _,
                    kind: TokenKind::Comma,
                    module_id: _,
                } => match prev_token {
                    Some(_) => {
                        let token = match self.tokens.peek() {
                            Some(token) => Ok(Shared::clone(token)),
                            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
                        }?;
                        match &*token {
                            Token {
                                range: _,
                                kind: TokenKind::Comma,
                                module_id: _,
                            } => {
                                return Err(ParseError::UnexpectedToken((*token).clone()));
                            }
                            _ => continue,
                        }
                    }
                    None => return Err(ParseError::UnexpectedToken((**token).clone())),
                },
                Token {
                    range: _,
                    kind: TokenKind::SemiColon,
                    module_id: _,
                } => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
                _ => {
                    // Arguments that are complex expressions (idents, selectors, if, fn)
                    args.push(self.parse_arg_expr(Shared::clone(token))?);
                }
            }

            prev_token = Some(token);

            if let Some(token) = self.tokens.peek() {
                if !matches!(token.kind, TokenKind::RParen | TokenKind::Comma) {
                    return Err(ParseError::ExpectedClosingParen((***token).clone()));
                }
            }
        }

        Ok(args)
    }

    // Helper to parse an argument that is expected to be a general expression.
    // This typically involves a recursive call to `parse_expr`.
    #[inline(always)]
    fn parse_arg_expr(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        self.parse_expr(Shared::clone(&token))
    }

    #[inline(always)]
    fn parse_head(&mut self, token: Shared<Token>, depth: u8) -> Result<Shared<Node>, ParseError> {
        Ok(Shared::new(Node {
            token_id: token_alloc(&self.token_arena, &token),
            expr: Shared::new(Expr::Selector(Selector::Heading(Some(depth)))),
        }))
    }

    /// Parse a selector with an attribute suffix and convert it to an attr() function call
    fn parse_selector_with_attribute(
        &mut self,
        token: Shared<Token>,
        attr_pos: usize,
    ) -> Result<Shared<Node>, ParseError> {
        if let TokenKind::Selector(selector) = &token.kind {
            let base_selector = &selector[..attr_pos];
            let attribute = &selector[attr_pos + 1..]; // Skip the dot

            // Create a new token for the base selector
            let base_token = Shared::new(Token {
                range: token.range.clone(),
                kind: TokenKind::Selector(SmolStr::new(base_selector)),
                module_id: token.module_id,
            });

            // Parse the base selector recursively
            let base_node = self.parse_selector_direct(base_token)?;

            // Create the attribute string literal
            let attr_literal = Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::Literal(Literal::String(attribute.to_string()))),
            });

            // Create the attr() function call
            Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::ATTR, Some(Shared::clone(&token))),
                    smallvec![base_node, attr_literal],
                )),
            }))
        } else {
            Err(ParseError::UnexpectedToken((*token).clone()))
        }
    }

    /// Parse a selector without checking for attributes (to avoid infinite recursion)
    fn parse_selector_direct(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        if let TokenKind::Selector(selector) = &token.kind {
            match selector.as_str() {
                // Handles heading selectors like `.h` or `.h(level)`.
                ".h" => self.parse_selector_heading_args(Shared::clone(&token)),
                ".h1" => self.parse_head(token, 1),
                ".h2" => self.parse_head(token, 2),
                ".h3" => self.parse_head(token, 3),
                ".h4" => self.parse_head(token, 4),
                ".h5" => self.parse_head(token, 5),
                ".h6" => self.parse_head(token, 6),
                ".>" | ".blockquote" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Blockquote)),
                })),
                ".^" | ".footnote" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Footnote)),
                })),
                ".<" | ".mdx_jsx_flow_element" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::MdxJsxFlowElement)),
                })),
                ".**" | ".emphasis" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Emphasis)),
                })),
                ".$$" | ".math" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Math)),
                })),
                ".horizontal_rule" | ".---" | ".***" | ".___" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::HorizontalRule)),
                })),
                ".{}" | ".mdx_text_expression" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::MdxTextExpression)),
                })),
                ".[^]" | ".footnote_ref" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::FootnoteRef)),
                })),
                ".definition" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Definition)),
                })),
                ".break" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Break)),
                })),
                ".delete" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Delete)),
                })),
                ".<>" | ".html" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Html)),
                })),
                ".image" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Image)),
                })),
                ".image_ref" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::ImageRef)),
                })),
                ".code_inline" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::InlineCode)),
                })),
                ".math_inline" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::InlineMath)),
                })),
                ".link" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Link)),
                })),
                ".link_ref" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::LinkRef)),
                })),
                ".list" => self.parse_selector_list_args(Shared::clone(&token)),
                ".toml" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Toml)),
                })),
                ".strong" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Strong)),
                })),
                ".yaml" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Yaml)),
                })),
                ".code" => {
                    if let Ok(s) = self.parse_string_arg(Shared::clone(&token)) {
                        Ok(Shared::new(Node {
                            token_id: token_alloc(&self.token_arena, &token),
                            expr: Shared::new(Expr::Selector(Selector::Code(Some(SmolStr::new(
                                s,
                            ))))),
                        }))
                    } else {
                        Ok(Shared::new(Node {
                            token_id: token_alloc(&self.token_arena, &token),
                            expr: Shared::new(Expr::Selector(Selector::Code(None))),
                        }))
                    }
                }
                ".mdx_js_esm" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::MdxJsEsm)),
                })),
                ".mdx_jsx_text_element" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::MdxJsxTextElement)),
                })),
                ".mdx_flow_expression" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::MdxFlowExpression)),
                })),
                ".text" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Text)),
                })),
                ".table" => Ok(Shared::new(Node {
                    token_id: token_alloc(&self.token_arena, &token),
                    expr: Shared::new(Expr::Selector(Selector::Table(None, None))),
                })),
                // Handles table/array indexing selectors like `.[index]` or `.[index1][index2]`.
                "." => self.parse_selector_table_args(Shared::clone(&token)),
                _ => Err(ParseError::UnexpectedToken((*token).clone())),
            }
        } else {
            Err(ParseError::InsufficientTokens((*token).clone()))
        }
    }

    fn parse_selector(&mut self, token: Shared<Token>) -> Result<Shared<Node>, ParseError> {
        if let TokenKind::Selector(selector) = &token.kind {
            // Check if the selector has an attribute suffix (e.g., ".h.text")
            if let Some(attr_pos) = selector[1..].find('.').map(|pos| pos + 1) {
                return self.parse_selector_with_attribute(token, attr_pos);
            }

            // Use the direct parser for normal selectors
            self.parse_selector_direct(token)
        } else {
            Err(ParseError::InsufficientTokens((*token).clone()))
        }
    }

    // Parses arguments for table or list item selectors like `.[index1][index2]` (for tables) or `.[index1]` (for lists).
    // Example: .[0][1] or .[0]
    fn parse_selector_table_args(
        &mut self,
        token: Shared<Token>,
    ) -> Result<Shared<Node>, ParseError> {
        let token1 = match self.tokens.peek() {
            Some(token) => Ok(Shared::clone(token)),
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }?;

        let ArrayIndex(i1) = self.parse_int_array_arg(&token1)?;
        let token2 = match self.tokens.peek() {
            Some(token) => Ok(Shared::clone(token)),
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }?;

        if let Token {
            range: _,
            kind: TokenKind::LBracket,
            module_id: _,
        } = &*token2
        {
            // .[n][n]
            let ArrayIndex(i2) = self.parse_int_array_arg(&token2)?;
            Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::Selector(Selector::Table(i1, i2))),
            }))
        } else {
            // .[n]
            Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::Selector(Selector::List(i1, None))),
            }))
        }
    }

    #[inline(always)]
    fn parse_selector_list_args(
        &mut self,
        token: Shared<Token>,
    ) -> Result<Shared<Node>, ParseError> {
        if let Ok(i) = self.parse_int_arg(Shared::clone(&token)) {
            Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::Selector(Selector::List(Some(i as usize), None))),
            }))
        } else {
            Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::Selector(Selector::List(None, None))),
            }))
        }
    }

    // Parses arguments for heading selectors like `.h(level)` or just `.h`.
    // Example: .h(1) or .h
    fn parse_selector_heading_args(
        &mut self,
        token: Shared<Token>,
    ) -> Result<Shared<Node>, ParseError> {
        if let Ok(depth) = self.parse_int_arg(Shared::clone(&token)) {
            Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::Selector(Selector::Heading(Some(depth as u8)))),
            }))
        } else {
            Ok(Shared::new(Node {
                token_id: token_alloc(&self.token_arena, &token),
                expr: Shared::new(Expr::Selector(Selector::Heading(None))),
            }))
        }
    }

    #[inline(always)]
    fn parse_int_arg(&mut self, token: Shared<Token>) -> Result<i64, ParseError> {
        let args = self.parse_int_args(Shared::clone(&token))?;

        if args.len() == 1 {
            Ok(args[0])
        } else {
            Err(ParseError::UnexpectedToken((*token).clone()))
        }
    }

    #[inline(always)]
    fn parse_string_arg(&mut self, token: Shared<Token>) -> Result<String, ParseError> {
        let args = self.parse_string_args(Shared::clone(&token))?;

        if args.len() == 1 {
            Ok(args[0].clone())
        } else {
            Err(ParseError::UnexpectedToken((*token).clone()))
        }
    }

    fn parse_int_array_arg(&mut self, token: &Shared<Token>) -> Result<ArrayIndex, ParseError> {
        let token_id = token_alloc(&self.token_arena, token);
        self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::LBracket)
        })?;

        let token = match self.tokens.peek() {
            Some(token) => Ok(Shared::clone(token)),
            None => return Err(ParseError::InsufficientTokens((**token).clone())),
        }?;

        if let Token {
            range: _,
            kind: TokenKind::NumberLiteral(n),
            module_id: _,
        } = &*token
        {
            let token_id = token_alloc(&self.token_arena, &token);
            self.tokens.next();
            self.next_token(token_id, |token_kind| {
                matches!(token_kind, TokenKind::RBracket)
            })?;
            Ok(ArrayIndex(Some(n.value() as usize)))
        } else if let Token {
            range: _,
            kind: TokenKind::RBracket,
            module_id: _,
        } = &*token
        {
            self.tokens.next();
            Ok(ArrayIndex(None))
        } else {
            Err(ParseError::UnexpectedToken((*token).clone()))
        }
    }

    fn parse_int_args(&mut self, arg_token: Shared<Token>) -> Result<Vec<i64>, ParseError> {
        let mut args = Vec::with_capacity(8);
        let token_id = token_alloc(&self.token_arena, &arg_token);

        self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::LParen)
        })?;

        loop {
            match self.tokens.next() {
                Some(token) => match &**token {
                    Token {
                        range: _,
                        kind: TokenKind::NumberLiteral(n),
                        module_id: _,
                    } => {
                        args.push(n.value() as i64);
                    }
                    Token {
                        range: _,
                        kind: TokenKind::RParen,
                        module_id: _,
                    } => break,
                    Token {
                        range: _,
                        kind: TokenKind::Comma,
                        module_id: _,
                    } => continue,
                    token => return Err(ParseError::UnexpectedToken((*token).clone())),
                },
                None => return Err(ParseError::InsufficientTokens((*arg_token).clone())),
            }
        }

        Ok(args)
    }

    fn parse_string_args(&mut self, arg_token: Shared<Token>) -> Result<Vec<String>, ParseError> {
        let token_id = token_alloc(&self.token_arena, &arg_token);
        self.next_token(token_id, |token_kind| {
            matches!(token_kind, TokenKind::LParen)
        })?;

        let mut args = Vec::with_capacity(8);

        loop {
            match self.tokens.next() {
                Some(token) => match &**token {
                    Token {
                        range: _,
                        kind: TokenKind::StringLiteral(s),
                        module_id: _,
                    } => {
                        args.push(s.to_owned());
                    }
                    Token {
                        range: _,
                        kind: TokenKind::RParen,
                        module_id: _,
                    } => break,
                    Token {
                        range: _,
                        kind: TokenKind::Comma,
                        module_id: _,
                    } => continue,
                    token => return Err(ParseError::UnexpectedToken((*token).clone())),
                },
                None => return Err(ParseError::InsufficientTokens((*arg_token).clone())),
            }
        }

        Ok(args)
    }

    #[inline(always)]
    fn next_token_with_eof(
        &mut self,
        current_token_id: TokenId,
        expected_kinds: fn(&TokenKind) -> bool,
    ) -> Result<TokenId, ParseError> {
        self._next_token(current_token_id, expected_kinds, true)
    }

    #[inline(always)]
    fn next_token(
        &mut self,
        current_token_id: TokenId,
        expected_kinds: fn(&TokenKind) -> bool,
    ) -> Result<TokenId, ParseError> {
        self._next_token(current_token_id, expected_kinds, false)
    }

    fn _next_token(
        &mut self,
        current_token_id: TokenId,
        expected_kinds: fn(&TokenKind) -> bool,
        expected_eof: bool,
    ) -> Result<TokenId, ParseError> {
        match self.tokens.peek() {
            // Token found and matches one of the expected kinds.
            Some(token) if expected_kinds(&token.kind) => {
                let token = self.tokens.next().unwrap();
                Ok(token_alloc(&self.token_arena, token))
            } // Consume and return.
            // Token found but does not match expected kinds.
            Some(token) => Err(ParseError::UnexpectedToken(Token {
                range: token.range.clone(),
                kind: token.kind.clone(),
                module_id: token.module_id,
            })),
            // No token found (EOF).
            None => {
                if expected_eof {
                    // If EOF is explicitly allowed in this context (e.g. end of a 'let' binding),
                    // fabricate an EOF token to satisfy the parser's expectation.
                    // This simplifies some parsing logic by not having to handle None explicitly everywhere.
                    let range = get_token(Shared::clone(&self.token_arena), current_token_id)
                        .range
                        .clone();
                    let module_id =
                        get_token(Shared::clone(&self.token_arena), current_token_id).module_id;
                    Ok(token_alloc(
                        &self.token_arena,
                        &Shared::new(Token {
                            range,
                            kind: TokenKind::Eof,
                            module_id,
                        }),
                    ))
                } else {
                    // If EOF is not expected here, it's an error.
                    Err(ParseError::UnexpectedEOFDetected(self.module_id))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Module, range::Range};

    use super::*;
    use rstest::rstest;
    use smallvec::smallvec;

    fn token(token_kind: TokenKind) -> Token {
        Token {
            range: Range::default(),
            kind: token_kind,
            module_id: 1.into(),
        }
    }

    #[rstest]
    #[case::ident1(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("startswith"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test2".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::AND, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("and")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token("contains", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("contains")))))),
                                smallvec![Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("test".to_owned())))
                                })],
                            ))
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token("startswith", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("startswith")))))),
                                smallvec![Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("test2".to_owned())))
                                })],
                            ))
                        })
                    ],
                ))
            })
        ]))]
    #[case::ident2(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::Selector(SmolStr::new(".h1"))),
            token(TokenKind::Comma),
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 8.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::AND, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("and")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Selector(Selector::Heading(Some(1)))),
                        }),
                        Shared::new(Node {
                            token_id: 7.into(),
                            expr: Shared::new(Expr::Selector(Selector::Table(Some(2), None))),
                        }),
                    ],
                ))
            })
        ]))]
    #[case::ident3(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("filter"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("arg1"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("arg2"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("arg1".to_owned())),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("arg2".to_owned())),
            token(TokenKind::RParen),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Def(
                    IdentWithToken::new_with_token("filter", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("filter")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("arg1", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arg1")))))))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("arg2", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arg2")))))))),
                        }),
                    ],
                    vec![Shared::new(Node {
                        token_id: 6.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token("contains", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("contains")))))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 4.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("arg1".to_owned()))),
                                }),
                                Shared::new(Node {
                                    token_id: 5.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("arg2".to_owned()))),
                                }),
                            ],
                        )),
                    })],
                )),
            }),
        ]))]
    #[case::ident4(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::None),
            token(TokenKind::Comma),
            token(TokenKind::Self_),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::AND, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("and")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Literal(Literal::None)),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Self_),
                        }),
                    ],
                ))
            })
        ]))]
    #[case::ident5(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::None),
            token(TokenKind::Comma),
            token(TokenKind::Self_),
            token(TokenKind::RParen),
            token(TokenKind::Ident(SmolStr::new("and"))),
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::Ident(SmolStr::new("and"))))))]
    #[case::ident5(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::None),
            token(TokenKind::Comma),
            token(TokenKind::Self_),
            token(TokenKind::RParen),
            token(TokenKind::Def),
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::Def))))]
    #[case::ident6(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::Def),
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::Ident(SmolStr::new("and"))))))]
    #[case::error(
        vec![
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::Selector(SmolStr::new("inline_code"))),
            token(TokenKind::Eof)
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::Selector(SmolStr::new("inline_code"))))))]
    #[case::def1(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Def(
                        IdentWithToken::new_with_token("name", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("name")))))),
                        SmallVec::new(),
                        vec![Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                        })],
                )),
            }),
        ]))]
    #[case::def_with_end(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::End)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Def(
                        IdentWithToken::new_with_token("name", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("name")))))),
                        SmallVec::new(),
                        vec![Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                        })],
                )),
            }),
        ]))]
    #[case::def2(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::Comma),
            token(TokenKind::RParen),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Comma, module_id: 1.into()})))]
    #[case::def3(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::Comma),
            token(TokenKind::RParen),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Def, module_id: 1.into()})))]
    #[case::def4(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Def, module_id: 1.into()})))]
    #[case::def5(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Def, module_id: 1.into()})))]
    #[case::def6(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::SemiColon),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Def, module_id: 1.into()})))]
    #[case::def7(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Def, module_id: 1.into()})))]
    #[case::def7(
        vec![
            token(TokenKind::Def),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::LParen, module_id: 1.into()})))]
    #[case::let_1(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Shared::new(Expr::Let(
                        IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(42.into()))),
                        }),
                    )),
                })
            ]))]
    #[case::let_2(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("y"))),
                token(TokenKind::Equal),
                token(TokenKind::StringLiteral("hello".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Shared::new(Expr::Let(
                        IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y")))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("hello".to_owned()))),
                        }),
                    )),
                })
            ]))]
    #[case::let_3(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("flag"))),
                token(TokenKind::Equal),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Shared::new(Expr::Let(
                        IdentWithToken::new_with_token("flag", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("flag")))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                        }),
                    )),
                })
            ]))]
    #[case::let_4(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("z"))),
                token(TokenKind::Equal),
                token(TokenKind::Ident(SmolStr::new("some_var"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Shared::new(Expr::Let(
                        IdentWithToken::new_with_token("z", Some(Shared::new(token(TokenKind::Ident("z".into()))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(
                                Expr::Ident(IdentWithToken::new_with_token("some_var",
                                                 Some(Shared::new(token(TokenKind::Ident(SmolStr::new("some_var"))))))))
                        }),
                    )),
                })
            ]))]
    #[case::let_5(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("z"))),
                token(TokenKind::Equal),
                token(TokenKind::Ident(SmolStr::new("some_var"))),
                token(TokenKind::Pipe),
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Shared::new(Expr::Let(
                        IdentWithToken::new_with_token("z", Some(Shared::new(token(TokenKind::Ident("z".into()))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(
                                Expr::Ident(IdentWithToken::new_with_token("some_var", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("some_var")))))))),
                        }),
                    )),
                })
            ]))]
    #[case::let_6(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(SmolStr::new("z"))),
                token(TokenKind::Equal),
                token(TokenKind::Ident(SmolStr::new("some_var"))),
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Shared::new(Expr::Let(
                        IdentWithToken::new_with_token("z", Some(Shared::new(token(TokenKind::Ident("z".into()))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(
                                Expr::Ident(IdentWithToken::new_with_token("some_var", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("some_var")))))))),
                        }),
                    )),
                })
            ]))]
    #[case::root_semicolon_error(
            vec![
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::SemiColon),
                token(TokenKind::Ident(SmolStr::new("y"))),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::if_1(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("false branch".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 7.into(),
                    expr: Shared::new(Expr::If(smallvec![
                        (
                            Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                            })),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("true branch".to_owned()))),
                            })
                        ),
                        (
                            None,
                            Shared::new(Node {
                                token_id: 6.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("false branch".to_owned()))),
                            })
                        )
                    ])),
                })
            ]))]
    #[case::if_elif_else(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(false)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("else branch".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 11.into(),
                    expr: Shared::new(Expr::If(smallvec![
                        (
                            Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                            })),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("true branch".to_owned()))),
                            })
                        ),
                        (
                            Some(Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Literal(Literal::Bool(false))),
                            })),
                            Shared::new(Node {
                                token_id: 7.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("elif branch".to_owned()))),
                            })
                        ),
                        (
                            None,
                            Shared::new(Node {
                                token_id: 10.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("else branch".to_owned()))),
                            })
                        )
                    ])),
                })
            ]))]
    #[case::if_only(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 4.into(),
                    expr: Shared::new(Expr::If(smallvec![
                        (
                            Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                            })),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("true branch".to_owned()))),
                            })
                        ),
                    ])),
                })
            ]))]
    #[case::if_elif(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 8.into(),
                    expr: Shared::new(Expr::If(smallvec![
                        (
                            Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                            })),
                            Shared::new(Node {
                                token_id: 3.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("true branch".to_owned()))),
                            })
                        ),
                        (
                            Some(Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                            })),
                            Shared::new(Node {
                                token_id: 7.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("true branch".to_owned()))),
                            })
                        ),
                    ])),
                })
            ]))]
    #[case::if_error(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(false)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedEOFDetected(0.into())))]
    #[case::if_error(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(false)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::If, module_id: 1.into()})))]
    #[case::elif_error(
            vec![
                token(TokenKind::If),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("true branch".to_owned())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_owned())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Elif, module_id: 1.into()})))]
    #[case::h_selector(
        vec![
            token(TokenKind::Selector(SmolStr::new(".h"))),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(3.into())),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Selector(Selector::Heading(Some(3)))),
            })
        ]))]
    #[case::h_selector_without_number(
        vec![
            token(TokenKind::Selector(SmolStr::new(".h"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Shared::new(Expr::Selector(Selector::Heading(None))),
            })
        ]))]
    #[case::while_(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(Expr::While(
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                }),
                vec![Shared::new(Node {
                    token_id: 3.into(),
                    expr: Shared::new(Expr::Literal(Literal::String("loop body".to_owned()))),
                })],
            )),
        })]))]
    #[case::while_error(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::While, module_id: 1.into()})))]
    #[case::while_error(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::While, module_id: 1.into()})))]
    #[case::until(
        vec![
            token(TokenKind::Until),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(false)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(Expr::Until(
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Literal(Literal::Bool(false))),
                }),
                vec![Shared::new(Node {
                    token_id: 3.into(),
                    expr: Shared::new(Expr::Literal(Literal::String("loop body".to_owned()))),
                })],
            )),
        })]))]
    #[case::until_error(
        vec![
            token(TokenKind::Until),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Eof),
        ],
        Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::until_error(
        vec![
            token(TokenKind::Until),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Eof),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Until, module_id: 1.into()})))]
    #[case::foreach(
        vec![
            token(TokenKind::Foreach),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("array".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("print"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::RParen),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 6.into(),
            expr: Shared::new(Expr::Foreach(
                IdentWithToken::new_with_token(
                    "item",
                    Some(Shared::new(token(TokenKind::Ident(SmolStr::new("item"))))),
                ),
                Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(Expr::Literal(Literal::String("array".to_owned()))),
                }),
                vec![Shared::new(Node {
                    token_id: 5.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(
                            "print",
                            Some(Shared::new(token(TokenKind::Ident(SmolStr::new(
                                "print",
                            ))))),
                        ),
                        smallvec![Shared::new(Node {
                            token_id: 4.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(
                                "item",
                                Some(Shared::new(token(TokenKind::Ident(SmolStr::new("item"))))),
                            ))),
                        })],
                    )),
                })],
            )),
        })]))]
    #[case::foreach(
        vec![
            token(TokenKind::Foreach),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("print"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::RParen),
            token(TokenKind::SemiColon),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Foreach, module_id: 1.into()})))]
    #[case::self_(
        vec![token(TokenKind::Self_), token(TokenKind::Eof)],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(Expr::Self_),
        })]))]
    #[case::include(
        vec![
            token(TokenKind::Include),
            token(TokenKind::StringLiteral("module_name".to_owned())),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(Expr::Include(Literal::String("module_name".to_owned()))),
        })]))]
    #[case::code_selector_with_language(
        vec![
            token(TokenKind::Selector(SmolStr::new(".code"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("rust".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 2.into(),
            expr: Shared::new(Expr::Selector(Selector::Code(Some(SmolStr::new(
                "rust",
            ))))),
        })]))]
    #[case::table_selector(
        vec![
            token(TokenKind::Selector(SmolStr::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 8.into(),
            expr: Shared::new(Expr::Selector(Selector::Table(Some(1), Some(2)))),
        })]))]
    #[case::foreach_error(
        vec![
            token(TokenKind::Foreach),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("array".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Eof),
        ],
        Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::while_error(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Eof),
        ],
        Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::using_reserved_keyword_let(
            vec![
                token(TokenKind::Let),
                token(TokenKind::If),  // Using "if" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::If, module_id: 1.into()})))]
    #[case::using_reserved_keyword_while(
            vec![
                token(TokenKind::Let),
                token(TokenKind::While),  // Using "while" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::While, module_id: 1.into()})))]
    #[case::using_reserved_keyword_def(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Def),  // Using "def" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Def, module_id: 1.into()})))]
    #[case::using_reserved_keyword_include(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Include),  // Using "include" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Include, module_id: 1.into()})))]
    #[case::nodes(
        vec![
            token(TokenKind::Nodes),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Nodes),
            })
        ]))]
    #[case::nodes_error_in_subprogram(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("test"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Nodes),
            token(TokenKind::SemiColon)
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::Nodes))))]
    #[case::nodes_then_selector(
        vec![
            token(TokenKind::Nodes),
            token(TokenKind::Pipe),
            token(TokenKind::Selector(SmolStr::new(".h1"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Nodes),
            }),
            Shared::new(Node {
                token_id: 1.into(),
                expr: Shared::new(Expr::Selector(Selector::Heading(Some(1)))),
            })
        ]))]
    #[case::root_level_with_multiple_pipes(
        vec![
            token(TokenKind::Nodes),
            token(TokenKind::Pipe),
            token(TokenKind::Nodes),
            token(TokenKind::Pipe),
            token(TokenKind::Selector(SmolStr::new(".h1"))),
            token(TokenKind::Pipe),
            token(TokenKind::Selector(SmolStr::new(".text"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Nodes),
            }),
            Shared::new(Node {
                token_id: 1.into(),
                expr: Shared::new(Expr::Nodes),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Selector(Selector::Heading(Some(1)))),
            }),
            Shared::new(Node {
                token_id: 3.into(),
                expr: Shared::new(Expr::Selector(Selector::Text)),
            })
        ]))]
    #[case::fn_simple(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("result".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Fn(
                    SmallVec::new(),
                    vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("result".to_owned()))),
                        })
                    ],
                )),
            })
        ]))]
    #[case::fn_with_args(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("y"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("y"))),
            token(TokenKind::RParen),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Fn(
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y")))))))),
                        }),
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token("contains", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("contains")))))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 4.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 5.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y")))))))),
                                    }),
                                ],
                            )),
                        })
                    ],
                )),
            })
        ]))]
    #[case::fn_with_multiple_statements(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("first".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::StringLiteral("second".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Fn(
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                        }),
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("first".to_owned()))),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("second".to_owned()))),
                        })
                    ],
                )),
            })
        ]))]
    #[case::fn_with_invalid_args(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("invalid".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("result".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::Fn))))]
    #[case::fn_without_colon(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::StringLiteral("result".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::StringLiteral("result".to_owned())))))]
    #[case::fn_without_body(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::SemiColon),
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::SemiColon))))]
    #[case::fn_nested_in_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("apply"))),
            token(TokenKind::LParen),
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("processed".to_owned())),
            token(TokenKind::SemiColon),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token("apply", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("apply")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Fn(
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                    }),
                                ],
                                vec![
                                    Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("processed".to_owned()))),
                                    })
                                ],
                            )),
                        })
                    ],
                )),
            })
        ]))]
    #[case::empty_array(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::LBracket)))),
                            SmallVec::new(),
                        )),
                    })
                ]))]
    #[case::array_with_elements(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("first".to_owned())),
                    token(TokenKind::Comma),
                    token(TokenKind::NumberLiteral(42.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::LBracket)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("first".to_owned()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(42.into()))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::array_with_mixed_elements(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("text".to_owned())),
                    token(TokenKind::Comma),
                    token(TokenKind::BoolLiteral(true)),
                    token(TokenKind::Comma),
                    token(TokenKind::None),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::LBracket)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("text".to_owned()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                                }),
                                Shared::new(Node {
                                    token_id: 3.into(),
                                    expr: Shared::new(Expr::Literal(Literal::None)),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::array_with_nested_array(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Comma),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(2.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::LBracket)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Call(
                                        IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::LBracket)))),
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 2.into(),
                                                expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                            }),
                                        ],
                                    )),
                                }),
                                Shared::new(Node {
                                    token_id: 3.into(),
                                    expr: Shared::new(Expr::Call(
                                        IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::LBracket)))),
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 4.into(),
                                                expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                            }),
                                        ],
                                    )),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::array_with_trailing_comma(
                vec![
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("value".to_owned())),
                    token(TokenKind::Comma),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::LBracket)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::array_unclosed(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::StringLiteral("value".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::array_invalid_token(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::Pipe),
                        token(TokenKind::RBracket),
                        token(TokenKind::Eof)
                    ],
                    Err(ParseError::UnexpectedToken(token(TokenKind::Pipe))))]
    #[case::array_nested_unclosed(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::LBracket),
                        token(TokenKind::StringLiteral("inner".to_owned())),
                        token(TokenKind::RBracket),
                        token(TokenKind::Eof)
                    ],
                    Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::array_with_ident(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::Ident(SmolStr::new("foo"))),
                        token(TokenKind::Comma),
                        token(TokenKind::Ident(SmolStr::new("bar"))),
                        token(TokenKind::RBracket),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::LBracket)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("foo", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("foo")))))))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("bar", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("bar")))))))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::equality_simple(
                    vec![
                        token(TokenKind::StringLiteral("hello".to_owned())),
                        token(TokenKind::EqEq),
                        token(TokenKind::StringLiteral("world".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("hello".to_owned()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("world".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::equality_numbers(
                    vec![
                        token(TokenKind::NumberLiteral(42.into())),
                        token(TokenKind::EqEq),
                        token(TokenKind::NumberLiteral(42.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(42.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(42.into()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::equality_booleans(
                    vec![
                        token(TokenKind::BoolLiteral(true)),
                        token(TokenKind::EqEq),
                        token(TokenKind::BoolLiteral(false)),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Bool(false))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::equality_with_identifiers(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::EqEq),
                        token(TokenKind::Ident(SmolStr::new("y"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y")))))))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::equality_with_function_call(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("foo"))),
                        token(TokenKind::LParen),
                        token(TokenKind::StringLiteral("arg".to_owned())),
                        token(TokenKind::RParen),
                        token(TokenKind::EqEq),
                        token(TokenKind::StringLiteral("result".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Call(
                                            IdentWithToken::new_with_token("foo", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("foo")))))),
                                            smallvec![
                                                Shared::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Shared::new(Expr::Literal(Literal::String("arg".to_owned()))),
                                                }),
                                            ],
                                        )),
                                    }),
                                    Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("result".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::equality_with_selectors(
                    vec![
                        token(TokenKind::Selector(SmolStr::new(".h1"))),
                        token(TokenKind::EqEq),
                        token(TokenKind::Selector(SmolStr::new(".text"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Selector(Selector::Heading(Some(1)))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Selector(Selector::Text)),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::equality_with_none(
                    vec![
                        token(TokenKind::None),
                        token(TokenKind::EqEq),
                        token(TokenKind::None),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::None)),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::None)),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::equality_error_missing_rhs(
                    vec![
                        token(TokenKind::StringLiteral("hello".to_owned())),
                        token(TokenKind::EqEq),
                        token(TokenKind::Eof)
                    ],
                    Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::equality_in_if_condition(
                    vec![
                        token(TokenKind::If),
                        token(TokenKind::LParen),
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::EqEq),
                        token(TokenKind::NumberLiteral(5.into())),
                        token(TokenKind::RParen),
                        token(TokenKind::Colon),
                        token(TokenKind::StringLiteral("equal".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Shared::new(Expr::If(smallvec![
                                (
                                    Some(Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Call(
                                            IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                                            smallvec![
                                                Shared::new(Node {
                                                    token_id: 1.into(),
                                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                                }),
                                                Shared::new(Node {
                                                    token_id: 3.into(),
                                                    expr: Shared::new(Expr::Literal(Literal::Number(5.into()))),
                                                }),
                                            ],
                                        )),
                                    })),
                                    Shared::new(Node {
                                        token_id: 5.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("equal".to_owned()))),
                                    })
                                ),
                            ])),
                        })
                    ]))]
    #[case::not_equality_simple(
                    vec![
                        token(TokenKind::StringLiteral("hello".to_owned())),
                        token(TokenKind::NeEq),
                        token(TokenKind::StringLiteral("world".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::NE, Some(Shared::new(token(TokenKind::NeEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("hello".to_owned()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("world".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::not_equality_numbers(
                    vec![
                        token(TokenKind::NumberLiteral(42.into())),
                        token(TokenKind::NeEq),
                        token(TokenKind::NumberLiteral(24.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::NE, Some(Shared::new(token(TokenKind::NeEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(42.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(24.into()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::not_equality_booleans(
                    vec![
                        token(TokenKind::BoolLiteral(true)),
                        token(TokenKind::NeEq),
                        token(TokenKind::BoolLiteral(false)),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::NE, Some(Shared::new(token(TokenKind::NeEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Bool(false))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::not_equality_with_identifiers(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::NeEq),
                        token(TokenKind::Ident(SmolStr::new("y"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::NE, Some(Shared::new(token(TokenKind::NeEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y")))))))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::not_equality_with_function_call(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("foo"))),
                        token(TokenKind::LParen),
                        token(TokenKind::StringLiteral("arg".to_owned())),
                        token(TokenKind::RParen),
                        token(TokenKind::NeEq),
                        token(TokenKind::StringLiteral("result".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::NE, Some(Shared::new(token(TokenKind::NeEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Call(
                                            IdentWithToken::new_with_token("foo", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("foo")))))),
                                            smallvec![
                                                Shared::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Shared::new(Expr::Literal(Literal::String("arg".to_owned()))),
                                                }),
                                            ],
                                        )),
                                    }),
                                    Shared::new(Node {
                                        token_id: 3.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("result".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::not_equality_with_selectors(
                    vec![
                        token(TokenKind::Selector(SmolStr::new(".h1"))),
                        token(TokenKind::NeEq),
                        token(TokenKind::Selector(SmolStr::new(".text"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::NE, Some(Shared::new(token(TokenKind::NeEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Selector(Selector::Heading(Some(1)))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Selector(Selector::Text)),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::not_equality_with_none(
                    vec![
                        token(TokenKind::None),
                        token(TokenKind::NeEq),
                        token(TokenKind::StringLiteral("something".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::NE, Some(Shared::new(token(TokenKind::NeEq)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::None)),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("something".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::not_equality_error_missing_rhs(
                    vec![
                        token(TokenKind::StringLiteral("hello".to_owned())),
                        token(TokenKind::NeEq),
                        token(TokenKind::Eof)
                    ],
                    Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::not_equality_in_if_condition(
                    vec![
                        token(TokenKind::If),
                        token(TokenKind::LParen),
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::NeEq),
                        token(TokenKind::NumberLiteral(5.into())),
                        token(TokenKind::RParen),
                        token(TokenKind::Colon),
                        token(TokenKind::StringLiteral("not equal".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 6.into(),
                            expr: Shared::new(Expr::If(smallvec![
                                (
                                    Some(Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Call(
                                            IdentWithToken::new_with_token(constants::NE, Some(Shared::new(token(TokenKind::NeEq)))),
                                            smallvec![
                                                Shared::new(Node {
                                                    token_id: 1.into(),
                                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                                }),
                                                Shared::new(Node {
                                                    token_id: 3.into(),
                                                    expr: Shared::new(Expr::Literal(Literal::Number(5.into()))),
                                                }),
                                            ],
                                        )),
                                    })),
                                    Shared::new(Node {
                                        token_id: 5.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("not equal".to_owned()))),
                                    })
                                ),
                            ])),
                        })
                    ]))]
    #[case::plus_simple(
                    vec![
                        token(TokenKind::NumberLiteral(1.into())),
                        token(TokenKind::Plus),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::ADD, Some(Shared::new(token(TokenKind::Plus)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::plus_with_identifiers(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::Plus),
                        token(TokenKind::Ident(SmolStr::new("y"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::ADD, Some(Shared::new(token(TokenKind::Plus)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y")))))))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::plus_error_missing_rhs(
                    vec![
                        token(TokenKind::NumberLiteral(1.into())),
                        token(TokenKind::Plus),
                        token(TokenKind::Eof)
                    ],
                    Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::lt_simple(
                    vec![
                        token(TokenKind::NumberLiteral(1.into())),
                        token(TokenKind::Lt),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::LT, Some(Shared::new(token(TokenKind::Lt)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::lte_simple(
                    vec![
                        token(TokenKind::NumberLiteral(1.into())),
                        token(TokenKind::Lte),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::LTE, Some(Shared::new(token(TokenKind::Lte)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::gt_simple(
                    vec![
                        token(TokenKind::NumberLiteral(3.into())),
                        token(TokenKind::Gt),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::GT, Some(Shared::new(token(TokenKind::Gt)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(3.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::gte_simple(
                    vec![
                        token(TokenKind::NumberLiteral(3.into())),
                        token(TokenKind::Gte),
                        token(TokenKind::NumberLiteral(2.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::GTE, Some(Shared::new(token(TokenKind::Gte)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(3.into()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::dict_empty(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::DICT, Some(Shared::new(token(TokenKind::LBrace)))),
                                    SmallVec::new(),
                                )),
                            })
                        ]))]
    #[case::dict_single_pair(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("key"))),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("value".to_owned())),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::DICT, Some(Shared::new(token(TokenKind::LBrace)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Call(
                                                IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("key")))))),
                                                smallvec![
                                                    Shared::new(Node {
                                                        token_id: 1.into(),
                                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("key", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("key")))))))),
                                                    }),
                                                    Shared::new(Node {
                                                        token_id: 2.into(),
                                                        expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                                                    }),
                                                ],
                                            )),
                                        }),
                                    ],
                                )),
                            })
                        ]))]
    #[case::dict_multiple_pairs(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("a"))),
                            token(TokenKind::Colon),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::Comma),
                            token(TokenKind::StringLiteral("b".to_owned())),
                            token(TokenKind::Colon),
                            token(TokenKind::BoolLiteral(true)),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::DICT, Some(Shared::new(token(TokenKind::LBrace)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Call(
                                                IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))),
                                                smallvec![
                                                    Shared::new(Node {
                                                        token_id: 1.into(),
                                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                                                    }),
                                                    Shared::new(Node {
                                                        token_id: 2.into(),
                                                        expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                                    }),
                                                ],
                                            )),
                                        }),
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Call(
                                                IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::StringLiteral("b".to_owned()))))),
                                                smallvec![
                                                    Shared::new(Node {
                                                        token_id: 3.into(),
                                                        expr: Shared::new(Expr::Literal(Literal::String("b".to_owned()))),
                                                    }),
                                                    Shared::new(Node {
                                                        token_id: 4.into(),
                                                        expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                                                    }),
                                                ],
                                            )),
                                        }),
                                    ],
                                )),
                            })
                        ]))]
    #[case::dict_trailing_comma(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("x"))),
                            token(TokenKind::Colon),
                            token(TokenKind::NumberLiteral(10.into())),
                            token(TokenKind::Comma),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::DICT, Some(Shared::new(token(TokenKind::LBrace)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Call(
                                                IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))),
                                                smallvec![
                                                    Shared::new(Node {
                                                        token_id: 1.into(),
                                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                                    }),
                                                    Shared::new(Node {
                                                        token_id: 2.into(),
                                                        expr: Shared::new(Expr::Literal(Literal::Number(10.into()))),
                                                    }),
                                                ],
                                            )),
                                        }),
                                    ],
                                )),
                            })
                        ]))]
    #[case::dict_unclosed(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("k"))),
                            token(TokenKind::Colon),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::Eof)
                        ],
                        Err(ParseError::ExpectedClosingBrace(token(TokenKind::Eof))))]
    #[case::dict_missing_colon(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("k"))),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Err(ParseError::UnexpectedToken(token(TokenKind::NumberLiteral(1.into())))))]
    #[case::dict_invalid_key(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("v".to_owned())),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Err(ParseError::UnexpectedToken(token(TokenKind::NumberLiteral(1.into())))))]
    #[case::attr_h_text(
        vec![
            token(TokenKind::Selector(".h.text".into())),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 3.into(),
                expr: Shared::new(Expr::Call(IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Selector(".h.text".into()))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Selector(Selector::Heading(None))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("text".to_owned()))),
                        }),

                    ],
                ))})]))]
    #[case::attr(
        vec![
            token(TokenKind::Selector(".list.checked".into())),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 3.into(),
                expr: Shared::new(Expr::Call(IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Selector(".list.checked".into()))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Selector(Selector::List(None, None))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("checked".to_owned()))),
                        }),

                    ],
                ))})]))]
    #[case::paren(
        vec![
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Plus),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RParen),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Paren(
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ADD, Some(Shared::new(token(TokenKind::Plus)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 3.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                }),
                            ],
                        )),
                    })
                )),
            })
        ]))]
    #[case::minus_simple(
        vec![
            token(TokenKind::NumberLiteral(5.into())),
            token(TokenKind::Minus),
            token(TokenKind::NumberLiteral(3.into())),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::SUB, Some(Shared::new(token(TokenKind::Minus)))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(5.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(3.into()))),
                        }),
                    ],
                )),
            })
        ]))]
    #[case::minus_with_identifiers(
        vec![
            token(TokenKind::Ident(SmolStr::new("a"))),
            token(TokenKind::Minus),
            token(TokenKind::Ident(SmolStr::new("b"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::SUB, Some(Shared::new(token(TokenKind::Minus)))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b")))))))),
                        }),
                    ],
                )),
            })
        ]))]
    #[case::slash_simple(
        vec![
            token(TokenKind::NumberLiteral(6.into())),
            token(TokenKind::Slash),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::DIV, Some(Shared::new(token(TokenKind::Slash)))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(6.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                        }),
                    ],
                )),
            })
        ]))]
    #[case::percent_simple(
            vec![
                token(TokenKind::NumberLiteral(10.into())),
                token(TokenKind::Percent),
                token(TokenKind::NumberLiteral(3.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::MOD, Some(Shared::new(token(TokenKind::Percent)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Literal(Literal::Number(10.into()))),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Shared::new(Expr::Literal(Literal::Number(3.into()))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::percent_with_identifiers(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Percent),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::MOD, Some(Shared::new(token(TokenKind::Percent)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b")))))))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::percent_error_missing_rhs(
            vec![
                token(TokenKind::NumberLiteral(10.into())),
                token(TokenKind::Percent),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::mul_simple(
            vec![
                token(TokenKind::NumberLiteral(3.into())),
                token(TokenKind::Asterisk),
                token(TokenKind::NumberLiteral(4.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::MUL, Some(Shared::new(token(TokenKind::Asterisk)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Literal(Literal::Number(3.into()))),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Shared::new(Expr::Literal(Literal::Number(4.into()))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::mul_with_identifiers(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::Asterisk),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::MUL, Some(Shared::new(token(TokenKind::Asterisk)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                            }),
                            Shared::new(Node {
                                token_id: 2.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b")))))))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::mul_error_missing_rhs(
            vec![
                token(TokenKind::NumberLiteral(5.into())),
                token(TokenKind::Asterisk),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::multiple_binary_operators(
            vec![
                token(TokenKind::NumberLiteral(1.into())),
                token(TokenKind::Asterisk),
                token(TokenKind::NumberLiteral(2.into())),
                token(TokenKind::Asterisk),
                token(TokenKind::NumberLiteral(3.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::MUL, Some(Shared::new(token(TokenKind::Asterisk)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::MUL, Some(Shared::new(token(TokenKind::Asterisk)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                        }),
                                    ],
                                )),
                            }),
                            Shared::new(Node {
                                token_id: 4.into(),
                                expr: Shared::new(Expr::Literal(Literal::Number(3.into()))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::multiple_binary_operators_eq(
            vec![
                token(TokenKind::NumberLiteral(1.into())),
                token(TokenKind::Plus),
                token(TokenKind::NumberLiteral(2.into())),
                token(TokenKind::EqEq),
                token(TokenKind::NumberLiteral(3.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::EQ, Some(Shared::new(token(TokenKind::EqEq)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::ADD, Some(Shared::new(token(TokenKind::Plus)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                        }),
                                    ],
                                )),
                            }),
                            Shared::new(Node {
                                token_id: 4.into(),
                                expr: Shared::new(Expr::Literal(Literal::Number(3.into()))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::multiple_and_operators(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::AND, Some(Shared::new(token(TokenKind::And)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::AND, Some(Shared::new(token(TokenKind::And)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b")))))))),
                                        }),
                                    ],
                                )),
                            }),
                            Shared::new(Node {
                                token_id: 4.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("c", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("c")))))))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::multiple_or_operators(
            vec![
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("y"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("z"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::OR, Some(Shared::new(token(TokenKind::Or)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::OR, Some(Shared::new(token(TokenKind::Or)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y")))))))),
                                        }),
                                    ],
                                )),
                            }),
                            Shared::new(Node {
                                token_id: 4.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("z", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("z")))))))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::and_or_mixed(
            vec![
                token(TokenKind::Ident(SmolStr::new("a"))),
                token(TokenKind::And),
                token(TokenKind::Ident(SmolStr::new("b"))),
                token(TokenKind::Or),
                token(TokenKind::Ident(SmolStr::new("c"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::OR, Some(Shared::new(token(TokenKind::Or)))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::AND, Some(Shared::new(token(TokenKind::And)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b")))))))),
                                        }),
                                    ],
                                )),
                            }),
                            Shared::new(Node {
                                token_id: 4.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("c", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("c")))))))),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::range_simple(
                vec![
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::RangeOp),
                    token(TokenKind::NumberLiteral(5.into())),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::RANGE, Some(Shared::new(token(TokenKind::RangeOp)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(5.into()))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::range_with_identifiers(
                vec![
                    token(TokenKind::Ident(SmolStr::new("start"))),
                    token(TokenKind::RangeOp),
                    token(TokenKind::Ident(SmolStr::new("end"))),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::RANGE, Some(Shared::new(token(TokenKind::RangeOp)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("start", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("start")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("end", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("end")))))))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::range_error_missing_rhs(
                vec![
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::RangeOp),
                    token(TokenKind::Eof)
                ],
                Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::args_missing_rparen(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::StringLiteral("bar".to_owned())),
                    // Missing RParen
                    token(TokenKind::Eof)
                ],
                Err(ParseError::ExpectedClosingParen(token(TokenKind::Eof)))
            )]
    #[case::args_unexpected_token(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::Colon), // Invalid token in args
                    token(TokenKind::RParen),
                    token(TokenKind::Eof)
                ],
                Err(ParseError::ExpectedClosingParen(token(TokenKind::Colon)))
            )]
    #[case::args_leading_comma(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::Comma),
                    token(TokenKind::Ident(SmolStr::new("bar"))),
                    token(TokenKind::RParen),
                    token(TokenKind::Eof)
                ],
                Err(ParseError::UnexpectedToken(token(TokenKind::Comma)))
            )]
    #[case::args_double_comma(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::Ident(SmolStr::new("bar"))),
                    token(TokenKind::Comma),
                    token(TokenKind::Comma),
                    token(TokenKind::Ident(SmolStr::new("baz"))),
                    token(TokenKind::RParen),
                    token(TokenKind::Eof)
                ],
                Err(ParseError::UnexpectedToken(token(TokenKind::Comma)))
            )]
    #[case::binary_operator_chaining(
                vec![
                    token(TokenKind::NumberLiteral(2.into())),
                    token(TokenKind::Gt),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::Or),
                    token(TokenKind::NumberLiteral(2.into())),
                    token(TokenKind::Gt),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::OR, Some(Shared::new(token(TokenKind::Or)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Call(
                                        IdentWithToken::new_with_token(constants::GT, Some(Shared::new(token(TokenKind::Gt)))),
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 0.into(),
                                                expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                            }),
                                            Shared::new(Node {
                                                token_id: 2.into(),
                                                expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                            }),
                                        ],
                                    )),
                                }),
                                Shared::new(Node {
                                    token_id: 5.into(),
                                    expr: Shared::new(Expr::Call(
                                        IdentWithToken::new_with_token(constants::GT, Some(Shared::new(token(TokenKind::Gt)))),
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 4.into(),
                                                expr: Shared::new(Expr::Literal(Literal::Number(2.into()))),
                                            }),
                                            Shared::new(Node {
                                                token_id: 6.into(),
                                                expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                            }),
                                        ],
                                    )),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::not_simple(
                vec![
                    token(TokenKind::Not),
                    token(TokenKind::BoolLiteral(false)),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::NOT, Some(Shared::new(token(TokenKind::Not)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Bool(false))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::not_with_expr(
                vec![
                    token(TokenKind::Not),
                    token(TokenKind::Ident(SmolStr::new("x"))),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::NOT, Some(Shared::new(token(TokenKind::Not)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::bracket_access_with_number(
                vec![
                    token(TokenKind::Ident(SmolStr::new("arr"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(5.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arr")))))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("arr", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arr")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(5.into()))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::bracket_access_with_string(
                vec![
                    token(TokenKind::Ident(SmolStr::new("dict"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("key".to_owned())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("dict")))))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(constants::DICT, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("dict")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("key".to_owned()))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::bracket_access_error_missing_rbracket(
                vec![
                    token(TokenKind::Ident(SmolStr::new("arr"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(5.into())),
                    token(TokenKind::Eof)
                ],
                Err(ParseError::ExpectedClosingBracket(token(TokenKind::Eof))))]
    #[case::slice_access_with_numbers(
                vec![
                    token(TokenKind::Ident(SmolStr::new("arr"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(1.into())),
                    token(TokenKind::Colon),
                    token(TokenKind::NumberLiteral(3.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 5.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::SLICE, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arr")))))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("arr", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arr")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(3.into()))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::slice_access_with_variables(
                vec![
                    token(TokenKind::Ident(SmolStr::new("items"))),
                    token(TokenKind::LBracket),
                    token(TokenKind::Ident(SmolStr::new("start"))),
                    token(TokenKind::Colon),
                    token(TokenKind::Ident(SmolStr::new("end"))),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 5.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::SLICE, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("items")))))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("items", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("items")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("start", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("start")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("end", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("end")))))))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::not_error_missing_rhs(
                vec![
                    token(TokenKind::Not),
                    token(TokenKind::Eof)
                ],
                Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::break_(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break),
                        })
                    ]))]
    #[case::continue_(
                    vec![
                        token(TokenKind::Continue),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Continue),
                        })
                    ]))]
    #[case::if_with_comment_before_elif(
                        vec![
                            token(TokenKind::If),
                            token(TokenKind::LParen),
                            token(TokenKind::BoolLiteral(true)),
                            token(TokenKind::RParen),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("true branch".to_owned())),
                            token(TokenKind::Comment("comment before elif".to_owned())),
                            token(TokenKind::Elif),
                            token(TokenKind::LParen),
                            token(TokenKind::BoolLiteral(false)),
                            token(TokenKind::RParen),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("elif branch".to_owned())),
                            token(TokenKind::Else),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("else branch".to_owned())),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 11.into(),
                                expr: Shared::new(Expr::If(smallvec![
                                    (
                                        Some(Shared::new(Node {
                                            token_id: 1.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                                        })),
                                        Shared::new(Node {
                                            token_id: 3.into(),
                                            expr: Shared::new(Expr::Literal(Literal::String("true branch".to_owned()))),
                                        })
                                    ),
                                    (
                                        Some(Shared::new(Node {
                                            token_id: 5.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Bool(false))),

                                        })),
                                        Shared::new(Node {
                                            token_id: 7.into(),
                                            expr: Shared::new(Expr::Literal(Literal::String("elif branch".to_owned()))),
                                        })
                                    ),
                                    (
                                        None,
                                        Shared::new(Node {
                                            token_id: 10.into(),
                                            expr: Shared::new(Expr::Literal(Literal::String("else branch".to_owned()))),
                                        })
                                    )
                                ])),
                            })
                        ])
                    )]
    #[rstest]
    #[case::if_with_multiple_comments_and_elifs(
                        vec![
                            token(TokenKind::If),
                            token(TokenKind::LParen),
                            token(TokenKind::BoolLiteral(true)),
                            token(TokenKind::RParen),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("true branch".to_owned())),
                            token(TokenKind::Comment("first comment".to_owned())),
                            token(TokenKind::Elif),
                            token(TokenKind::LParen),
                            token(TokenKind::BoolLiteral(false)),
                            token(TokenKind::RParen),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("first elif branch".to_owned())),
                            token(TokenKind::Comment("second comment".to_owned())),
                            token(TokenKind::Elif),
                            token(TokenKind::LParen),
                            token(TokenKind::BoolLiteral(true)),
                            token(TokenKind::RParen),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("second elif branch".to_owned())),
                            token(TokenKind::Else),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("else branch".to_owned())),
                            token(TokenKind::Eof)
                        ],
                        Ok(vec![
                            Shared::new(Node {
                                token_id: 15.into(),
                                expr: Shared::new(Expr::If(smallvec![
                                    (
                                        Some(Shared::new(Node {
                                            token_id: 1.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                                        })),
                                        Shared::new(Node {
                                            token_id: 3.into(),
                                            expr: Shared::new(Expr::Literal(Literal::String("true branch".to_owned()))),
                                        })
                                    ),
                                    (
                                        Some(Shared::new(Node {
                                            token_id: 5.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Bool(false))),
                                        })),
                                        Shared::new(Node {
                                            token_id: 7.into(),
                                            expr: Shared::new(Expr::Literal(Literal::String("first elif branch".to_owned()))),
                                        })
                                    ),
                                    (
                                        Some(Shared::new(Node {
                                            token_id: 9.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                                        })),
                                        Shared::new(Node {
                                            token_id: 11.into(),
                                            expr: Shared::new(Expr::Literal(Literal::String("second elif branch".to_owned()))),
                                        })
                                    ),
                                    (
                                        None,
                                        Shared::new(Node {
                                            token_id: 14.into(),
                                            expr: Shared::new(Expr::Literal(Literal::String("else branch".to_owned()))),
                                        })
                                    )
                                ])),
                            })
                        ])
                    )]
    #[case::self_bracket_access_with_number(
                vec![
                    token(TokenKind::Self_),
                    token(TokenKind::LBracket),
                    token(TokenKind::NumberLiteral(5.into())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Self_)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Self_),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::Number(5.into()))),
                                }),
                            ],
                        )),
                    })
                ]))]
    #[case::self_bracket_access_with_string(
                vec![
                    token(TokenKind::Self_),
                    token(TokenKind::LBracket),
                    token(TokenKind::StringLiteral("key".to_owned())),
                    token(TokenKind::RBracket),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Self_)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Self_),
                                }),
                                Shared::new(Node {
                                    token_id: 1.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("key".to_owned()))),
                                }),
                            ],
                        )),
                    })
                ]))]
    // Test function call followed by index access (e.g., foo()[0])
    #[case::function_call_with_index_access(
        vec![
            token(TokenKind::Ident(SmolStr::new("foo"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("foo")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token("foo", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("foo")))))),
                                SmallVec::new(),
                            )),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(0.into()))),
                        }),
                    ],
                )),
            })
        ]))]
    // Test function call with arguments followed by index access
    #[case::function_call_with_args_and_index_access(
        vec![
            token(TokenKind::Ident(SmolStr::new("bar"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("arg".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::StringLiteral("key".to_owned())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 1.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("bar")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token("bar", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("bar")))))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("arg".to_owned()))),
                                    })
                                ],
                            )),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("key".to_owned()))),
                        }),
                    ],
                )),
            })
        ]))]
    // Test chained index access on function call result
    #[case::function_call_with_chained_index_access(
        vec![
            token(TokenKind::Ident(SmolStr::new("baz"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RBracket),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("baz")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("baz")))))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Call(
                                            IdentWithToken::new_with_token("baz", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("baz")))))),
                                            SmallVec::new(),
                                        )),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Literal(Literal::Number(0.into()))),
                                    }),
                                ],
                            )),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(1.into()))),
                        }),
                    ],
                )),
            })
        ]))]
    fn test_parse(#[case] input: Vec<Token>, #[case] expected: Result<Program, ParseError>) {
        let arena = Arena::new(10);
        let tokens: Vec<Shared<Token>> = input.into_iter().map(Shared::new).collect();
        let result = Parser::new(
            tokens.iter(),
            Shared::new(SharedCell::new(arena)),
            Module::TOP_LEVEL_MODULE_ID,
        )
        .parse();

        match (&result, &expected) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual.len(), expected.len());
                let actual_exprs: Vec<_> = actual.iter().map(|a| &*a.expr).collect();
                let expected_exprs: Vec<_> = expected.iter().map(|e| &*e.expr).collect();
                assert_eq!(actual_exprs, expected_exprs);
            }
            (Err(actual), Err(expected)) => {
                assert_eq!(actual, expected);
            }
            _ => {
                panic!("Mismatch: actual = {:?}, expected = {:?}", result, expected)
            }
        }
    }

    #[rstest]
    #[case::heading(".h1", Selector::Heading(Some(1)))]
    #[case::heading_h3(".h3", Selector::Heading(Some(3)))]
    #[case::blockquote(".>", Selector::Blockquote)]
    #[case::blockquote_full(".blockquote", Selector::Blockquote)]
    #[case::footnote(".^", Selector::Footnote)]
    #[case::footnote_full(".footnote", Selector::Footnote)]
    #[case::mdx_jsx_flow(".mdx_jsx_flow_element", Selector::MdxJsxFlowElement)]
    #[case::mdx_jsx_flow_short(".<", Selector::MdxJsxFlowElement)]
    #[case::emphasis(".**", Selector::Emphasis)]
    #[case::emphasis_full(".emphasis", Selector::Emphasis)]
    #[case::math(".$$", Selector::Math)]
    #[case::math_full(".math", Selector::Math)]
    #[case::horizontal_rule(".---", Selector::HorizontalRule)]
    #[case::horizontal_rule_alt(".***", Selector::HorizontalRule)]
    #[case::horizontal_rule_full(".horizontal_rule", Selector::HorizontalRule)]
    #[case::mdx_text_expression(".{}", Selector::MdxTextExpression)]
    #[case::mdx_text_expression_full(".mdx_text_expression", Selector::MdxTextExpression)]
    #[case::footnote_ref(".[^]", Selector::FootnoteRef)]
    #[case::footnote_ref_full(".footnote_ref", Selector::FootnoteRef)]
    #[case::definition(".definition", Selector::Definition)]
    #[case::break_selector(".break", Selector::Break)]
    #[case::delete(".delete", Selector::Delete)]
    #[case::html(".<>", Selector::Html)]
    #[case::html_full(".html", Selector::Html)]
    #[case::image(".image", Selector::Image)]
    #[case::image_ref(".image_ref", Selector::ImageRef)]
    #[case::code(".code", Selector::Code(None))]
    #[case::code_inline(".code_inline", Selector::InlineCode)]
    #[case::math_inline(".math_inline", Selector::InlineMath)]
    #[case::link(".link", Selector::Link)]
    #[case::link_ref(".link_ref", Selector::LinkRef)]
    #[case::list(".list", Selector::List(None, None))]
    #[case::toml(".toml", Selector::Toml)]
    #[case::strong(".strong", Selector::Strong)]
    #[case::yaml(".yaml", Selector::Yaml)]
    #[case::text(".text", Selector::Text)]
    #[case::mdx_js_esm(".mdx_js_esm", Selector::MdxJsEsm)]
    #[case::mdx_jsx_text_element(".mdx_jsx_text_element", Selector::MdxJsxTextElement)]
    #[case::mdx_flow_expression(".mdx_flow_expression", Selector::MdxFlowExpression)]
    fn test_parse_selector(#[case] selector_str: &str, #[case] expected_selector: Selector) {
        let arena = Arena::new(10);
        let token = Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(selector_str)),
            module_id: 1.into(),
        });

        let tokens = [
            Shared::clone(&token),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }),
        ];

        let result = Parser::new(
            tokens.iter(),
            Shared::new(SharedCell::new(arena)),
            Module::TOP_LEVEL_MODULE_ID,
        )
        .parse();

        match result {
            Ok(program) => {
                assert_eq!(program.len(), 1);
                if let Expr::Selector(selector) = &*program[0].expr {
                    assert_eq!(*selector, expected_selector);
                } else {
                    panic!("Expected Selector expression, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }

    #[rstest]
    #[case(".code", "rust", Selector::Code(Some(SmolStr::new("rust"))))]
    #[case(".h", "2", Selector::Heading(Some(2)))]
    #[case(".list", "3", Selector::List(Some(3), None))]
    fn test_parse_selector_with_args(
        #[case] selector_str: &str,
        #[case] arg: &str,
        #[case] expected_selector: Selector,
    ) {
        let arena = Arena::new(10);
        let tokens = [
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Selector(SmolStr::new(selector_str)),
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::LParen,
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: if selector_str == ".code" {
                    TokenKind::StringLiteral(arg.to_owned())
                } else {
                    TokenKind::NumberLiteral(arg.parse::<f64>().unwrap().into())
                },
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::RParen,
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }),
        ];

        let result = Parser::new(
            tokens.iter(),
            Shared::new(SharedCell::new(arena)),
            Module::TOP_LEVEL_MODULE_ID,
        )
        .parse();

        match result {
            Ok(program) => {
                assert_eq!(program.len(), 1);
                if let Expr::Selector(selector) = &*program[0].expr {
                    assert_eq!(*selector, expected_selector);
                } else {
                    panic!("Expected Selector expression, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }

    #[rstest]
    #[case(".", Some(1), None, Selector::List(Some(1), None))]
    #[case(".", Some(2), Some(3), Selector::Table(Some(2), Some(3)))]
    fn test_parse_array_selector(
        #[case] selector_str: &str,
        #[case] first_idx: Option<usize>,
        #[case] second_idx: Option<usize>,
        #[case] expected_selector: Selector,
    ) {
        let arena = Arena::new(10);
        let mut tokens = vec![
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Selector(SmolStr::new(selector_str)),
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::LBracket,
                module_id: 1.into(),
            }),
        ];

        if let Some(idx) = first_idx {
            tokens.push(Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::NumberLiteral(idx.into()),
                module_id: 1.into(),
            }));
        }

        tokens.push(Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::RBracket,
            module_id: 1.into(),
        }));

        if second_idx.is_some() {
            tokens.push(Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::LBracket,
                module_id: 1.into(),
            }));

            if let Some(idx) = second_idx {
                tokens.push(Shared::new(Token {
                    range: Range::default(),
                    kind: TokenKind::NumberLiteral(idx.into()),
                    module_id: 1.into(),
                }));
            }

            tokens.push(Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::RBracket,
                module_id: 1.into(),
            }));
        }

        tokens.push(Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        }));

        let result = Parser::new(
            tokens.iter(),
            Shared::new(SharedCell::new(arena)),
            Module::TOP_LEVEL_MODULE_ID,
        )
        .parse();

        match result {
            Ok(program) => {
                assert_eq!(program.len(), 1);
                if let Expr::Selector(selector) = &*program[0].expr {
                    assert_eq!(*selector, expected_selector);
                } else {
                    panic!("Expected Selector expression, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }

    #[test]
    fn test_parse_env() {
        unsafe { std::env::set_var("MQ_TEST_VAR", "test_value") };

        let arena = Arena::new(10);
        let tokens = [
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Env("MQ_TEST_VAR".into()),
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }),
        ];

        let result = Parser::new(
            tokens.iter(),
            Shared::new(SharedCell::new(arena)),
            Module::TOP_LEVEL_MODULE_ID,
        )
        .parse();

        match result {
            Ok(program) => {
                assert_eq!(program.len(), 1);
                if let Expr::Literal(Literal::String(value)) = &*program[0].expr {
                    assert_eq!(value, "test_value");
                } else {
                    panic!("Expected String literal, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }

    #[test]
    fn test_parse_env_not_found() {
        let arena = Arena::new(10);
        let token = Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Env("MQ_NONEXISTENT_VAR".into()),
            module_id: 1.into(),
        });

        let tokens = [
            Shared::clone(&token),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }),
        ];

        let result = Parser::new(
            tokens.iter(),
            Shared::new(SharedCell::new(arena)),
            Module::TOP_LEVEL_MODULE_ID,
        )
        .parse();

        assert!(matches!(
            result,
            Err(ParseError::EnvNotFound(_, var)) if var == "MQ_NONEXISTENT_VAR"
        ));
    }

    #[test]
    fn test_parse_env_in_arguments() {
        unsafe { std::env::set_var("MQ_ARG_TEST", "env_arg_value") };

        let arena = Arena::new(10);
        let tokens = [
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Ident(SmolStr::new("function")),
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::LParen,
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Env("MQ_ARG_TEST".into()),
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::RParen,
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }),
        ];

        let result = Parser::new(
            tokens.iter(),
            Shared::new(SharedCell::new(arena)),
            Module::TOP_LEVEL_MODULE_ID,
        )
        .parse();

        match result {
            Ok(program) => {
                assert_eq!(program.len(), 1);
                if let Expr::Call(ident, args) = &*program[0].expr {
                    assert_eq!(ident.name, "function".into());
                    assert_eq!(args.len(), 1);
                    if let Expr::Literal(Literal::String(value)) = &*args[0].expr {
                        assert_eq!(value, "env_arg_value");
                    } else {
                        panic!(
                            "Expected String literal in argument, got {:?}",
                            args[0].expr
                        );
                    }
                } else {
                    panic!("Expected Call expression, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }

    #[rstest]
    #[case::h_text(".h.text", "h", "text")]
    #[case::h1_text(".h1.text", "h1", "text")]
    #[case::code_html(".code.html", "code", "html")]
    #[case::text_markdown(".text.markdown", "text", "markdown")]
    fn test_parse_selector_with_attribute(
        #[case] selector_str: &str,
        #[case] base_selector: &str,
        #[case] attribute: &str,
    ) {
        let arena = Arena::new(10);
        let token = Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Selector(SmolStr::new(selector_str)),
            module_id: 1.into(),
        });

        let tokens = [
            Shared::clone(&token),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }),
        ];

        let result = Parser::new(
            tokens.iter(),
            Shared::new(SharedCell::new(arena)),
            Module::TOP_LEVEL_MODULE_ID,
        )
        .parse();

        match result {
            Ok(program) => {
                assert_eq!(program.len(), 1);
                if let Expr::Call(ident, args) = &*program[0].expr {
                    // Should be transformed to attr(base_selector, "attribute")
                    assert_eq!(ident.name, "attr".into());
                    assert_eq!(args.len(), 2);

                    // First argument should be the base selector
                    if let Expr::Selector(selector) = &*args[0].expr {
                        match base_selector {
                            "h" => assert_eq!(*selector, Selector::Heading(None)),
                            "h1" => assert_eq!(*selector, Selector::Heading(Some(1))),
                            "code" => assert_eq!(*selector, Selector::Code(None)),
                            "text" => assert_eq!(*selector, Selector::Text),
                            _ => panic!("Unexpected base selector: {}", base_selector),
                        }
                    } else {
                        panic!(
                            "Expected Selector expression in first argument, got {:?}",
                            args[0].expr
                        );
                    }

                    // Second argument should be the attribute string
                    if let Expr::Literal(Literal::String(attr_str)) = &*args[1].expr {
                        assert_eq!(attr_str, attribute);
                    } else {
                        panic!(
                            "Expected String literal in second argument, got {:?}",
                            args[1].expr
                        );
                    }
                } else {
                    panic!("Expected Call expression, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }
}
