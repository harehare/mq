use crate::arena::Arena;
use crate::ast::node::{IdentWithToken, MatchArm, Pattern};
use crate::error::syntax::SyntaxError;
use crate::lexer::Lexer;
use crate::lexer::token::{Token, TokenKind};
use crate::module::ModuleId;
use crate::selector::Selector;
use crate::{Ident, Shared, lexer};
use smallvec::{SmallVec, smallvec};
use smol_str::SmolStr;
use std::iter::Peekable;

use super::constants;
use super::node::{AccessTarget, Args, Branches, Expr, Literal, Node, Param, Params};
use super::{Program, TokenId};

type IfExpr = (Option<Shared<Node>>, Shared<Node>);

#[derive(Debug)]
struct ArrayIndex(Option<usize>);

pub struct Parser<'a, 'alloc> {
    tokens: Peekable<core::slice::Iter<'a, Shared<Token>>>,
    token_arena: &'alloc mut Arena<Shared<Token>>,
    module_id: ModuleId,
}

impl<'a, 'alloc> Parser<'a, 'alloc> {
    pub fn new(
        tokens: core::slice::Iter<'a, Shared<Token>>,
        token_arena: &'alloc mut Arena<Shared<Token>>,
        module_id: ModuleId,
    ) -> Self {
        Self {
            tokens: tokens.peekable(),
            token_arena,
            module_id,
        }
    }

    pub fn parse(&mut self) -> Result<Program, SyntaxError> {
        self.parse_program(true)
    }

    fn parse_program(&mut self, root: bool) -> Result<Program, SyntaxError> {
        let mut asts = Vec::with_capacity(64);

        // Initial check for invalid starting tokens in a program.
        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::Pipe | TokenKind::SemiColon | TokenKind::End => {
                    return Err(SyntaxError::UnexpectedToken((***token).clone()));
                }
                _ => {}
            },
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::Pipe => continue, // Skip pipes.
                TokenKind::Eof => break,     // End of file terminates the program.
                TokenKind::SemiColon | TokenKind::End => {
                    // Semicolons and 'end' keyword terminate sub-programs (e.g., in 'def', 'fn').
                    // In the root program, after a semicolon or 'end', the parser checks if the next token is EOF.
                    // If the next token is not EOF, it returns an error for the unexpected token.
                    if root && let Some(token) = self.tokens.peek() {
                        if let TokenKind::Eof = &token.kind {
                            break;
                        } else {
                            return Err(SyntaxError::UnexpectedToken((***token).clone()));
                        }
                    }
                    // For non-root programs (e.g. function bodies), a semicolon/end explicitly ends the program.
                    break;
                }
                TokenKind::Nodes if root => {
                    let ast = self.parse_all_nodes(token)?;
                    asts.push(ast);
                }
                TokenKind::Nodes => {
                    return Err(SyntaxError::UnexpectedToken((**token).clone()));
                }
                TokenKind::NewLine | TokenKind::Tab(_) | TokenKind::Whitespace(_) => unreachable!(),
                _ => {
                    let ast = self.parse_expr(token)?;
                    asts.push(ast);
                }
            }
        }

        if asts.is_empty() {
            return Err(SyntaxError::UnexpectedEOFDetected(self.module_id));
        }

        Ok(asts)
    }

    #[inline(always)]
    fn parse_expr(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        self.parse_equality_expr(token)
    }

    #[inline(always)]
    fn binary_op_precedence(kind: &TokenKind) -> u8 {
        match kind {
            TokenKind::Equal => 0,
            TokenKind::Or => 1,
            TokenKind::And => 2,
            TokenKind::EqEq | TokenKind::NeEq | TokenKind::Gt | TokenKind::Gte | TokenKind::Lt | TokenKind::Lte => 3,
            TokenKind::Plus | TokenKind::Minus => 4,
            TokenKind::Asterisk | TokenKind::Slash | TokenKind::Percent => 5,
            TokenKind::RangeOp | TokenKind::Coalesce => 6,
            _ => 0,
        }
    }

    fn binary_op_function_name(kind: &TokenKind) -> &'static str {
        match kind {
            TokenKind::Equal => constants::ASSIGN,
            TokenKind::And => constants::AND,
            TokenKind::Asterisk => constants::MUL,
            TokenKind::Coalesce => constants::COALESCE,
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

    fn parse_binary_op(parser: &mut Parser, min_prec: u8, mut lhs: Shared<Node>) -> Result<Shared<Node>, SyntaxError> {
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
            let operator_token_id = parser.token_arena.alloc(Shared::clone(operator_token));

            let rhs_token = match parser.tokens.next() {
                Some(t) => t,
                None => return Err(SyntaxError::UnexpectedEOFDetected(parser.module_id)),
            };
            let mut rhs = parser.parse_primary_expr(rhs_token)?;

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

            lhs = match kind {
                TokenKind::Equal => match &*lhs.expr {
                    Expr::Ident(ident) => Shared::new(Node {
                        token_id: operator_token_id,
                        expr: Shared::new(Expr::Assign(ident.clone(), rhs)),
                    }),
                    _ => {
                        return Err(SyntaxError::InvalidAssignmentTarget(
                            (*parser.token_arena[lhs.token_id]).clone(),
                        ));
                    }
                },
                TokenKind::And => Shared::new(Node {
                    token_id: operator_token_id,
                    expr: Shared::new(Expr::And(lhs, rhs)),
                }),
                TokenKind::Or => Shared::new(Node {
                    token_id: operator_token_id,
                    expr: Shared::new(Expr::Or(lhs, rhs)),
                }),
                _ => Shared::new(Node {
                    token_id: operator_token_id,
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(
                            Self::binary_op_function_name(kind),
                            Some(Shared::clone(operator_token)),
                        ),
                        smallvec![lhs, rhs],
                    )),
                }),
            };
        }

        Ok(lhs)
    }

    fn parse_equality_expr(&mut self, initial_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let lhs = self.parse_primary_expr(initial_token)?;
        Self::parse_binary_op(self, 0, lhs) // Start from precedence 0 to include assignment
    }

    fn parse_primary_expr(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Selector(_) => self.parse_selector(token),
            TokenKind::Let => self.parse_let(token),
            TokenKind::Var => self.parse_var(token),
            TokenKind::Def => self.parse_def(token),
            TokenKind::Macro => self.parse_macro(token),
            TokenKind::Do => self.parse_block(token),
            TokenKind::Fn => self.parse_fn(token),
            TokenKind::While => self.parse_while(token),
            TokenKind::Loop => self.parse_loop(token),
            TokenKind::Foreach => self.parse_foreach(token),
            TokenKind::Module => self.parse_module(token),
            TokenKind::Try => self.parse_try(token),
            TokenKind::Quote => self.parse_quote(token),
            TokenKind::Unquote => self.parse_unquote(token),
            TokenKind::If => self.parse_if(token),
            TokenKind::Match => self.parse_match(token),
            TokenKind::InterpolatedString(_) => self.parse_interpolated_string(token),
            TokenKind::Include => self.parse_include(token),
            TokenKind::Import => self.parse_import(token),
            TokenKind::Self_ => self.parse_self(token),
            TokenKind::Break => self.parse_break(token),
            TokenKind::Continue => self.parse_continue(token),
            TokenKind::Ident(name) => self.parse_ident(name, token),
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
            TokenKind::Colon => self.parse_symbol(token),
            TokenKind::Eof => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(SyntaxError::UnexpectedToken((**token).clone())),
        }
    }

    fn parse_module(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Module => match self.tokens.peek() {
                Some(_) => {
                    let ident_token = self
                        .tokens
                        .next()
                        .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?;

                    self.consume_colon_or_do();

                    let program = self.parse_program(false)?;

                    // Only allow 'let', 'def', or 'module' at the top-level of a module block
                    for node in &program {
                        match &*node.expr {
                            Expr::Let(_, _) | Expr::Def(_, _, _) | Expr::Module(_, _) | Expr::Import(_) => {}
                            _ => {
                                return Err(SyntaxError::UnexpectedToken((*self.token_arena[node.token_id]).clone()));
                            }
                        }
                    }

                    Ok(Shared::new(Node {
                        token_id: self.token_arena.alloc(Shared::clone(token)),
                        expr: Shared::new(Expr::Module(
                            IdentWithToken::new_with_token(
                                match &ident_token.kind {
                                    TokenKind::Ident(name) => name,
                                    _ => {
                                        return Err(SyntaxError::UnexpectedToken((**ident_token).clone()));
                                    }
                                },
                                Some(Shared::clone(ident_token)),
                            ),
                            program.iter().map(Shared::clone).collect(),
                        )),
                    }))
                }
                None => Err(SyntaxError::UnexpectedToken((**token).clone())),
            },
            _ => Err(SyntaxError::UnexpectedToken((**token).clone())),
        }
    }

    fn parse_symbol(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Colon => {
                let next_token = match self.tokens.next() {
                    Some(t) => t,
                    None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                };
                match &next_token.kind {
                    TokenKind::Ident(name) => Ok(Shared::new(Node {
                        token_id: self.token_arena.alloc(Shared::clone(token)),
                        expr: Shared::new(Expr::Literal(Literal::Symbol(Ident::new(name)))),
                    })),
                    TokenKind::StringLiteral(s) => Ok(Shared::new(Node {
                        token_id: self.token_arena.alloc(Shared::clone(token)),
                        expr: Shared::new(Expr::Literal(Literal::Symbol(Ident::new(s)))),
                    })),
                    _ => Err(SyntaxError::UnexpectedToken((**next_token).clone())),
                }
            }
            _ => Err(SyntaxError::UnexpectedToken((**token).clone())),
        }
    }

    fn parse_paren(&mut self, lparen_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(lparen_token));
        let expr_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        let expr_node = self.parse_expr(expr_token)?;

        self.next_token(|token_kind| matches!(token_kind, TokenKind::RParen))?;

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Paren(expr_node)),
        }))
    }

    fn parse_not(&mut self, not_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(not_token));

        let expr_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        if !matches!(
            expr_token.kind,
            TokenKind::BoolLiteral(_)
                | TokenKind::StringLiteral(_)
                | TokenKind::NumberLiteral(_)
                | TokenKind::If
                | TokenKind::Foreach
                | TokenKind::LBrace
                | TokenKind::LBracket
                | TokenKind::While
                | TokenKind::Loop
                | TokenKind::Match
                | TokenKind::Self_
                | TokenKind::Selector(_)
                | TokenKind::Env(_)
                | TokenKind::Ident(_)
        ) {
            return Err(SyntaxError::UnexpectedToken((**expr_token).clone()));
        }

        let expr_node = self.parse_primary_expr(expr_token)?;

        // Convert ! to not() function call
        let not_ident = IdentWithToken::new_with_token(constants::NOT, Some(Shared::clone(not_token)));
        let args = smallvec![expr_node];

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Call(not_ident, args)),
        }))
    }

    fn parse_negate(&mut self, minus_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(minus_token));

        let expr_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        if !matches!(
            expr_token.kind,
            TokenKind::NumberLiteral(_)
                | TokenKind::If
                | TokenKind::Foreach
                | TokenKind::LBrace
                | TokenKind::LBracket
                | TokenKind::While
                | TokenKind::Loop
                | TokenKind::Match
                | TokenKind::Self_
                | TokenKind::Env(_)
                | TokenKind::Ident(_)
        ) {
            return Err(SyntaxError::UnexpectedToken((**expr_token).clone()));
        }

        let expr_node = self.parse_primary_expr(expr_token)?;
        let negate_ident = IdentWithToken::new_with_token(constants::NEGATE, Some(Shared::clone(minus_token)));
        let args = smallvec![expr_node];

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Call(negate_ident, args)),
        }))
    }

    fn parse_dict(&mut self, lbrace_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(lbrace_token));
        let mut pairs = SmallVec::new();

        loop {
            match self.tokens.peek() {
                Some(token) if token.kind == TokenKind::RBrace => {
                    self.tokens.next();
                    break;
                }
                None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                _ => {}
            }

            // Parse key
            let key_token = match self.tokens.next() {
                Some(t) => t,
                None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            };

            let key_node = match &key_token.kind {
                TokenKind::Ident(name) => Shared::new(Node {
                    token_id: self.token_arena.alloc(Shared::clone(key_token)),
                    expr: Shared::new(Expr::Literal(Literal::Symbol(Ident::new(name)))),
                }),
                TokenKind::StringLiteral(s) => Shared::new(Node {
                    token_id: self.token_arena.alloc(Shared::clone(key_token)),
                    expr: Shared::new(Expr::Literal(Literal::String(s.clone()))),
                }),
                _ => {
                    return Err(SyntaxError::UnexpectedToken((**key_token).clone()));
                }
            };

            // Expect Colon
            match self.tokens.next() {
                Some(token) if token.kind == TokenKind::Colon => {}
                Some(token) => return Err(SyntaxError::UnexpectedToken((**token).clone())),
                None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            }

            // Parse value
            let value_token = match self.tokens.next() {
                Some(t) => t,
                None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            };
            let value_node = self.parse_expr(value_token)?;

            pairs.push(Shared::new(Node {
                token_id,
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::clone(key_token))),
                    smallvec![key_node, value_node],
                )),
            }));

            // Peek for Comma or RBrace
            match self.tokens.peek() {
                Some(token) if token.kind == TokenKind::Comma => {
                    self.tokens.next(); // Consume Comma
                    // Check for trailing comma followed by RBrace
                    if let Some(next_token) = self.tokens.peek()
                        && next_token.kind == TokenKind::RBrace
                    {
                        self.tokens.next(); // Consume RBrace
                        break;
                    }
                }
                Some(token) if token.kind == TokenKind::RBrace => {
                    self.tokens.next(); // Consume RBrace
                    break;
                }
                Some(token) => {
                    return Err(SyntaxError::ExpectedClosingBrace((***token).clone()));
                }
                None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            }
        }

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Call(
                IdentWithToken::new_with_token(constants::DICT, Some(Shared::clone(lbrace_token))),
                pairs,
            )),
        }))
    }

    fn parse_env(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Env(s) => Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(token)),
                expr: std::env::var(s)
                    .map_err(|_| SyntaxError::EnvNotFound((**token).clone(), SmolStr::new(s)))
                    .map(|s| Shared::new(Expr::Literal(Literal::String(s.to_owned()))))?,
            })),
            TokenKind::Eof => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(SyntaxError::UnexpectedToken((**token).clone())),
        }
    }

    fn parse_attribute_access(
        &mut self,
        base_node: Shared<Node>,
        token_id: TokenId,
    ) -> Result<Shared<Node>, SyntaxError> {
        let selector_token = match self.tokens.peek() {
            Some(t) => Shared::clone(t),
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        if let TokenKind::Selector(selector) = &selector_token.kind
            && selector.len() > 1
        {
            if !Selector::try_from(&*selector_token)
                .map_err(SyntaxError::UnknownSelector)?
                .is_attribute_selector()
            {
                return Err(SyntaxError::UnexpectedToken((*selector_token).clone()));
            }

            let attribute_name = &selector[1..]; // Skip the leading '.'
            let attr_literal_token_id = self.token_arena.alloc(Shared::clone(&selector_token));
            let attr_literal = Shared::new(Node {
                token_id: attr_literal_token_id,
                expr: Shared::new(Expr::Literal(Literal::String(attribute_name.to_string()))),
            });

            self.tokens.next(); // Consume selector token

            Ok(Shared::new(Node {
                token_id: attr_literal_token_id,
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::ATTR, Some(Shared::clone(&self.token_arena[token_id]))),
                    smallvec![base_node, attr_literal],
                )),
            }))
        } else {
            Ok(base_node)
        }
    }

    fn parse_self(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(token));
        let self_node = Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Self_),
        });
        let node = self.parse_attribute_access(self_node, token_id)?;

        match self.tokens.peek().map(|t| &t.kind) {
            Some(TokenKind::LBracket) => self.parse_bracket_access(node, token),
            _ => Ok(node),
        }
    }

    fn parse_break(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(token));

        // Check for colon and expression (break: expr)
        let value = if self.tokens.peek().map(|t| &t.kind) == Some(&TokenKind::Colon) {
            self.tokens.next(); // consume colon
            let expr_token = self
                .tokens
                .next()
                .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?;
            Some(self.parse_expr(expr_token)?)
        } else {
            None
        };

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Break(value)),
        }))
    }

    fn parse_continue(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        Ok(Shared::new(Node {
            token_id: self.token_arena.alloc(Shared::clone(token)),
            expr: Shared::new(Expr::Continue),
        }))
    }

    fn parse_array(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(token));
        let mut elements: SmallVec<[Shared<Node>; 4]> = SmallVec::new();

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::RBracket => break,
                TokenKind::Comma => continue,
                _ => {
                    let expr = self.parse_expr(token)?;
                    elements.push(expr);
                }
            }
        }

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Call(
                IdentWithToken::new_with_token(constants::ARRAY, Some(Shared::clone(token))),
                elements,
            )),
        }))
    }

    fn parse_all_nodes(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        Ok(Shared::new(Node {
            token_id: self.token_arena.alloc(Shared::clone(token)),
            expr: Shared::new(Expr::Nodes),
        }))
    }

    fn is_binary_op(token_kind: &TokenKind) -> bool {
        matches!(
            token_kind,
            TokenKind::And
                | TokenKind::Asterisk
                | TokenKind::Equal
                | TokenKind::EqEq
                | TokenKind::Coalesce
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

    fn is_next_token(&mut self, expected: impl Fn(&TokenKind) -> bool) -> bool {
        self.tokens.peek().as_ref().map(|t| &t.kind).is_some_and(expected)
    }

    fn is_next_token_allowed(token_kind: Option<&TokenKind>) -> bool {
        matches!(
            token_kind,
            Some(TokenKind::And)
                | Some(TokenKind::Asterisk)
                | Some(TokenKind::Catch)
                | Some(TokenKind::Colon)
                | Some(TokenKind::Comma)
                | Some(TokenKind::Eof)
                | Some(TokenKind::Elif)
                | Some(TokenKind::Else)
                | Some(TokenKind::EqEq)
                | Some(TokenKind::Equal)
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
                | Some(TokenKind::Coalesce)
                | None
        )
    }

    fn parse_literal(&mut self, literal_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let literal_node = match &literal_token.kind {
            TokenKind::BoolLiteral(b) => Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(literal_token)),
                expr: Shared::new(Expr::Literal(Literal::Bool(*b))),
            })),
            TokenKind::StringLiteral(s) => Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(literal_token)),
                expr: Shared::new(Expr::Literal(Literal::String(s.to_owned()))),
            })),
            TokenKind::NumberLiteral(n) => Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(literal_token)),
                expr: Shared::new(Expr::Literal(Literal::Number(*n))),
            })),
            TokenKind::None => Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(literal_token)),
                expr: Shared::new(Expr::Literal(Literal::None)),
            })),
            TokenKind::Eof => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(SyntaxError::UnexpectedToken((**literal_token).clone())),
        }?;

        let token = self.tokens.peek();

        if Self::is_next_token_allowed(token.as_ref().map(|t| &t.kind)) {
            Ok(literal_node)
        } else {
            Err(SyntaxError::UnexpectedToken((***token.unwrap()).clone()))
        }
    }

    fn parse_ident(&mut self, ident: &str, ident_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        match self.tokens.peek().map(|t| &t.kind) {
            Some(TokenKind::Selector(selector)) if selector.len() > 1 => {
                let token_id = self.token_arena.alloc(Shared::clone(ident_token));
                let base_node = Shared::new(Node {
                    token_id,
                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(
                        ident,
                        Some(Shared::clone(ident_token)),
                    ))),
                });

                self.parse_attribute_access(base_node, token_id)
            }
            Some(TokenKind::DoubleColon) => {
                // Parse qualified access: module::function(), module::ident, or module::module2::method
                // Build the module path by collecting all identifiers separated by '::'
                let mut module_path = vec![IdentWithToken::new_with_token(ident, Some(Shared::clone(ident_token)))];

                // Collect all module path segments
                while matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::DoubleColon)) {
                    self.tokens.next(); // consume '::'

                    let next_token = self
                        .tokens
                        .next()
                        .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?;

                    let next_ident = match &next_token.kind {
                        TokenKind::Ident(name) => name.clone(),
                        _ => return Err(SyntaxError::UnexpectedToken((**next_token).clone())),
                    };

                    // Check if this is the last segment (followed by '(' or not '::')
                    match self.tokens.peek().map(|t| &t.kind) {
                        Some(TokenKind::DoubleColon) => {
                            // More segments to come, add to module path
                            module_path.push(IdentWithToken::new_with_token(
                                &next_ident,
                                Some(Shared::clone(next_token)),
                            ));
                        }
                        Some(TokenKind::LParen) => {
                            // This is a function call: module::...::function(args)
                            let args = self.parse_args()?;
                            let access_target = AccessTarget::Call(
                                IdentWithToken::new_with_token(&next_ident, Some(Shared::clone(next_token))),
                                args,
                            );

                            let token_id = self.token_arena.alloc(Shared::clone(ident_token));
                            return Ok(Shared::new(Node {
                                token_id,
                                expr: Shared::new(Expr::QualifiedAccess(module_path, access_target)),
                            }));
                        }
                        _ => {
                            // This is an identifier: module::...::ident
                            let access_target = AccessTarget::Ident(IdentWithToken::new_with_token(
                                &next_ident,
                                Some(Shared::clone(next_token)),
                            ));

                            let token_id = self.token_arena.alloc(Shared::clone(ident_token));
                            return Ok(Shared::new(Node {
                                token_id,
                                expr: Shared::new(Expr::QualifiedAccess(module_path, access_target)),
                            }));
                        }
                    }
                }

                // This should not be reached, but handle it gracefully
                Err(SyntaxError::UnexpectedToken((**ident_token).clone()))
            }
            Some(TokenKind::LParen) => {
                let mut args = self.parse_args()?;
                let token_id = self.token_arena.alloc(Shared::clone(ident_token));

                // Check for macro call (e.g., foo(args) do ...)
                if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Do)) {
                    let do_token = self.tokens.next().unwrap(); // consume 'do'
                    let block = self.parse_block(do_token)?;
                    args.push(block);

                    return Ok(Shared::new(Node {
                        token_id: self.token_arena.alloc(Shared::clone(ident_token)),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(ident, Some(Shared::clone(ident_token))),
                            args,
                        )),
                    }));
                }

                let call_node = Shared::new(Node {
                    token_id,
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(ident, Some(Shared::clone(ident_token))),
                        args,
                    )),
                });

                if self.is_next_token(|token_kind| matches!(token_kind, TokenKind::Question)) {
                    let question_token = self.tokens.next().unwrap();
                    let question_token_id = self.token_arena.alloc(Shared::clone(question_token));

                    return Ok(Shared::new(Node {
                        token_id: question_token_id,
                        expr: Shared::new(Expr::Try(
                            call_node,
                            Shared::new(Node {
                                token_id,
                                expr: Shared::new(Expr::Literal(Literal::None)),
                            }),
                        )),
                    }));
                }

                // Check for bracket access after function call (e.g., foo()[0])
                if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                    self.parse_bracket_access(call_node, ident_token)
                } else if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Selector(_))) {
                    self.parse_attribute_access(call_node, token_id)
                } else if Self::is_next_token_allowed(self.tokens.peek().map(|t| &t.kind)) {
                    Ok(call_node)
                } else {
                    Err(SyntaxError::UnexpectedToken((***self.tokens.peek().unwrap()).clone()))
                }
            }
            Some(TokenKind::LBracket) => {
                let ident_node = Shared::new(Node {
                    token_id: self.token_arena.alloc(Shared::clone(ident_token)),
                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(
                        ident,
                        Some(Shared::clone(ident_token)),
                    ))),
                });

                self.parse_bracket_access(ident_node, ident_token)
            }
            token if Self::is_next_token_allowed(token) => Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(ident_token)),
                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(
                    ident,
                    Some(Shared::clone(ident_token)),
                ))),
            })),
            _ => Err(SyntaxError::UnexpectedToken((**ident_token).clone())),
        }
    }

    // Parses bracket access operations recursively to handle nested access like arr[0][1][2]
    fn parse_bracket_access(
        &mut self,
        target_node: Shared<Node>,
        original_token: &Shared<Token>,
    ) -> Result<Shared<Node>, SyntaxError> {
        let _ = self.tokens.next(); // consume '['

        // Parse the first expression
        let first_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        let first_node = self.parse_expr(first_token)?;

        // Check if this is a slice operation (contains ':')
        let is_slice = matches!(self.tokens.peek(), Some(token) if matches!(token.kind, TokenKind::Colon));

        let result_node = if is_slice {
            // Consume the colon
            let _ = self.tokens.next();

            match self.tokens.next() {
                Some(t) if t.kind == TokenKind::RBracket => Shared::new(Node {
                    token_id: self.token_arena.alloc(Shared::clone(original_token)),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(constants::SLICE, Some(Shared::clone(original_token))),
                        smallvec![
                            Shared::clone(&target_node),
                            first_node,
                            Shared::new(Node {
                                token_id: self.token_arena.alloc(Shared::clone(original_token)),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::LEN, None),
                                    smallvec![target_node],
                                )),
                            })
                        ],
                    )),
                }),
                Some(t) => {
                    let second_node = self.parse_expr(t)?;

                    // Expect closing bracket
                    match self.tokens.peek() {
                        Some(token) if matches!(token.kind, TokenKind::RBracket) => {
                            let _ = self.tokens.next(); // consume ']'
                        }
                        Some(token) => {
                            return Err(SyntaxError::ExpectedClosingBracket((***token).clone()));
                        }
                        None => {
                            return Err(SyntaxError::ExpectedClosingBracket(Token {
                                range: original_token.range,
                                kind: TokenKind::Eof,
                                module_id: self.module_id,
                            }));
                        }
                    }

                    Shared::new(Node {
                        token_id: self.token_arena.alloc(Shared::clone(original_token)),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::SLICE, Some(Shared::clone(original_token))),
                            smallvec![target_node, first_node, second_node],
                        )),
                    })
                }
                None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            }
        } else {
            // Expect closing bracket
            match self.tokens.peek() {
                Some(token) if matches!(token.kind, TokenKind::RBracket) => {
                    let _ = self.tokens.next(); // consume ']'
                }
                Some(token) => {
                    return Err(SyntaxError::ExpectedClosingBracket((***token).clone()));
                }
                None => {
                    return Err(SyntaxError::ExpectedClosingBracket(Token {
                        range: original_token.range,
                        kind: TokenKind::Eof,
                        module_id: self.module_id,
                    }));
                }
            }

            Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(original_token)),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::GET, Some(Shared::clone(original_token))),
                    smallvec![target_node, first_node],
                )),
            })
        };

        // Check for additional bracket access (nested indexing)
        let final_result = if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
            self.parse_bracket_access(result_node, original_token)?
        } else {
            result_node
        };

        // Check for function call after bracket access (e.g., arr[0]() or arr[0][1]())
        if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LParen)) {
            let args = self.parse_args()?;
            Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(original_token)),
                expr: Shared::new(Expr::CallDynamic(final_result, args)),
            }))
        } else {
            Ok(final_result)
        }
    }

    fn parse_def(&mut self, def_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let ident_token = self.tokens.next();
        let ident = match &ident_token {
            Some(token) => match &token.kind {
                TokenKind::Ident(ident) => Ok(ident),
                _ => Err(SyntaxError::UnexpectedToken((***token).clone())),
            },
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;
        let def_token_id = self.token_arena.alloc(Shared::clone(def_token));
        let params = self.parse_params()?;

        self.consume_colon_or_do();

        let program = self.parse_program(false)?;

        Ok(Shared::new(Node {
            token_id: def_token_id,
            expr: Shared::new(Expr::Def(
                IdentWithToken::new_with_token(ident, ident_token.map(Shared::clone)),
                params,
                program,
            )),
        }))
    }

    fn parse_macro(&mut self, macro_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let ident_token = self.tokens.next();
        let ident = match &ident_token {
            Some(token) => match &token.kind {
                TokenKind::Ident(ident) => Ok(ident),
                _ => Err(SyntaxError::UnexpectedToken((***token).clone())),
            },
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;
        let macro_token_id = self.token_arena.alloc(Shared::clone(macro_token));
        let params = self.parse_params()?;

        // Macros should not support default parameters
        if params.iter().any(|p| p.default.is_some()) {
            return Err(SyntaxError::MacroParametersCannotHaveDefaults(
                (*self.token_arena[macro_token_id]).clone(),
            ));
        }

        self.consume_colon();

        let expr = match self.tokens.next() {
            Some(token) => self.parse_expr(token)?,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        Ok(Shared::new(Node {
            token_id: macro_token_id,
            expr: Shared::new(Expr::Macro(
                IdentWithToken::new_with_token(ident, ident_token.map(Shared::clone)),
                params,
                expr,
            )),
        }))
    }

    fn parse_block(&mut self, do_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let do_token_id = self.token_arena.alloc(Shared::clone(do_token));
        let program = self.parse_program(false)?;

        // The End token is already consumed by parse_program when it encounters it
        // No need to expect another End token here

        Ok(Shared::new(Node {
            token_id: do_token_id,
            expr: Shared::new(Expr::Block(program)),
        }))
    }

    fn parse_fn(&mut self, fn_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let fn_token_id = self.token_arena.alloc(Shared::clone(fn_token));
        let params = self.parse_params()?;

        self.consume_colon_or_do();

        let program = self.parse_program(false)?;

        Ok(Shared::new(Node {
            token_id: fn_token_id,
            expr: Shared::new(Expr::Fn(params, program)),
        }))
    }

    fn parse_while(&mut self, while_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(while_token));
        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(SyntaxError::UnexpectedToken((**while_token).clone()));
        }

        self.consume_colon_or_do();

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
            None => Err(SyntaxError::UnexpectedToken((**while_token).clone())),
        }
    }

    fn parse_loop(&mut self, loop_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(loop_token));

        self.consume_colon_or_do();

        match self.tokens.peek() {
            Some(_) => {
                let body_program = self.parse_program(false)?;

                Ok(Shared::new(Node {
                    token_id,
                    expr: Shared::new(Expr::Loop(body_program.iter().map(Shared::clone).collect())),
                }))
            }
            None => Err(SyntaxError::UnexpectedToken((**loop_token).clone())),
        }
    }

    fn parse_try(&mut self, try_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(try_token));

        self.consume_colon_or_do();

        // Parse try expression
        let try_expr = match self.tokens.next() {
            Some(token) => self.parse_expr(token)?,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        if !self.is_next_token(|token_kind| matches!(token_kind, TokenKind::Catch)) {
            return Ok(Shared::new(Node {
                token_id,
                expr: Shared::new(Expr::Try(
                    try_expr,
                    Shared::new(Node {
                        token_id,
                        expr: Shared::new(Expr::Literal(Literal::None)),
                    }),
                )),
            }));
        }

        // Expect 'catch' keyword
        self.next_token(|token_kind| matches!(token_kind, TokenKind::Catch))?;
        self.consume_colon_or_do();

        // Parse catch expression
        let catch_expr = match self.tokens.next() {
            Some(token) => self.parse_expr(token)?,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Try(try_expr, catch_expr)),
        }))
    }

    fn parse_quote(&mut self, quote_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(quote_token));

        self.consume_colon();

        let expr = match self.tokens.next() {
            Some(token) => self.parse_expr(token)?,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Quote(expr)),
        }))
    }

    fn parse_unquote(&mut self, unquote_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(unquote_token));

        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(SyntaxError::UnexpectedToken((*self.token_arena[token_id]).clone()));
        }

        let expr = Shared::clone(args.first().unwrap());

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Unquote(expr)),
        }))
    }

    fn parse_foreach(&mut self, foreach_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let args = self.parse_args()?;

        if args.len() != 2 {
            return Err(SyntaxError::UnexpectedToken((**foreach_token).clone()));
        }

        let first_arg = &*args.first().unwrap().expr;

        match first_arg {
            Expr::Ident(IdentWithToken {
                name: ident,
                token: ident_token,
            }) => {
                self.consume_colon_or_do();

                let each_values = Shared::clone(&args[1]);
                let body_program = self.parse_program(false)?;

                Ok(Shared::new(Node {
                    token_id: self.token_arena.alloc(Shared::clone(foreach_token)),
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
            _ => Err(SyntaxError::UnexpectedToken((**foreach_token).clone())),
        }
    }

    fn parse_if(&mut self, if_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(if_token));
        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(SyntaxError::UnexpectedToken((*self.token_arena[token_id]).clone()));
        }
        let cond = args.first().unwrap();

        self.consume_colon();

        let then_expr = self.parse_next_expr(token_id)?;

        let mut branches: Branches = SmallVec::new();
        branches.push((Some(Shared::clone(cond)), then_expr));

        let elif_branches = self.parse_elif()?;
        branches.extend(elif_branches);

        if let Some(token) = self.tokens.peek()
            && matches!(token.kind, TokenKind::Else)
        {
            let token_id = self.next_token(|token_kind| matches!(token_kind, TokenKind::Else))?;

            self.consume_colon();

            let else_expr = self.parse_next_expr(token_id)?;
            branches.push((None, else_expr));
        }

        Ok(Shared::new(Node {
            token_id: self.token_arena.alloc(Shared::clone(if_token)),
            expr: Shared::new(Expr::If(branches)),
        }))
    }

    fn parse_match(&mut self, match_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(match_token));

        // Parse the value expression: match (value):
        let args = self.parse_args()?;
        if args.len() != 1 {
            return Err(SyntaxError::UnexpectedToken((*self.token_arena[token_id]).clone()));
        }
        let value = Shared::clone(args.first().unwrap());

        self.consume_colon_or_do();

        // Parse match arms
        let mut arms: super::node::MatchArms = SmallVec::new();

        while let Some(token) = self.tokens.peek() {
            // Check for end of match
            if matches!(token.kind, TokenKind::End | TokenKind::Eof) {
                break;
            }

            // Consume pipe '|' before each arm
            let token_id = self.next_token(|token_kind| matches!(token_kind, TokenKind::Pipe))?;
            // Parse pattern
            let pattern = self.parse_pattern()?;
            // Check for guard (if condition)
            let guard = if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::If) {
                    let if_token = Shared::clone(token);
                    self.tokens.next(); // consume 'if'
                    let guard_args = self.parse_args()?;
                    if guard_args.len() != 1 {
                        return Err(SyntaxError::UnexpectedToken((*if_token).clone()));
                    }
                    Some(Shared::clone(guard_args.first().unwrap()))
                } else {
                    None
                }
            } else {
                None
            };

            self.consume_colon();

            // Parse body expression
            let body = self.parse_next_expr(token_id)?;

            arms.push(MatchArm { pattern, guard, body });
        }

        // Consume 'end' keyword
        if let Some(token) = self.tokens.peek()
            && matches!(token.kind, TokenKind::End)
        {
            self.tokens.next();
        }

        Ok(Shared::new(Node {
            token_id,
            expr: Shared::new(Expr::Match(value, arms)),
        }))
    }

    fn parse_pattern(&mut self) -> Result<Pattern, SyntaxError> {
        let token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        match &token.kind {
            // Wildcard pattern: _
            TokenKind::Ident(name) if name == constants::PATTERN_MATCH_WILDCARD => Ok(Pattern::Wildcard),
            // Type pattern: :string, :number, etc.
            TokenKind::Colon => {
                let type_token = match self.tokens.next() {
                    Some(t) => t,
                    None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                };
                match &type_token.kind {
                    TokenKind::Ident(type_name) => Ok(Pattern::Type(Ident::new(type_name))),
                    _ => Err(SyntaxError::UnexpectedToken((**type_token).clone())),
                }
            }
            // Literal patterns
            TokenKind::StringLiteral(s) => Ok(Pattern::Literal(Literal::String(s.clone()))),
            TokenKind::NumberLiteral(n) => Ok(Pattern::Literal(Literal::Number(*n))),
            TokenKind::BoolLiteral(b) => Ok(Pattern::Literal(Literal::Bool(*b))),
            TokenKind::None => Ok(Pattern::Literal(Literal::None)),
            // Array pattern: [pattern, pattern, ...]
            TokenKind::LBracket => self.parse_array_pattern(),
            // Dict pattern: {key, key: pattern}
            TokenKind::LBrace => self.parse_dict_pattern(),
            // Identifier pattern (binding)
            TokenKind::Ident(name) => Ok(Pattern::Ident(IdentWithToken::new(name))),
            _ => Err(SyntaxError::UnexpectedToken((**token).clone())),
        }
    }

    fn parse_array_pattern(&mut self) -> Result<super::node::Pattern, SyntaxError> {
        let mut patterns = Vec::new();
        let mut has_rest = false;
        let mut rest_binding: Option<IdentWithToken> = None;

        loop {
            // Check for closing bracket
            if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::RBracket) {
                    self.tokens.next(); // consume ]
                    break;
                }

                // Check for rest pattern: ...rest
                if matches!(token.kind, TokenKind::RangeOp) {
                    self.tokens.next();
                    if let Some(ident_token) = self.tokens.next() {
                        if let TokenKind::Ident(name) = &ident_token.kind {
                            rest_binding = Some(IdentWithToken::new(name));
                            has_rest = true;
                        } else {
                            return Err(SyntaxError::UnexpectedToken((**ident_token).clone()));
                        }
                    }
                    // Expect closing bracket after rest
                    if let Some(token) = self.tokens.next()
                        && !matches!(token.kind, TokenKind::RBracket)
                    {
                        return Err(SyntaxError::UnexpectedToken((**token).clone()));
                    }
                    break;
                }
            }

            let pattern = self.parse_pattern()?;
            patterns.push(pattern);

            // Check for comma or closing bracket
            if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::Comma) {
                    self.tokens.next(); // consume comma
                } else if matches!(token.kind, TokenKind::RBracket) {
                    // Will be consumed in next iteration
                    continue;
                } else {
                    return Err(SyntaxError::UnexpectedToken((***token).clone()));
                }
            }
        }

        if has_rest {
            Ok(Pattern::ArrayRest(patterns, rest_binding.unwrap()))
        } else {
            Ok(Pattern::Array(patterns))
        }
    }

    fn parse_dict_pattern(&mut self) -> Result<super::node::Pattern, SyntaxError> {
        let mut fields = Vec::new();

        loop {
            // Check for closing brace
            if let Some(token) = self.tokens.peek()
                && matches!(token.kind, TokenKind::RBrace)
            {
                self.tokens.next(); // consume }
                break;
            }

            // Parse key (must be identifier)
            let key_token = match self.tokens.next() {
                Some(t) => t,
                None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            };

            let key = match &key_token.kind {
                TokenKind::Ident(name) => IdentWithToken::new(name),
                _ => return Err(SyntaxError::UnexpectedToken((**key_token).clone())),
            };

            // Check if there's a colon (key: pattern) or just key shorthand
            let pattern = if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::Colon) {
                    self.tokens.next(); // consume colon
                    self.parse_pattern()?
                } else {
                    // Shorthand: {key} means {key: key}
                    super::node::Pattern::Ident(key.clone())
                }
            } else {
                super::node::Pattern::Ident(key.clone())
            };

            fields.push((key, pattern));

            // Check for comma or closing brace
            if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::Comma) {
                    self.tokens.next(); // consume comma
                } else if matches!(token.kind, TokenKind::RBrace) {
                    // Will be consumed in next iteration
                    continue;
                } else {
                    return Err(SyntaxError::UnexpectedToken((***token).clone()));
                }
            }
        }

        Ok(super::node::Pattern::Dict(fields))
    }

    #[inline(always)]
    fn parse_next_expr(&mut self, token_id: TokenId) -> Result<Shared<Node>, SyntaxError> {
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(SyntaxError::UnexpectedToken((*self.token_arena[token_id]).clone())),
        }?;

        self.parse_expr(expr_token)
    }

    fn parse_elif(&mut self) -> Result<Vec<IfExpr>, SyntaxError> {
        let mut nodes = Vec::with_capacity(8);

        while let Some(token) = self.tokens.peek() {
            if !matches!(token.kind, TokenKind::Elif) {
                break;
            }

            let token_id = self.next_token(|token_kind| matches!(token_kind, TokenKind::Elif))?;
            let args = self.parse_args()?;

            if args.len() != 1 {
                return Err(SyntaxError::UnexpectedToken((*self.token_arena[token_id]).clone()));
            }

            self.consume_colon();

            let expr_token = match self.tokens.next() {
                Some(token) => Ok(token),
                None => Err(SyntaxError::UnexpectedToken((*self.token_arena[token_id]).clone())),
            }?;

            let cond = args.first().unwrap();
            let then_expr = self.parse_expr(expr_token)?;

            nodes.push((Some(Shared::clone(cond)), then_expr));
        }

        Ok(nodes)
    }

    fn parse_let(&mut self, let_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let ident_token = self.tokens.next();
        let ident = match &ident_token {
            Some(token) => match &***token {
                Token {
                    range: _,
                    kind: TokenKind::Ident(ident),
                    module_id: _,
                } => Ok(ident),
                token => Err(SyntaxError::UnexpectedToken((*token).clone())),
            },
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        let let_token_id = self.token_arena.alloc(Shared::clone(let_token));
        self.next_token(|token_kind| matches!(token_kind, TokenKind::Equal))?;
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        if matches!(expr_token.kind, TokenKind::Let | TokenKind::Var) {
            return Err(SyntaxError::UnexpectedToken((**expr_token).clone()));
        }

        let ast = self.parse_expr(expr_token)?;

        if let Some(token) = self.tokens.peek()
            && !matches!(
                token.kind,
                TokenKind::Pipe | TokenKind::Eof | TokenKind::SemiColon | TokenKind::End
            )
        {
            return Err(SyntaxError::UnexpectedToken((***token).clone()));
        }

        Ok(Shared::new(Node {
            token_id: let_token_id,
            expr: Shared::new(Expr::Let(
                IdentWithToken::new_with_token(ident, ident_token.map(Shared::clone)),
                ast,
            )),
        }))
    }

    fn parse_var(&mut self, var_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let ident_token = self.tokens.next();
        let ident = match &ident_token {
            Some(token) => match &token.kind {
                TokenKind::Ident(ident) => Ok(ident),
                _ => Err(SyntaxError::UnexpectedToken((***token).clone())),
            },
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        let var_token_id = self.token_arena.alloc(Shared::clone(var_token));
        self.next_token(|token_kind| matches!(token_kind, TokenKind::Equal))?;
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        if matches!(expr_token.kind, TokenKind::Let | TokenKind::Var) {
            return Err(SyntaxError::UnexpectedToken((**expr_token).clone()));
        }

        let ast = self.parse_expr(expr_token)?;

        if let Some(token) = self.tokens.peek()
            && !matches!(
                token.kind,
                TokenKind::Pipe | TokenKind::Eof | TokenKind::SemiColon | TokenKind::End
            )
        {
            return Err(SyntaxError::UnexpectedToken((***token).clone()));
        }

        Ok(Shared::new(Node {
            token_id: var_token_id,
            expr: Shared::new(Expr::Var(
                IdentWithToken::new_with_token(ident, ident_token.map(Shared::clone)),
                ast,
            )),
        }))
    }

    #[inline(always)]
    fn parse_include(&mut self, include_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::StringLiteral(module) => {
                    self.tokens.next();
                    Ok(Shared::new(Node {
                        token_id: self.token_arena.alloc(Shared::clone(include_token)),
                        expr: Shared::new(Expr::Include(Literal::String(module.to_owned()))),
                    }))
                }
                _ => Err(SyntaxError::InsufficientTokens((***token).clone())),
            },
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }
    }

    #[inline(always)]
    fn parse_import(&mut self, import_token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.token_arena.alloc(Shared::clone(import_token));
        let token = match self.tokens.next() {
            Some(token) => Ok(Shared::clone(token)),
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        match &token.kind {
            TokenKind::StringLiteral(module) => {
                let module_name = module.to_owned();
                Ok(Shared::new(Node {
                    token_id,
                    expr: Shared::new(Expr::Import(Literal::String(module_name))),
                }))
            }
            _ => Err(SyntaxError::InsufficientTokens((*token).clone())),
        }
    }

    fn parse_interpolated_string(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        if let TokenKind::InterpolatedString(segments) = &token.kind {
            let mut parsed_segments = Vec::new();

            for segment in segments {
                match segment {
                    lexer::token::StringSegment::Text(text, _) => {
                        parsed_segments.push(super::node::StringSegment::Text(text.clone()));
                    }
                    lexer::token::StringSegment::Expr(expr_str, range) => {
                        // Parse the expression string
                        let expr_str = expr_str.trim();

                        // Handle special cases first
                        if expr_str == constants::SELF {
                            parsed_segments.push(super::node::StringSegment::Self_);
                        } else if let Some(stripped) = expr_str.strip_prefix("$") {
                            // Environment variable
                            parsed_segments.push(super::node::StringSegment::Env(SmolStr::from(stripped)));
                        } else {
                            // Parse as a full expression
                            let lexer = Lexer::new(crate::lexer::Options::default());
                            let tokens = lexer.tokenize(expr_str, token.module_id).map_err(|_| {
                                SyntaxError::UnexpectedToken(Token {
                                    range: *range,
                                    kind: TokenKind::InterpolatedString(vec![]),
                                    module_id: token.module_id,
                                })
                            })?;

                            let shared_tokens: Vec<Shared<Token>> = tokens.into_iter().map(Shared::new).collect();
                            let mut parser = Parser::new(shared_tokens.iter(), self.token_arena, token.module_id);
                            let expr_node = parser.parse_expr_from_tokens().map_err(|_| {
                                SyntaxError::UnexpectedToken(Token {
                                    range: *range,
                                    kind: TokenKind::InterpolatedString(vec![]),
                                    module_id: token.module_id,
                                })
                            })?;

                            parsed_segments.push(super::node::StringSegment::Expr(expr_node));
                        }
                    }
                }
            }

            Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(token)),
                expr: Shared::new(Expr::InterpolatedString(parsed_segments)),
            }))
        } else {
            Err(SyntaxError::UnexpectedToken((**token).clone()))
        }
    }

    #[inline(always)]
    fn parse_expr_from_tokens(&mut self) -> Result<Shared<Node>, SyntaxError> {
        if let Some(token) = self.tokens.next() {
            self.parse_expr(token)
        } else {
            Err(SyntaxError::UnexpectedEOFDetected(self.module_id))
        }
    }

    /// Parse function parameters (supports default values)
    fn parse_params(&mut self) -> Result<Params, SyntaxError> {
        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::LParen => {
                    self.tokens.next();
                }
                _ => return Err(SyntaxError::UnexpectedToken((***token).clone())),
            },
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        let mut params: Params = SmallVec::new();
        let mut prev_token: Option<&TokenKind> = None;
        let mut seen_default = false;

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::RParen => match prev_token {
                    Some(TokenKind::Comma) => {
                        return Err(SyntaxError::UnexpectedToken((**token).clone()));
                    }
                    _ => break,
                },
                TokenKind::Eof => match prev_token {
                    Some(TokenKind::RParen) => break,
                    Some(_) | None => {
                        return Err(SyntaxError::ExpectedClosingParen((**token).clone()));
                    }
                },
                TokenKind::Comma => match prev_token {
                    Some(_) => {
                        let token = match self.tokens.peek() {
                            Some(token) => Ok(Shared::clone(token)),
                            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                        }?;
                        match &token.kind {
                            TokenKind::Comma => {
                                return Err(SyntaxError::UnexpectedToken((*token).clone()));
                            }
                            _ => continue,
                        }
                    }
                    None => return Err(SyntaxError::UnexpectedToken((**token).clone())),
                },
                TokenKind::SemiColon => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                TokenKind::Ident(name) => {
                    // Parse parameter name
                    let ident = IdentWithToken::new_with_token(name, Some(Shared::clone(token)));

                    // Check for '=' indicating a default value
                    let default = if let Some(next_token) = self.tokens.peek()
                        && matches!(next_token.kind, TokenKind::Equal)
                    {
                        self.tokens.next(); // consume '='
                        seen_default = true;

                        // Parse default value expression
                        let default_token = match self.tokens.next() {
                            Some(t) => t,
                            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                        };

                        Some(self.parse_expr(default_token)?)
                    } else {
                        if seen_default {
                            return Err(SyntaxError::ParameterWithoutDefaultAfterDefault((**token).clone()));
                        }
                        None
                    };

                    params.push(Param::with_default(ident, default));
                }
                _ => {
                    return Err(SyntaxError::UnexpectedToken((**token).clone()));
                }
            }

            prev_token = Some(&token.kind);

            if let Some(token) = self.tokens.peek()
                && !matches!(token.kind, TokenKind::RParen | TokenKind::Comma)
            {
                return Err(SyntaxError::ExpectedClosingParen((***token).clone()));
            }
        }

        Ok(params)
    }

    fn parse_args(&mut self) -> Result<Args, SyntaxError> {
        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::LParen => {
                    self.tokens.next();
                }
                _ => return Err(SyntaxError::UnexpectedToken((***token).clone())),
            },
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        let mut args: Args = SmallVec::new();
        let mut prev_token: Option<&TokenKind> = None;

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::RParen => match prev_token {
                    Some(TokenKind::Comma) => {
                        return Err(SyntaxError::UnexpectedToken((**token).clone()));
                    }
                    _ => break,
                },
                TokenKind::Eof => match prev_token {
                    Some(TokenKind::RParen) => break,
                    Some(_) | None => {
                        return Err(SyntaxError::ExpectedClosingParen((**token).clone()));
                    }
                },
                TokenKind::Comma => match prev_token {
                    Some(_) => {
                        let token = match self.tokens.peek() {
                            Some(token) => Ok(Shared::clone(token)),
                            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                        }?;
                        match &token.kind {
                            TokenKind::Comma => {
                                return Err(SyntaxError::UnexpectedToken((*token).clone()));
                            }
                            _ => continue,
                        }
                    }
                    None => return Err(SyntaxError::UnexpectedToken((**token).clone())),
                },
                TokenKind::SemiColon => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                _ => {
                    // Arguments that are complex expressions (idents, selectors, if, fn)
                    args.push(self.parse_arg_expr(token)?);
                }
            }

            prev_token = Some(&token.kind);

            if let Some(token) = self.tokens.peek()
                && !matches!(token.kind, TokenKind::RParen | TokenKind::Comma)
            {
                return Err(SyntaxError::ExpectedClosingParen((***token).clone()));
            }
        }

        Ok(args)
    }

    // Helper to parse an argument that is expected to be a general expression.
    // This typically involves a recursive call to `parse_expr`.
    #[inline(always)]
    fn parse_arg_expr(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        self.parse_expr(token)
    }

    /// Parse a selector with an attribute suffix and convert it to an attr() function call
    fn parse_selector_with_attribute(
        &mut self,
        token: &Shared<Token>,
        attr_token: Shared<Token>,
    ) -> Result<Shared<Node>, SyntaxError> {
        if let TokenKind::Selector(attr_selector) = &attr_token.kind {
            let attribute = &attr_selector[1..]; // Skip the dot
            // Parse the base selector recursively
            let base_node = self.parse_selector_direct(token)?;

            if !Selector::try_from(&*attr_token)
                .map_err(SyntaxError::UnknownSelector)?
                .is_attribute_selector()
            {
                return Err(SyntaxError::UnexpectedToken((*attr_token).clone()));
            }

            // Create the attribute string literal
            let attr_literal = Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(token)),
                expr: Shared::new(Expr::Literal(Literal::String(attribute.to_string()))),
            });

            // Create the attr() function call
            Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(token)),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::ATTR, Some(Shared::clone(token))),
                    smallvec![base_node, attr_literal],
                )),
            }))
        } else {
            Err(SyntaxError::UnexpectedToken((**token).clone()))
        }
    }

    /// Parse a selector without checking for attributes (to avoid infinite recursion)
    fn parse_selector_direct(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        if let TokenKind::Selector(selector) = &token.kind {
            if selector == "." {
                if self.is_next_token(|token_kind| matches!(token_kind, TokenKind::LBracket)) {
                    self.parse_selector_table_args(Shared::clone(token))
                } else {
                    Ok(Shared::new(Node {
                        token_id: self.token_arena.alloc(Shared::clone(token)),
                        expr: Shared::new(Expr::Self_),
                    }))
                }
            } else {
                let selector = Selector::try_from(&**token).map_err(SyntaxError::UnknownSelector)?;

                Ok(Shared::new(Node {
                    token_id: self.token_arena.alloc(Shared::clone(token)),
                    expr: Shared::new(Expr::Selector(selector)),
                }))
            }
        } else {
            Err(SyntaxError::InsufficientTokens((**token).clone()))
        }
    }

    fn parse_selector(&mut self, token: &Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        if let TokenKind::Selector(_) = &token.kind {
            if self.is_next_token(|kind| matches!(kind, TokenKind::Selector(_)))
                && let Some(attr_token) = self.tokens.next()
            {
                return self.parse_selector_with_attribute(token, Shared::clone(attr_token));
            }

            self.parse_selector_direct(token)
        } else {
            Err(SyntaxError::InsufficientTokens((**token).clone()))
        }
    }

    // Parses arguments for table or list item selectors like `.[index1][index2]` (for tables) or `.[index1]` (for lists).
    // Example: .[0][1] or .[0]
    fn parse_selector_table_args(&mut self, token: Shared<Token>) -> Result<Shared<Node>, SyntaxError> {
        let token1 = match self.tokens.peek() {
            Some(token) => Ok(Shared::clone(token)),
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        let ArrayIndex(i1) = self.parse_int_array_arg(&token1)?;
        let token2 = match self.tokens.peek() {
            Some(token) => Ok(Shared::clone(token)),
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        if let TokenKind::LBracket = &token2.kind {
            // .[n][n]
            let ArrayIndex(i2) = self.parse_int_array_arg(&token2)?;
            Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(&token)),
                expr: Shared::new(Expr::Selector(Selector::Table(i1, i2))),
            }))
        } else {
            // .[n]
            Ok(Shared::new(Node {
                token_id: self.token_arena.alloc(Shared::clone(&token)),
                expr: Shared::new(Expr::Selector(Selector::List(i1, None))),
            }))
        }
    }

    fn parse_int_array_arg(&mut self, token: &Shared<Token>) -> Result<ArrayIndex, SyntaxError> {
        self.next_token(|token_kind| matches!(token_kind, TokenKind::LBracket))?;

        let token = match self.tokens.peek() {
            Some(token) => Ok(Shared::clone(token)),
            None => return Err(SyntaxError::InsufficientTokens((**token).clone())),
        }?;

        if let TokenKind::NumberLiteral(n) = &token.kind {
            self.tokens.next();
            self.next_token(|token_kind| matches!(token_kind, TokenKind::RBracket))?;
            Ok(ArrayIndex(Some(n.value() as usize)))
        } else if let TokenKind::RBracket = &token.kind {
            self.tokens.next();
            Ok(ArrayIndex(None))
        } else {
            Err(SyntaxError::UnexpectedToken((*token).clone()))
        }
    }

    fn next_token(&mut self, expected_kinds: fn(&TokenKind) -> bool) -> Result<TokenId, SyntaxError> {
        match self.tokens.peek() {
            // Token found and matches one of the expected kinds.
            Some(token) if expected_kinds(&token.kind) => {
                let token = self.tokens.next().unwrap();
                Ok(self.token_arena.alloc(Shared::clone(token)))
            } // Consume and return.
            // Token found but does not match expected kinds.
            Some(token) => Err(SyntaxError::UnexpectedToken(Token {
                range: token.range,
                kind: token.kind.clone(),
                module_id: token.module_id,
            })),
            // No token found (EOF).
            None =>
            // If EOF is not expected here, it's an error.
            {
                Err(SyntaxError::UnexpectedEOFDetected(self.module_id))
            }
        }
    }

    #[inline(always)]
    fn consume_colon(&mut self) {
        if self.is_next_token(|token_kind| matches!(token_kind, TokenKind::Colon)) {
            let _ = self.next_token(|token_kind| matches!(token_kind, TokenKind::Colon));
        }
    }

    #[inline(always)]
    fn consume_colon_or_do(&mut self) {
        // Check for 'do' keyword
        if self.is_next_token(|kind| matches!(kind, TokenKind::Do)) {
            let _ = self.next_token(|kind| matches!(kind, TokenKind::Do));
        } else {
            self.consume_colon();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::node::StringSegment;
    use crate::{Module, ast::node::MatchArm, range::Range, selector};

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
                            token_id: 4.into(),
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
                        Param::new(IdentWithToken::new_with_token("arg1", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arg1"))))))),
                        Param::new(IdentWithToken::new_with_token("arg2", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arg2"))))))),
                    ],
                    vec![Shared::new(Node {
                        token_id: 4.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token("contains", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("contains")))))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Literal(Literal::String("arg1".to_owned()))),
                                }),
                                Shared::new(Node {
                                    token_id: 3.into(),
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
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Ident(SmolStr::new("and"))))))]
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
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Def))))]
    #[case::ident6(
        vec![
            token(TokenKind::Ident(SmolStr::new("and"))),
            token(TokenKind::Def),
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Ident(SmolStr::new("and"))))))]
    #[case::ident_attribute_access(
        vec![
            token(TokenKind::Ident(SmolStr::new("c"))),
            token(TokenKind::Selector(SmolStr::new(".lang"))),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("c")))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("c", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("c")))))))),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("lang".to_owned()))),
                        }),
                    ],
                )),
            })
        ]))]
    #[case::error(
        vec![
            token(TokenKind::Ident(SmolStr::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::Selector(SmolStr::new("inline_code"))),
            token(TokenKind::Eof)
        ],
        Err(SyntaxError::UnknownSelector(selector::UnknownSelector::new(token(TokenKind::Selector(SmolStr::new("inline_code")))))))]
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
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Comma, module_id: 1.into()})))]
    #[case::def3(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::Comma),
            token(TokenKind::RParen),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
    #[case::def4(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
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
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
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
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
    #[case::def7(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::StringLiteral("value".to_string()), module_id: 1.into()})))]
    #[case::def7(
        vec![
            token(TokenKind::Def),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::LParen, module_id: 1.into()})))]
    #[case::def_without_colon1(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
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
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                        })],
                )),
            }),
        ]))]
    #[case::def_without_colon2(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
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
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                        })],
                )),
            }),
        ]))]
    #[case::def_without_colon_with_args(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Def(
                        IdentWithToken::new_with_token("name", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("name")))))),
                        smallvec![
                          Param::new(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x"))))))),
                        ],
                        vec![Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Ident(
                                IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))),
                            )),
                        })],
                )),
            }),
        ]))]
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
    #[case::var_1(
            vec![
                token(TokenKind::Var),
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Shared::new(Expr::Var(
                        IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(42.into()))),
                        }),
                    )),
                })
            ]))]
    #[case::var_2(
            vec![
                token(TokenKind::Var),
                token(TokenKind::Ident(SmolStr::new("count"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(0.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 0.into(),
                    expr: Shared::new(Expr::Var(
                        IdentWithToken::new_with_token("count", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("count")))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(0.into()))),
                        }),
                    )),
                })
            ]))]
    #[case::assign_1(
            vec![
                token(TokenKind::Ident(SmolStr::new("x"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(100.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Assign(
                        IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(100.into()))),
                        }),
                    )),
                })
            ]))]
    #[case::assign_2(
            vec![
                token(TokenKind::Ident(SmolStr::new("name"))),
                token(TokenKind::Equal),
                token(TokenKind::StringLiteral("Alice".to_owned())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Assign(
                        IdentWithToken::new_with_token("name", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("name")))))),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("Alice".to_owned()))),
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
            Err(SyntaxError::UnexpectedToken(token(TokenKind::Ident(SmolStr::new("y"))))))]
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
            Err(SyntaxError::UnexpectedEOFDetected(0.into())))]
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
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::If, module_id: 1.into()})))]
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
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Elif, module_id: 1.into()})))]
    #[case::h_selector(
        vec![
            token(TokenKind::Selector(SmolStr::new(".h"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Selector(Selector::Heading(None))),
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
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::While, module_id: 1.into()})))]
    #[case::while_error(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::While, module_id: 1.into()})))]
    #[case::while_do_end(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Do),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::End),
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
    #[case::loop_(
        vec![
            token(TokenKind::Loop),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("loop body".to_owned())),
            token(TokenKind::SemiColon),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 1.into(),
            expr: Shared::new(Expr::Loop(
                vec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(Expr::Literal(Literal::String("loop body".to_owned()))),
                })],
            )),
        })]))]
    #[case::loop_error_no_body(
        vec![
            token(TokenKind::Loop),
            token(TokenKind::Colon),
        ],
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Loop, module_id: 1.into()})))]
    #[case::try_catch(
        vec![
            token(TokenKind::Try),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("error_expr"))),
            token(TokenKind::Catch),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("fallback".to_owned())),
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 2.into(),
            expr: Shared::new(Expr::Try(
                Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("error_expr", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("error_expr")))))))),
                }),
                Shared::new(Node {
                    token_id: 5.into(),
                    expr: Shared::new(Expr::Literal(Literal::String("fallback".to_owned()))),
                }),
            )),
        })]))]
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
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Literal(Literal::String("array".to_owned()))),
                }),
                vec![Shared::new(Node {
                    token_id: 4.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(
                            "print",
                            Some(Shared::new(token(TokenKind::Ident(SmolStr::new(
                                "print",
                            ))))),
                        ),
                        smallvec![Shared::new(Node {
                            token_id: 3.into(),
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
        Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Foreach, module_id: 1.into()})))]
    #[case::foreach_do_end(
        vec![
            token(TokenKind::Foreach),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("array".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Do),
            token(TokenKind::Ident(SmolStr::new("print"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("item"))),
            token(TokenKind::RParen),
            token(TokenKind::End),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 6.into(),
            expr: Shared::new(Expr::Foreach(
                IdentWithToken::new_with_token(
                    "item",
                    Some(Shared::new(token(TokenKind::Ident(SmolStr::new("item"))))),
                ),
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::Literal(Literal::String("array".to_owned()))),
                }),
                vec![Shared::new(Node {
                    token_id: 4.into(),
                    expr: Shared::new(Expr::Call(
                        IdentWithToken::new_with_token(
                            "print",
                            Some(Shared::new(token(TokenKind::Ident(SmolStr::new(
                                "print",
                            ))))),
                        ),
                        smallvec![Shared::new(Node {
                            token_id: 3.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token(
                                "item",
                                Some(Shared::new(token(TokenKind::Ident(SmolStr::new("item"))))),
                            ))),
                        })],
                    )),
                })],
            )),
        })]))]
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
            token(TokenKind::Eof),
        ],
        Ok(vec![Shared::new(Node {
            token_id: 2.into(),
            expr: Shared::new(Expr::Selector(Selector::Code)),
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
        Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::while_error(
        vec![
            token(TokenKind::While),
            token(TokenKind::LParen),
            token(TokenKind::BoolLiteral(true)),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Eof),
        ],
        Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::using_reserved_keyword_let(
            vec![
                token(TokenKind::Let),
                token(TokenKind::If),  // Using "if" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::If, module_id: 1.into()})))]
    #[case::using_reserved_keyword_while(
            vec![
                token(TokenKind::Let),
                token(TokenKind::While),  // Using "while" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::While, module_id: 1.into()})))]
    #[case::using_reserved_keyword_def(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Def),  // Using "def" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Def, module_id: 1.into()})))]
    #[case::using_reserved_keyword_include(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Include),  // Using "include" as a variable name (should error)
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Err(SyntaxError::UnexpectedToken(Token{range: Range::default(), kind: TokenKind::Include, module_id: 1.into()})))]
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
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Nodes))))]
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
                        Param::new(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x"))))))),
                        Param::new(IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y"))))))),
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token("contains", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("contains")))))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 3.into(),
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
                        Param::new(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x"))))))),
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("first".to_owned()))),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
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
        Err(SyntaxError::UnexpectedToken(token(TokenKind::StringLiteral("invalid".to_owned())))))]
    #[case::fn_without_body(
        vec![
            token(TokenKind::Fn),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::SemiColon),
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::SemiColon))))]
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
                                  Param::new(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x"))))))),
                                ],
                                vec![
                                    Shared::new(Node {
                                        token_id: 2.into(),
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
                    Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::array_invalid_token(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::Pipe),
                        token(TokenKind::RBracket),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::UnexpectedToken(token(TokenKind::Pipe))))]
    #[case::array_nested_unclosed(
                    vec![
                        token(TokenKind::LBracket),
                        token(TokenKind::LBracket),
                        token(TokenKind::StringLiteral("inner".to_owned())),
                        token(TokenKind::RBracket),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
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
                    Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
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
                    Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
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
                    Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
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
                                                        expr: Shared::new(Expr::Literal(Literal::Symbol(Ident::new("key")))),
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
                                                        expr: Shared::new(Expr::Literal(Literal::Symbol(Ident::new("a")))),
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
                                                        expr: Shared::new(Expr::Literal(Literal::Symbol(Ident::new("x")))),
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
                        Err(SyntaxError::ExpectedClosingBrace(token(TokenKind::Eof))))]
    #[case::dict_missing_colon(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::Ident(SmolStr::new("k"))),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(token(TokenKind::NumberLiteral(1.into())))))]
    #[case::dict_invalid_key(
                        vec![
                            token(TokenKind::LBrace),
                            token(TokenKind::NumberLiteral(1.into())),
                            token(TokenKind::Colon),
                            token(TokenKind::StringLiteral("v".to_owned())),
                            token(TokenKind::RBrace),
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(token(TokenKind::NumberLiteral(1.into())))))]
    #[case::attr_h_value(
        vec![
            token(TokenKind::Selector(".h".into())),
            token(TokenKind::Selector(".value".into())),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Call(IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Selector(".h".into()))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Selector(Selector::Heading(None))),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                        }),

                    ],
                ))})]))]
    #[case::attr(
        vec![
            token(TokenKind::Selector(".list".into())),
            token(TokenKind::Selector(".checked".into())),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Call(IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Selector(".list".into()))))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Selector(Selector::List(None, None))),
                        }),
                        Shared::new(Node {
                            token_id: 1.into(),
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
            Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
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
            Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
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
                    expr: Shared::new(Expr::And(
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::And(
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b")))))))),
                                }),
                            )),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("c", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("c")))))))),
                        }),
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
                    expr: Shared::new(Expr::Or(
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Or(
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("y", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("y")))))))),
                                }),
                            )),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("z", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("z")))))))),
                        }),
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
                    expr: Shared::new(Expr::Or(
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::And(
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b")))))))),
                                }),
                            )),
                        }),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("c", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("c")))))))),
                        }),
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
                Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::args_missing_rparen(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::StringLiteral("bar".to_owned())),
                    // Missing RParen
                    token(TokenKind::Eof)
                ],
                Err(SyntaxError::ExpectedClosingParen(token(TokenKind::Eof)))
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
                Err(SyntaxError::ExpectedClosingParen(token(TokenKind::Colon)))
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
                Err(SyntaxError::UnexpectedToken(token(TokenKind::Comma)))
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
                Err(SyntaxError::UnexpectedToken(token(TokenKind::Comma)))
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
                        expr: Shared::new(Expr::Or(
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
                Err(SyntaxError::ExpectedClosingBracket(token(TokenKind::Eof))))]
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
                Err(SyntaxError::UnexpectedToken(token(TokenKind::Eof))))]
    #[case::break_(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(None)),
                        })
                    ]))]
    #[case::break_with_number(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::NumberLiteral(42.into())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::Number(42.into()))),
                            })))),
                        })
                    ]))]
    #[case::break_with_string(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::StringLiteral("result".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("result".to_owned()))),
                            })))),
                        })
                    ]))]
    #[case::break_with_bool_true(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::BoolLiteral(true)),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                            })))),
                        })
                    ]))]
    #[case::break_with_bool_false(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::BoolLiteral(false)),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::Bool(false))),
                            })))),
                        })
                    ]))]
    #[case::break_with_self(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::Self_),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Self_),
                            })))),
                        })
                    ]))]
    #[case::break_with_function_call(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::Ident(SmolStr::new("contains"))),
                        token(TokenKind::LParen),
                        token(TokenKind::StringLiteral("test".to_owned())),
                        token(TokenKind::RParen),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 2.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token("contains", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("contains")))))),
                                    smallvec![Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("test".to_owned()))),
                                    })],
                                )),
                            })))),
                        })
                    ]))]
    #[case::break_with_nested_call(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::Ident(SmolStr::new("and"))),
                        token(TokenKind::LParen),
                        token(TokenKind::BoolLiteral(true)),
                        token(TokenKind::Comma),
                        token(TokenKind::BoolLiteral(false)),
                        token(TokenKind::RParen),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 3.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::AND, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("and")))))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 1.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Bool(true))),
                                        }),
                                        Shared::new(Node {
                                            token_id: 2.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Bool(false))),
                                        })
                                    ],
                                )),
                            })))),
                        })
                    ]))]
    #[case::break_with_selector(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::Selector(SmolStr::new(".h1"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Selector(Selector::Heading(Some(1)))),
                            })))),
                        })
                    ]))]
    #[case::break_with_none(
                    vec![
                        token(TokenKind::Break),
                        token(TokenKind::Colon),
                        token(TokenKind::None),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Break(Some(Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::None)),
                            })))),
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
    #[case::try_without_catch(
            vec![
                token(TokenKind::Try),
                token(TokenKind::Colon),
                token(TokenKind::Ident(SmolStr::new("error_expr"))),
                token(TokenKind::Eof),
            ],
            Ok(vec![Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::Try(
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("error_expr", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("error_expr")))))))),
                    }),
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Literal(Literal::None)),
                    }),
                )),
            })])
        )]
    // Test index access followed by function call (e.g., arr[0]())
    #[case::index_access_with_function_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::CallDynamic(
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
                                    expr: Shared::new(Expr::Literal(Literal::Number(0.into()))),
                                }),
                            ],
                        )),
                    }),
                    SmallVec::new(),
                )),
            })
        ]))]
    // Test index access with args followed by function call (e.g., arr[0](arg))
    #[case::index_access_with_function_call_and_args(
        vec![
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(Expr::CallDynamic(
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
                                    expr: Shared::new(Expr::Literal(Literal::Number(0.into()))),
                                }),
                            ],
                        )),
                    }),
                    smallvec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Shared::new(Expr::Literal(Literal::String("test".to_owned()))),
                        })
                    ],
                )),
            })
        ]))]
    // Test chained index access followed by function call (e.g., arr[0][1]())
    #[case::chained_index_access_with_function_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(Expr::CallDynamic(
                    Shared::new(Node {
                        token_id: 4.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::GET, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arr")))))),
                            smallvec![
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
                    }),
                    SmallVec::new(),
                )),
            })
        ]))]
    #[case::function_call_with_question_mark(
            vec![
                token(TokenKind::Ident(SmolStr::new("foo"))),
                token(TokenKind::LParen),
                token(TokenKind::StringLiteral("arg".to_owned())),
                token(TokenKind::RParen),
                token(TokenKind::Question),
                token(TokenKind::Eof),
            ],
            Ok(vec![Shared::new(Node {
                token_id: 1.into(),
                expr: Shared::new(Expr::Try(
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
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Literal(Literal::None)),
                    }),
                )),
            })])
        )]
    #[case::question_mark_after_call(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::LParen),
                    token(TokenKind::RParen),
                    token(TokenKind::Question),
                    token(TokenKind::Eof),
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Try(
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token("foo", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("foo")))))),
                                    SmallVec::new(),
                                )),
                            }),
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Literal(Literal::None)),
                            }),
                        )),
                    })
                ]))]
    #[case::question_mark_after_call_with_args(
                vec![
                    token(TokenKind::Ident(SmolStr::new("bar"))),
                    token(TokenKind::LParen),
                    token(TokenKind::StringLiteral("arg".to_owned())),
                    token(TokenKind::RParen),
                    token(TokenKind::Question),
                    token(TokenKind::Eof),
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Try(
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token("bar", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("bar")))))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Literal(Literal::String("arg".to_owned()))),
                                        }),
                                    ],
                                )),
                            }),
                            Shared::new(Node {
                                token_id: 1.into(),
                                expr: Shared::new(Expr::Literal(Literal::None)),
                            }),
                        )),
                    })
                ]))]
    #[case::question_mark_after_call_error(
                vec![
                    token(TokenKind::Ident(SmolStr::new("foo"))),
                    token(TokenKind::Question),
                    token(TokenKind::Eof),
                ],
                Err(SyntaxError::UnexpectedToken(token(TokenKind::Ident("foo".into())))))]
    #[case::coalesce_simple(
                    vec![
                        token(TokenKind::StringLiteral("foo".to_owned())),
                        token(TokenKind::Coalesce),
                        token(TokenKind::StringLiteral("bar".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::COALESCE, Some(Shared::new(token(TokenKind::Coalesce)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("foo".to_owned()))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("bar".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::coalesce_with_none(
                    vec![
                        token(TokenKind::None),
                        token(TokenKind::Coalesce),
                        token(TokenKind::StringLiteral("default".to_owned())),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::COALESCE, Some(Shared::new(token(TokenKind::Coalesce)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Literal(Literal::None)),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("default".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::coalesce_with_identifiers(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("x"))),
                        token(TokenKind::Coalesce),
                        token(TokenKind::Ident(SmolStr::new("y"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::COALESCE, Some(Shared::new(token(TokenKind::Coalesce)))),
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
    #[case::coalesce_error_missing_rhs(
                    vec![
                        token(TokenKind::StringLiteral("foo".to_owned())),
                        token(TokenKind::Coalesce),
                        token(TokenKind::Eof)
                    ],
                    Err(SyntaxError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::negate_simple(
        vec![
            token(TokenKind::Minus),
            token(TokenKind::NumberLiteral(42.into())),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::NEGATE, Some(Shared::new(token(TokenKind::Minus)))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Literal(Literal::Number(42.into()))),
                        }),
                    ],
                )),
            })
        ]))]
    #[case::negate_with_identifier(
        vec![
            token(TokenKind::Minus),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Call(
                    IdentWithToken::new_with_token(constants::NEGATE, Some(Shared::new(token(TokenKind::Minus)))),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                        }),
                    ],
                )),
            })
        ]))]
    #[case::negate_error_missing_rhs(
        vec![
            token(TokenKind::Minus),
            token(TokenKind::Eof)
        ],
        Err(SyntaxError::UnexpectedToken(token(TokenKind::Eof))))]
    #[case::import_simple(
            vec![
            token(TokenKind::Import),
            token(TokenKind::StringLiteral("name".to_owned())),
            token(TokenKind::Eof),
            ],
            Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Import(
                    Literal::String("name".to_owned()),
                )),
            })
            ]))]
    #[case::import_as(
            vec![
            token(TokenKind::Import),
            token(TokenKind::StringLiteral("name".to_owned())),
            token(TokenKind::Eof),
            ],
            Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Import(
                    Literal::String("name".to_owned()),
                )),
            })
            ]))]
    #[case::qualified_access(
            vec![
            token(TokenKind::Ident(SmolStr::new("test"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("foo"))),
            token(TokenKind::Eof),
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(Expr::QualifiedAccess(
                        vec![IdentWithToken::new_with_token("test", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("test"))))))],
                        AccessTarget::Ident(IdentWithToken::new_with_token("foo", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("foo")))))),
                    ))),
                })
            ]))]
    #[case::qualified_access_with_call(
        vec![
            token(TokenKind::Ident(SmolStr::new("mod"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("func"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("arg".to_owned())),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(Expr::QualifiedAccess(
                    vec![IdentWithToken::new_with_token("mod", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("mod"))))))],
                    AccessTarget::Call(
                        IdentWithToken::new_with_token("func", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("func")))))),
                        smallvec![
                            Shared::new(Node {
                                token_id: 0.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("arg".to_owned()))),
                            }),
                        ],
                    ),
                )),
            })
        ]))]
    #[case::qualified_access_multi_level(
        vec![
            token(TokenKind::Ident(SmolStr::new("mod1"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("mod2"))),
            token(TokenKind::DoubleColon),
            token(TokenKind::Ident(SmolStr::new("func"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(Expr::QualifiedAccess(
                    vec![
                        IdentWithToken::new_with_token("mod1", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("mod1")))))),
                        IdentWithToken::new_with_token("mod2", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("mod2")))))),
                    ],
                    AccessTarget::Call(
                        IdentWithToken::new_with_token("func", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("func")))))),
                        smallvec![],
                    ),
                )),
            })
        ]))]
    #[case::slice_access_with_start_only(
            vec![
                token(TokenKind::Ident(SmolStr::new("arr"))),
                token(TokenKind::LBracket),
                token(TokenKind::NumberLiteral(1.into())),
                token(TokenKind::Colon),
                token(TokenKind::RBracket),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Shared::new(Node {
                    token_id: 4.into(),
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
                                token_id: 3.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token("len", None),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 0.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("arr", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("arr")))))))),
                                        }),
                                    ],
                                )),
                            }),
                        ],
                    )),
                })
            ]))]
    #[case::selector_dot_is_self(
                vec![
                    token(TokenKind::Selector(SmolStr::new("."))),
                    token(TokenKind::Eof)
                ],
                Ok(vec![
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(Expr::Self_),
                    })
                ]))]
    #[case::ident_with_single_attr(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("obj"))),
                        token(TokenKind::Selector(SmolStr::new(".name"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("obj")))))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("obj", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("obj")))))))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("name".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::function_call_result_with_attr(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("get_user"))),
                        token(TokenKind::LParen),
                        token(TokenKind::RParen),
                        token(TokenKind::Selector(SmolStr::new(".name"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("get_user")))))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Call(
                                            IdentWithToken::new_with_token("get_user", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("get_user")))))),
                                            SmallVec::new(),
                                        )),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("name".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::ident_with_attr_in_pipe(
                    vec![
                        token(TokenKind::Ident(SmolStr::new("data"))),
                        token(TokenKind::Pipe),
                        token(TokenKind::Ident(SmolStr::new("obj"))),
                        token(TokenKind::Selector(SmolStr::new(".value"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("data", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("data")))))))),
                        }),
                        Shared::new(Node {
                            token_id: 3.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Ident(SmolStr::new("obj")))))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("obj", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("obj")))))))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 2.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::self_with_attr(
                    vec![
                        token(TokenKind::Self_),
                        token(TokenKind::Selector(SmolStr::new(".value"))),
                        token(TokenKind::Eof)
                    ],
                    Ok(vec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(Expr::Call(
                                IdentWithToken::new_with_token(constants::ATTR, Some(Shared::new(token(TokenKind::Self_)))),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(Expr::Self_),
                                    }),
                                    Shared::new(Node {
                                        token_id: 1.into(),
                                        expr: Shared::new(Expr::Literal(Literal::String("value".to_owned()))),
                                    }),
                                ],
                            )),
                        })
                    ]))]
    #[case::let_with_reserved_keyword_as_value(
                        vec![
                            token(TokenKind::Let),
                            token(TokenKind::Ident(SmolStr::new("aaa"))),
                            token(TokenKind::Equal),
                            token(TokenKind::Let), // Using "let" as a value (should error)
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(Token {
                            range: Range::default(),
                            kind: TokenKind::Let,
                            module_id: 1.into(),
                        }))
                    )]
    #[case::let_with_reserved_keyword_as_variable_and_value(
                        vec![
                            token(TokenKind::Let),
                            token(TokenKind::Let), // Using "let" as a variable name (should error)
                            token(TokenKind::Equal),
                            token(TokenKind::Ident(SmolStr::new("vvv"))),
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(Token {
                            range: Range::default(),
                            kind: TokenKind::Let,
                            module_id: 1.into(),
                        }))
                    )]
    #[case::let_with_reserved_keyword_as_variable_and_value2(
                        vec![
                            token(TokenKind::Let),
                            token(TokenKind::Let), // Using "let" as a variable name (should error)
                            token(TokenKind::Equal),
                            token(TokenKind::Let), // Using "let" as a value (should error)
                            token(TokenKind::Eof)
                        ],
                        Err(SyntaxError::UnexpectedToken(Token {
                            range: Range::default(),
                            kind: TokenKind::Let,
                            module_id: 1.into(),
                        }))
                    )]
    #[case::macro_basic(
        vec![
            token(TokenKind::Macro),
            token(TokenKind::Ident(SmolStr::new("double"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Plus),
            token(TokenKind::Ident(SmolStr::new("x"))),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Macro(
                    IdentWithToken::new_with_token("double", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("double")))))),
                    smallvec![
                        Param::new(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))
                    ],
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ADD, Some(Shared::new(token(TokenKind::Plus)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 4.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                })
                            ]
                        ))
                    }),
                )),
            }),
        ]))]
    #[case::macro_with_end(
        vec![
            token(TokenKind::Macro),
            token(TokenKind::Ident(SmolStr::new("triple"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Plus),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Plus),
            token(TokenKind::Ident(SmolStr::new("x"))),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Macro(
                    IdentWithToken::new_with_token("triple", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("triple")))))),
                    smallvec![
                        Param::new(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))
                    ],
                    Shared::new(Node {
                        token_id: 5.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ADD, Some(Shared::new(token(TokenKind::Plus)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 3.into(),
                                    expr: Shared::new(Expr::Call(
                                        IdentWithToken::new_with_token(constants::ADD, Some(Shared::new(token(TokenKind::Plus)))),
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 2.into(),
                                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                            }),
                                            Shared::new(Node {
                                                token_id: 4.into(),
                                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                            })
                                        ]
                                    ))
                                }),
                                Shared::new(Node {
                                    token_id: 6.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("x")))))))),
                                })
                            ]
                        ))
                    }),
                )),
            }),
        ]))]
    #[case::macro_multiple_params(
        vec![
            token(TokenKind::Macro),
            token(TokenKind::Ident(SmolStr::new("add_two"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("a"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("b"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("a"))),
            token(TokenKind::Plus),
            token(TokenKind::Ident(SmolStr::new("b"))),
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Macro(
                    IdentWithToken::new_with_token("add_two", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("add_two")))))),
                    smallvec![
                        Param::new(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a"))))))),
                        Param::new(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b"))))))),
                    ],
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(Expr::Call(
                            IdentWithToken::new_with_token(constants::ADD, Some(Shared::new(token(TokenKind::Plus)))),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 2.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("a")))))))),
                                }),
                                Shared::new(Node {
                                    token_id: 4.into(),
                                    expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(token(TokenKind::Ident(SmolStr::new("b")))))))),
                                })
                            ]
                        ))
                    }),
                )),
            }),
        ]))]
    fn test_parse(#[case] input: Vec<Token>, #[case] expected: Result<Program, SyntaxError>) {
        let mut arena = Arena::new(10);
        let tokens: Vec<Shared<Token>> = input.into_iter().map(Shared::new).collect();
        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

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
    #[case::code(".code", Selector::Code)]
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
        let mut arena = Arena::new(10);
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

        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

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
        let mut arena = Arena::new(10);
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

        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

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

        let mut arena = Arena::new(10);
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

        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

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
        let mut arena = Arena::new(10);
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

        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

        assert!(matches!(
            result,
            Err(SyntaxError::EnvNotFound(_, var)) if var == "MQ_NONEXISTENT_VAR"
        ));
    }

    #[test]
    fn test_parse_env_in_arguments() {
        unsafe { std::env::set_var("MQ_ARG_TEST", "env_arg_value") };

        let mut arena = Arena::new(10);
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

        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

        match result {
            Ok(program) => {
                assert_eq!(program.len(), 1);
                if let Expr::Call(ident, args) = &*program[0].expr {
                    assert_eq!(ident.name, "function".into());
                    assert_eq!(args.len(), 1);
                    if let Expr::Literal(Literal::String(value)) = &*args[0].expr {
                        assert_eq!(value, "env_arg_value");
                    } else {
                        panic!("Expected String literal in argument, got {:?}", args[0].expr);
                    }
                } else {
                    panic!("Expected Call expression, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }

    #[rstest]
    #[case::h_value(vec![".h", ".value"], "h", "value")]
    #[case::h1_value(vec![".h1", ".value"], "h1", "value")]
    #[case::code_lang(vec![".code", ".lang"], "code", "lang")]
    #[case::text_value(vec![".text", ".value"], "text", "value")]
    fn test_parse_selector_with_attribute(
        #[case] selectors: Vec<&str>,
        #[case] base_selector: &str,
        #[case] attribute: &str,
    ) {
        let mut arena = Arena::new(10);
        let mut tokens = selectors
            .iter()
            .map(|selector| {
                Shared::new(Token {
                    range: Range::default(),
                    kind: TokenKind::Selector(SmolStr::new(selector)),
                    module_id: 1.into(),
                })
            })
            .collect::<Vec<_>>();

        tokens.push(Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: 1.into(),
        }));

        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

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
                            "code" => assert_eq!(*selector, Selector::Code),
                            "text" => assert_eq!(*selector, Selector::Text),
                            _ => panic!("Unexpected base selector: {}", base_selector),
                        }
                    } else {
                        panic!("Expected Selector expression in first argument, got {:?}", args[0].expr);
                    }

                    // Second argument should be the attribute string
                    if let Expr::Literal(Literal::String(attr_str)) = &*args[1].expr {
                        assert_eq!(attr_str, attribute);
                    } else {
                        panic!("Expected String literal in second argument, got {:?}", args[1].expr);
                    }
                } else {
                    panic!("Expected Call expression, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }

    #[rstest]
    #[case::match_simple_literal(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("one".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("two".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("other".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(Token {
                            range: Range::default(),
                            kind: TokenKind::Ident(SmolStr::new("x")),
                            module_id: 1.into()
                        })))))
                    }),
                    smallvec![
                        MatchArm {
                            pattern: Pattern::Literal(Literal::Number(1.into())),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("one".to_owned())))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Literal(Literal::Number(2.into())),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("two".to_owned())))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 11.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("other".to_owned())))
                            })
                        }
                    ]
                ))
            })
        ]))]
    #[case::match_type_pattern(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("value"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("string"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("is string".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("number"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("is number".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("value", Some(Shared::new(Token {
                            range: Range::default(),
                            kind: TokenKind::Ident(SmolStr::new("value")),
                            module_id: 1.into()
                        })))))
                    }),
                    smallvec![
                        MatchArm {
                            pattern: Pattern::Type(Ident::new("string")),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("is string".to_owned())))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Type(Ident::new("number")),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("is number".to_owned())))
                            })
                        }
                    ]
                ))
            })
        ]))]
    #[case::match_array_pattern(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::LBracket),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("y"))),
            token(TokenKind::RBracket),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("two elements".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("arr", Some(Shared::new(Token {
                            range: Range::default(),
                            kind: TokenKind::Ident(SmolStr::new("arr")),
                            module_id: 1.into()
                        })))))
                    }),
                    smallvec![
                        MatchArm {
                            pattern: Pattern::Array(vec![
                                Pattern::Ident(IdentWithToken::new("x")),
                                Pattern::Ident(IdentWithToken::new("y"))
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("two elements".to_owned())))
                            })
                        }
                    ]
                ))
            })
        ]))]
    #[case::match_array_rest_pattern(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("arr"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::LBracket),
            token(TokenKind::Ident(SmolStr::new("first"))),
            token(TokenKind::Comma),
            token(TokenKind::RangeOp),
            token(TokenKind::Ident(SmolStr::new("rest"))),
            token(TokenKind::RBracket),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("first"))),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("arr", Some(Shared::new(Token {
                            range: Range::default(),
                            kind: TokenKind::Ident(SmolStr::new("arr")),
                            module_id: 1.into()
                        })))))
                    }),
                    smallvec![
                        MatchArm {
                            pattern: Pattern::ArrayRest(
                                vec![Pattern::Ident(IdentWithToken::new("first"))],
                                IdentWithToken::new("rest")
                            ),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("first", Some(Shared::new(Token {
                                    range: Range::default(),
                                    kind: TokenKind::Ident(SmolStr::new("first")),
                                    module_id: 1.into()
                                })))))
                            })
                        }
                    ]
                ))
            })
        ]))]
    #[case::match_dict_pattern(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("obj"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::LBrace),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(SmolStr::new("age"))),
            token(TokenKind::RBrace),
            token(TokenKind::Colon),
            token(TokenKind::Ident(SmolStr::new("name"))),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("obj", Some(Shared::new(Token {
                            range: Range::default(),
                            kind: TokenKind::Ident(SmolStr::new("obj")),
                            module_id: 1.into()
                        })))))
                    }),
                    smallvec![
                        MatchArm {
                            pattern: Pattern::Dict(vec![
                                (IdentWithToken::new("name"), Pattern::Ident(IdentWithToken::new("name"))),
                                (IdentWithToken::new("age"), Pattern::Ident(IdentWithToken::new("age")))
                            ]),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("name", Some(Shared::new(Token {
                                    range: Range::default(),
                                    kind: TokenKind::Ident(SmolStr::new("name")),
                                    module_id: 1.into()
                                })))))
                            })
                        }
                    ]
                ))
            })
        ]))]
    #[case::match_with_guard(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("n"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::If),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Gt),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("positive".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::If),
            token(TokenKind::LParen),
            token(TokenKind::Ident(SmolStr::new("x"))),
            token(TokenKind::Lt),
            token(TokenKind::NumberLiteral(0.into())),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("negative".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("zero".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("n", Some(Shared::new(Token {
                            range: Range::default(),
                            kind: TokenKind::Ident(SmolStr::new("n")),
                            module_id: 1.into()
                        })))))
                    }),
                    smallvec![
                        MatchArm {
                            pattern: Pattern::Ident(IdentWithToken::new("x")),
                            guard: Some(Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::GT, Some(Shared::new(token(TokenKind::Gt)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 4.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(Token {
                                                range: Range::default(),
                                                kind: TokenKind::Ident(SmolStr::new("x")),
                                                module_id: 1.into()
                                            })))))
                                        }),
                                        Shared::new(Node {
                                            token_id: 6.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Number(0.into())))
                                        })
                                    ]
                                ))
                            })),
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("positive".to_owned())))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Ident(IdentWithToken::new("x")),
                            guard: Some(Shared::new(Node {
                                token_id: 11.into(),
                                expr: Shared::new(Expr::Call(
                                    IdentWithToken::new_with_token(constants::LT, Some(Shared::new(token(TokenKind::Lt)))),
                                    smallvec![
                                        Shared::new(Node {
                                            token_id: 10.into(),
                                            expr: Shared::new(Expr::Ident(IdentWithToken::new_with_token("x", Some(Shared::new(Token {
                                                range: Range::default(),
                                                kind: TokenKind::Ident(SmolStr::new("x")),
                                                module_id: 1.into()
                                            })))))
                                        }),
                                        Shared::new(Node {
                                            token_id: 12.into(),
                                            expr: Shared::new(Expr::Literal(Literal::Number(0.into())))
                                        })
                                    ]
                                ))
                            })),
                            body: Shared::new(Node {
                                token_id: 14.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("negative".to_owned())))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 17.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("zero".to_owned())))
                            })
                        }
                    ]
                ))
            })
        ]))]
    #[case::match_do_end(
        vec![
            token(TokenKind::Match),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RParen),
            token(TokenKind::Do),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(1.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("one".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("two".to_owned())),
            token(TokenKind::Pipe),
            token(TokenKind::Ident(SmolStr::new("_"))),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("other".to_owned())),
            token(TokenKind::End),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(Expr::Match(
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(Expr::Literal(Literal::Number(2.into())))
                    }),
                    smallvec![
                        MatchArm {
                            pattern: Pattern::Literal(Literal::Number(1.into())),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 5.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("one".to_owned())))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Literal(Literal::Number(2.into())),
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 8.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("two".to_owned())))
                            })
                        },
                        MatchArm {
                            pattern: Pattern::Wildcard,
                            guard: None,
                            body: Shared::new(Node {
                                token_id: 11.into(),
                                expr: Shared::new(Expr::Literal(Literal::String("other".to_owned())))
                            })
                        }
                    ]
                ))
            })
        ]))]
    fn test_parse_match(#[case] input: Vec<Token>, #[case] expected: Result<Program, SyntaxError>) {
        let mut arena = Arena::new(10);
        let tokens: Vec<Shared<Token>> = input.into_iter().map(Shared::new).collect();
        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::expr_literal(
        vec![
            lexer::token::StringSegment::Text("Value: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("42".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Value: ") &&
            matches!(&segments[1], StringSegment::Expr(node) if matches!(&*node.expr, Expr::Literal(Literal::Number(_))))
        }
    )]
    #[case::expr_string(
        vec![
            lexer::token::StringSegment::Text("Result: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("\"hello\"".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Result: ") &&
            matches!(&segments[1], StringSegment::Expr(node) if matches!(&*node.expr, Expr::Literal(Literal::String(s)) if s == "hello"))
        }
    )]
    #[case::expr_call(
        vec![
            lexer::token::StringSegment::Text("Result: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("add(1, 2)".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Result: ") &&
            if let StringSegment::Expr(node) = &segments[1] {
                if let Expr::Call(ident, args) = &*node.expr {
                    ident.name == "add".into() && args.len() == 2
                } else {
                    false
                }
            } else {
                false
            }
        }
    )]
    #[case::expr_self(
        vec![
            lexer::token::StringSegment::Text("Value: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("self".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Value: ") &&
            matches!(&segments[1], StringSegment::Self_)
        }
    )]
    #[case::expr_env(
        vec![
            lexer::token::StringSegment::Text("Env: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("$MQ_TEST_INTERPOLATION".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            unsafe { std::env::set_var("MQ_TEST_INTERPOLATION", "test_value") };
            matches!(&segments[0], StringSegment::Text(s) if s == "Env: ") &&
            matches!(&segments[1], StringSegment::Env(var) if var == "MQ_TEST_INTERPOLATION")
        }
    )]
    #[case::multiple_exprs(
        vec![
            lexer::token::StringSegment::Text("A: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("1".into(), Range::default()),
            lexer::token::StringSegment::Text(", B: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("2".into(), Range::default()),
        ],
        4,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "A: ") &&
            matches!(&segments[1], StringSegment::Expr(_)) &&
            matches!(&segments[2], StringSegment::Text(s) if s == ", B: ") &&
            matches!(&segments[3], StringSegment::Expr(_))
        }
    )]
    #[case::mixed_segments(
        vec![
            lexer::token::StringSegment::Text("Literal, ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("self".into(), Range::default()),
            lexer::token::StringSegment::Text(", ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("$MQ_MIXED_TEST".into(), Range::default()),
            lexer::token::StringSegment::Text(", ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("42".into(), Range::default()),
        ],
        6,
        |segments: &[StringSegment]| {
            unsafe { std::env::set_var("MQ_MIXED_TEST", "env_value") };
            matches!(&segments[0], StringSegment::Text(s) if s == "Literal, ") &&
            matches!(&segments[1], StringSegment::Self_) &&
            matches!(&segments[2], StringSegment::Text(s) if s == ", ") &&
            matches!(&segments[3], StringSegment::Env(var) if var == "MQ_MIXED_TEST") &&
            matches!(&segments[4], StringSegment::Text(s) if s == ", ") &&
            matches!(&segments[5], StringSegment::Expr(_))
        }
    )]
    #[case::expr_bool(
        vec![
            lexer::token::StringSegment::Text("Bool: ".to_string(), Range::default()),
            lexer::token::StringSegment::Expr("true".into(), Range::default()),
        ],
        2,
        |segments: &[StringSegment]| {
            matches!(&segments[0], StringSegment::Text(s) if s == "Bool: ") &&
            matches!(&segments[1], StringSegment::Expr(node) if matches!(&*node.expr, Expr::Literal(Literal::Bool(true))))
        }
    )]
    fn test_parse_interpolated_string_with_expr(
        #[case] input_segments: Vec<lexer::token::StringSegment>,
        #[case] expected_len: usize,
        #[case] validator: fn(&[StringSegment]) -> bool,
    ) {
        let mut arena = Arena::new(10);
        let tokens = [
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::InterpolatedString(input_segments),
                module_id: 1.into(),
            }),
            Shared::new(Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: 1.into(),
            }),
        ];

        let result = Parser::new(tokens.iter(), &mut arena, Module::TOP_LEVEL_MODULE_ID).parse();

        match result {
            Ok(program) => {
                assert_eq!(program.len(), 1);
                if let Expr::InterpolatedString(segments) = &*program[0].expr {
                    assert_eq!(segments.len(), expected_len);
                    assert!(validator(segments), "Validator failed for segments");
                } else {
                    panic!("Expected InterpolatedString, got {:?}", program[0].expr);
                }
            }
            Err(err) => panic!("Parse error: {:?}", err),
        }
    }
}
