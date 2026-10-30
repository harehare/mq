use crate::arena::Arena;
use crate::ast::node::{IdentWithToken, MatchArm, Pattern};
use crate::error::syntax::SyntaxError;
use crate::lexer::Lexer;
use crate::lexer::token::{Token, TokenKind};
use crate::module::ModuleId;
use crate::range::Position;
use crate::runtime::builtin::io_context;
use crate::selector::Selector;
use crate::{Ident, Shared, lexer};
use smallvec::{SmallVec, smallvec};
use smol_str::SmolStr;
use std::iter::Peekable;
use std::sync::LazyLock;

use super::constants;
use super::node::{AccessTarget, Args, BinaryOp, Branches, Expr, Literal, Node, Param, Params, UnaryOp};
use super::{Program, TokenId};

type IfExpr = (Option<Shared<Node>>, Shared<Node>);

/// Maximum expression nesting depth accepted by the parser.
pub(crate) const MAX_PARSE_DEPTH: usize = 512;

static GET_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::from(constants::builtins::GET));

/// How a program ends.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProgramKind {
    /// Top level, ends at EOF.
    Root,
    /// Nested body, ends at `;` or `end`, or before a `,` or closing bracket owned by the enclosing construct.
    Block,
}

/// A lexer token held by the token arena.
#[derive(Clone, Copy)]
struct RetainedToken {
    id: TokenId,
    /// Whether a node already owns `id`.
    node_owned: bool,
}

/// Parses a token stream into an AST, allocating tokens in the arena.
pub struct Parser<'a, 'alloc> {
    tokens: Peekable<core::slice::Iter<'a, Token>>,
    token_base_address: usize,
    token_cache: Vec<Option<RetainedToken>>,
    token_arena: &'alloc mut Arena<Token>,
    module_id: ModuleId,
    depth: usize,
}

impl<'a, 'alloc> Parser<'a, 'alloc> {
    /// Creates a parser over `tokens` for the module `module_id`.
    pub fn new(
        tokens: core::slice::Iter<'a, Token>,
        token_arena: &'alloc mut Arena<Token>,
        module_id: ModuleId,
    ) -> Self {
        let token_slice = tokens.as_slice();
        Self {
            tokens: tokens.peekable(),
            token_base_address: token_slice.as_ptr() as usize,
            token_cache: vec![None; token_slice.len()],
            token_arena,
            module_id,
            depth: 0,
        }
    }

    /// Parses the whole input into a program.
    pub fn parse(&mut self) -> Result<Program, SyntaxError> {
        self.parse_program(ProgramKind::Root)
    }

    /// Index of `token` in the input slice, which is stable for the duration of parsing and so
    /// replaces a hash-table lookup per token.
    fn token_index(&self, token: &Token) -> usize {
        let token_size = std::mem::size_of::<Token>();
        let byte_offset = (token as *const Token as usize)
            .checked_sub(self.token_base_address)
            .expect("parser tokens belong to its input slice");
        debug_assert_eq!(byte_offset % token_size, 0);
        let index = byte_offset / token_size;
        debug_assert!(index < self.token_cache.len());
        index
    }

    /// Retains `token` for a node and returns the node's arena identifier.
    ///
    /// Each node owns its identifier, because debugger instrumentation resolves a node from it
    /// and a postfix call built from the same lexer token as its callee must stay distinct. An
    /// entry already retained for an identifier goes to the first node that asks, so a node and
    /// its identifier share one entry.
    fn alloc_token(&mut self, token: &Token) -> TokenId {
        let index = self.token_index(token);
        match &mut self.token_cache[index] {
            Some(retained) if !retained.node_owned => {
                retained.node_owned = true;
                retained.id
            }
            slot => {
                let id = self.token_arena.alloc(token.clone());
                slot.get_or_insert(RetainedToken { id, node_owned: true });
                id
            }
        }
    }

    /// Retains `token` for an identifier and returns its arena identifier, reusing the entry
    /// retained for the node built from the same lexer token.
    fn alloc_ident_token(&mut self, token: &Token) -> TokenId {
        let index = self.token_index(token);
        self.token_cache[index]
            .get_or_insert_with(|| RetainedToken {
                id: self.token_arena.alloc(token.clone()),
                node_owned: false,
            })
            .id
    }

    fn parse_program(&mut self, kind: ProgramKind) -> Result<Program, SyntaxError> {
        let root = kind == ProgramKind::Root;
        let mut asts = Vec::with_capacity(64);

        // Initial check for invalid starting tokens in a program.
        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::Pipe | TokenKind::SemiColon => {
                    return Err(SyntaxError::UnexpectedToken((**token).clone()));
                }
                TokenKind::Comma | TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace if !root => {
                    return Err(SyntaxError::UnexpectedToken((**token).clone()));
                }
                TokenKind::End => {
                    return Err(SyntaxError::UnmatchedEnd((**token).clone()));
                }
                _ => {}
            },
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        while !(!root && self.at_body_end())
            && let Some(token) = self.tokens.next()
        {
            match &token.kind {
                // A pipe separates the next expression in the chain. EOF uses the same
                // missing-right-hand diagnostic as `1 +`. A terminator would otherwise end
                // the program and drop the pipe.
                TokenKind::Pipe => match self.tokens.peek().map(|next| &next.kind) {
                    Some(TokenKind::Eof) | None => return Err(SyntaxError::UnexpectedEOFAfterToken(token.clone())),
                    Some(TokenKind::SemiColon | TokenKind::End) => {
                        return Err(SyntaxError::UnexpectedToken(token.clone()));
                    }
                    Some(TokenKind::Pipe) => {
                        let next = self.next_or_eof()?;
                        return Err(SyntaxError::UnexpectedToken(next.clone()));
                    }
                    Some(TokenKind::Comma | TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace) if !root => {
                        return Err(SyntaxError::UnexpectedToken(token.clone()));
                    }
                    _ => continue,
                },
                TokenKind::Eof => break, // End of file terminates the program.
                TokenKind::SemiColon | TokenKind::End => {
                    // Semicolons and 'end' keyword terminate sub-programs (e.g., in 'def', 'fn').
                    // In the root program, after a semicolon or 'end', the parser checks if the next token is EOF.
                    // If the next token is not EOF, it returns an error for the unexpected token.
                    if root {
                        match self.tokens.peek() {
                            Some(next_token) if !matches!(next_token.kind, TokenKind::Eof) => {
                                if matches!(token.kind, TokenKind::End) {
                                    return Err(SyntaxError::UnmatchedEnd(token.clone()));
                                } else {
                                    return Err(SyntaxError::UnexpectedToken((**next_token).clone()));
                                }
                            }
                            _ => break,
                        }
                    }
                    // For non-root programs (e.g. function bodies), a semicolon/end explicitly ends the program.
                    break;
                }
                TokenKind::Nodes if root => {
                    let ast = self.parse_all_nodes(token)?;
                    self.expect_statement_end(&ast)?;
                    asts.push(ast);
                }
                TokenKind::Nodes => {
                    return Err(SyntaxError::UnexpectedToken(token.clone()));
                }
                TokenKind::NewLine | TokenKind::Tab(_) | TokenKind::Whitespace(_) => {
                    unreachable!("parse_program should have filtered out whitespace tokens")
                }
                _ => {
                    let ast = self.parse_expr(token)?;
                    self.expect_statement_end(&ast)?;
                    asts.push(ast);
                }
            }
        }

        if asts.is_empty() {
            return Err(SyntaxError::UnexpectedEOFDetected(self.module_id));
        }

        Ok(asts)
    }

    /// Rejects a statement directly followed by another one. Only definitions, blocks that consume their own
    /// closing `;` or `end`, and `import`/`include` may be followed by a statement without a `|`.
    /// Keep in sync with `closes_itself` in the CST parser.
    fn expect_statement_end(&mut self, ast: &Node) -> Result<(), SyntaxError> {
        // Selector chains such as `.a .b` are also `Block`s, so only a `do` block counts.
        let is_do_block =
            matches!(ast.expr, Expr::Block(..)) && matches!(self.token_arena[ast.token_id].kind, TokenKind::Do);

        if is_do_block
            || matches!(
                ast.expr,
                Expr::Def(..)
                    | Expr::Fn(..)
                    | Expr::Foreach(..)
                    | Expr::While(..)
                    | Expr::Until(..)
                    | Expr::Loop(..)
                    | Expr::Match(..)
                    | Expr::Module(..)
                    | Expr::Import(..)
                    | Expr::Include(..)
            )
        {
            return Ok(());
        }

        match self.tokens.peek() {
            Some(token)
                if !matches!(
                    token.kind,
                    TokenKind::Pipe
                        | TokenKind::Eof
                        | TokenKind::SemiColon
                        | TokenKind::End
                        | TokenKind::Comma
                        | TokenKind::RParen
                        | TokenKind::RBracket
                        | TokenKind::RBrace
                ) =>
            {
                Err(SyntaxError::UnexpectedToken((**token).clone()))
            }
            _ => Ok(()),
        }
    }

    /// True at a `,` or closing bracket, which no statement can contain. The enclosing call, array
    /// or dict owns it. EOF also counts, so the enclosing construct reports its own missing closer.
    fn at_body_end(&mut self) -> bool {
        self.tokens.peek().is_some_and(|token| {
            matches!(
                token.kind,
                TokenKind::Comma | TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace | TokenKind::Eof
            )
        })
    }

    #[inline(always)]
    fn parse_expr(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        self.parse_equality_expr(token)
    }

    fn binary_op_function_name(kind: &TokenKind) -> Option<&'static str> {
        Some(match kind {
            TokenKind::Asterisk => constants::builtins::MUL,
            TokenKind::Coalesce => constants::builtins::COALESCE,
            TokenKind::EqEq => constants::builtins::EQ,
            TokenKind::Gte => constants::builtins::GTE,
            TokenKind::Gt => constants::builtins::GT,
            TokenKind::Lte => constants::builtins::LTE,
            TokenKind::Lt => constants::builtins::LT,
            TokenKind::Minus => constants::builtins::SUB,
            TokenKind::NeEq => constants::builtins::NE,
            TokenKind::Percent => constants::builtins::MOD,
            TokenKind::Plus => constants::builtins::ADD,
            TokenKind::DoubleDot => constants::builtins::RANGE,
            TokenKind::Slash => constants::builtins::DIV,
            TokenKind::TildeEqual => constants::builtins::IS_REGEX_MATCH,
            TokenKind::NotTildeEqual => constants::builtins::IS_NOT_REGEX_MATCH,
            TokenKind::LeftShift => constants::builtins::SHIFT_LEFT,
            TokenKind::RightShift => constants::builtins::SHIFT_RIGHT,
            TokenKind::Convert => constants::builtins::CONVERT,
            _ => return None,
        })
    }

    /// Maps the token kinds with a dedicated VM fast path (`+ - * / % == != < <= > >=`) to their
    /// `BinaryOp`. `None` for every other binary operator (`??`, `..`, `~=`, `!~=`, `<<`, `>>`,
    /// `as`), which stay desugared into `Expr::Call` via `binary_op_function_name`.
    fn binary_op_kind(kind: &TokenKind) -> Option<BinaryOp> {
        match kind {
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Sub),
            TokenKind::Asterisk => Some(BinaryOp::Mul),
            TokenKind::Slash => Some(BinaryOp::Div),
            TokenKind::Percent => Some(BinaryOp::Mod),
            TokenKind::EqEq => Some(BinaryOp::Eq),
            TokenKind::NeEq => Some(BinaryOp::Ne),
            TokenKind::Lt => Some(BinaryOp::Lt),
            TokenKind::Lte => Some(BinaryOp::Le),
            TokenKind::Gt => Some(BinaryOp::Gt),
            TokenKind::Gte => Some(BinaryOp::Ge),
            _ => None,
        }
    }

    fn create_compound_assign(
        &mut self,
        lhs: &Shared<Node>,
        rhs: Shared<Node>,
        operator_token_id: TokenId,
        operator_token: &Token,
        op: BinaryOp,
    ) -> Result<Shared<Node>, SyntaxError> {
        let compound_rhs = Shared::new(Node {
            token_id: operator_token_id,
            expr: Expr::BinaryOp(op, Shared::clone(lhs), rhs),
        });
        self.create_assign(lhs, compound_rhs, operator_token_id, operator_token)
    }

    /// Creates an assignment node, handling both simple identifiers and indexed targets.
    ///
    /// - `x = rhs` → `Assign(x, rhs)`
    /// - `arr[idx] = rhs` (lhs is `Call("get", [arr, idx])`) → `IndexAssign(arr, idx, rhs)`
    /// - `arr[i][j] = rhs` → `arr[i] = set(arr[i], j, rhs)`, applied recursively
    fn create_assign(
        &mut self,
        lhs: &Shared<Node>,
        rhs: Shared<Node>,
        operator_token_id: TokenId,
        operator_token: &Token,
    ) -> Result<Shared<Node>, SyntaxError> {
        match &lhs.expr {
            Expr::Ident(ident) => Ok(Shared::new(Node {
                token_id: operator_token_id,
                expr: Expr::Assign(ident.clone(), rhs),
            })),
            Expr::Call(func_ident, args) if func_ident.name == *GET_IDENT && args.len() == 2 => match &args[0].expr {
                Expr::Ident(var_ident) => Ok(Shared::new(Node {
                    token_id: operator_token_id,
                    expr: Expr::IndexAssign(var_ident.clone(), Shared::clone(&args[1]), rhs),
                })),
                Expr::Call(..) => {
                    let set_call = Shared::new(Node {
                        token_id: operator_token_id,
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(
                                constants::builtins::SET,
                                Some(self.alloc_ident_token(operator_token)),
                            ),
                            smallvec![Shared::clone(&args[0]), Shared::clone(&args[1]), rhs],
                        ),
                    });
                    self.create_assign(&args[0], set_call, operator_token_id, operator_token)
                }
                _ => Err(SyntaxError::InvalidAssignmentTarget(
                    self.token_arena[args[0].token_id].clone(),
                )),
            },
            _ => Err(SyntaxError::InvalidAssignmentTarget(
                self.token_arena[lhs.token_id].clone(),
            )),
        }
    }

    fn parse_binary_op(parser: &mut Parser, min_prec: u8, mut lhs: Shared<Node>) -> Result<Shared<Node>, SyntaxError> {
        while let Some(peeked_token_rc) = parser.tokens.peek() {
            let kind = &peeked_token_rc.kind;
            let Some(prec) = kind.binary_op_precedence() else {
                break;
            };

            if prec < min_prec {
                break;
            }

            let operator_token = parser.next_or_eof()?;
            let operator_token_id = parser.alloc_token(operator_token);

            let rhs_token = match parser.tokens.next() {
                Some(t) if t.kind == TokenKind::Eof => {
                    return Err(SyntaxError::UnexpectedEOFAfterToken(operator_token.clone()));
                }
                Some(t) => t,
                None => return Err(SyntaxError::UnexpectedEOFAfterToken(operator_token.clone())),
            };
            let mut rhs = parser.parse_primary_expr(rhs_token)?;

            loop {
                let next_prec = parser
                    .tokens
                    .peek()
                    .and_then(|next_token| next_token.kind.binary_op_precedence())
                    .unwrap_or(0);
                if next_prec > prec {
                    rhs = Self::parse_binary_op(parser, next_prec, rhs)?;
                } else {
                    break;
                }
            }

            lhs = match kind {
                TokenKind::Equal => parser.create_assign(&lhs, rhs, operator_token_id, operator_token)?,
                TokenKind::And => {
                    if matches!(&lhs.expr, Expr::And(_)) {
                        let mut lhs = lhs;
                        let node = Shared::make_mut(&mut lhs);
                        node.token_id = operator_token_id;
                        let Expr::And(operands) = &mut node.expr else {
                            unreachable!("checked the expression before making it mutable");
                        };
                        operands.push(rhs);
                        lhs
                    } else {
                        Shared::new(Node {
                            token_id: operator_token_id,
                            expr: Expr::And(vec![lhs, rhs]),
                        })
                    }
                }
                TokenKind::Or => {
                    if matches!(&lhs.expr, Expr::Or(_)) {
                        let mut lhs = lhs;
                        let node = Shared::make_mut(&mut lhs);
                        node.token_id = operator_token_id;
                        let Expr::Or(operands) = &mut node.expr else {
                            unreachable!("checked the expression before making it mutable");
                        };
                        operands.push(rhs);
                        lhs
                    } else {
                        Shared::new(Node {
                            token_id: operator_token_id,
                            expr: Expr::Or(vec![lhs, rhs]),
                        })
                    }
                }
                TokenKind::PlusEqual => {
                    parser.create_compound_assign(&lhs, rhs, operator_token_id, operator_token, BinaryOp::Add)?
                }
                TokenKind::MinusEqual => {
                    parser.create_compound_assign(&lhs, rhs, operator_token_id, operator_token, BinaryOp::Sub)?
                }
                TokenKind::StarEqual => {
                    parser.create_compound_assign(&lhs, rhs, operator_token_id, operator_token, BinaryOp::Mul)?
                }
                TokenKind::SlashEqual => {
                    parser.create_compound_assign(&lhs, rhs, operator_token_id, operator_token, BinaryOp::Div)?
                }
                TokenKind::PercentEqual => {
                    parser.create_compound_assign(&lhs, rhs, operator_token_id, operator_token, BinaryOp::Mod)?
                }
                TokenKind::DoubleSlashEqual => {
                    let floor_div_rhs = Shared::new(Node {
                        token_id: operator_token_id,
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(
                                constants::builtins::FLOOR,
                                Some(parser.alloc_ident_token(operator_token)),
                            ),
                            smallvec![Shared::new(Node {
                                token_id: operator_token_id,
                                expr: Expr::BinaryOp(BinaryOp::Div, Shared::clone(&lhs), rhs),
                            })],
                        ),
                    });
                    parser.create_assign(&lhs, floor_div_rhs, operator_token_id, operator_token)?
                }
                _ => {
                    let expr = match Self::binary_op_kind(kind) {
                        Some(op) => Expr::BinaryOp(op, lhs, rhs),
                        None => {
                            let name = Self::binary_op_function_name(kind)
                                .ok_or_else(|| SyntaxError::UnexpectedToken(operator_token.clone()))?;
                            Expr::Call(
                                IdentWithToken::new_with_token(name, Some(parser.alloc_ident_token(operator_token))),
                                smallvec![lhs, rhs],
                            )
                        }
                    };
                    Shared::new(Node {
                        token_id: operator_token_id,
                        expr,
                    })
                }
            };
        }

        Ok(lhs)
    }

    fn parse_equality_expr(&mut self, initial_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let lhs = self.parse_primary_expr(initial_token)?;
        let lhs = Self::parse_binary_op(self, 0, lhs)?;

        if let Some(token) = self.tokens.peek()
            && matches!(token.kind, TokenKind::As)
        {
            return self.parse_as_binding(lhs);
        }

        Ok(lhs)
    }

    fn parse_as_binding(&mut self, expr: Shared<Node>) -> Result<Shared<Node>, SyntaxError> {
        let as_token = self.next_or_eof()?;
        let as_token_id = self.alloc_token(as_token);

        let name_token = match self.tokens.next() {
            Some(token) => token,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        match &name_token.kind {
            TokenKind::Ident(name) => {
                let ident = IdentWithToken::new_with_token(name, Some(self.alloc_ident_token(name_token)));
                Ok(Shared::new(Node {
                    token_id: as_token_id,
                    expr: Expr::As(ident, expr),
                }))
            }
            _ => Err(SyntaxError::UnexpectedToken(name_token.clone())),
        }
    }

    /// Every recursive path in the grammar goes through here, so the depth limit is enforced once.
    fn parse_primary_expr(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        if self.depth >= MAX_PARSE_DEPTH {
            return Err(SyntaxError::TooDeeplyNested(token.clone(), MAX_PARSE_DEPTH));
        }
        self.depth += 1;
        let result = self.parse_primary_expr_inner(token);
        self.depth -= 1;
        result
    }

    fn parse_primary_expr_inner(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Selector(_) | TokenKind::DoubleDot => self.parse_selector(token),
            TokenKind::Let => self.parse_let(token),
            TokenKind::Var => self.parse_var(token),
            TokenKind::Def => self.parse_def(token),
            TokenKind::Do => self.parse_block(token),
            TokenKind::Fn if self.is_next_token(|kind| matches!(kind, TokenKind::Colon)) => {
                self.parse_implicit_fn(token)
            }
            TokenKind::Fn | TokenKind::Arrow => self.parse_fn(token),
            TokenKind::While => self.parse_while(token),
            TokenKind::Loop => self.parse_loop(token),
            TokenKind::Until => self.parse_until(token),
            TokenKind::Unless => self.parse_unless(token),
            TokenKind::Foreach => self.parse_foreach(token),
            TokenKind::Module => self.parse_module(token),
            TokenKind::Try => self.parse_try(token),
            TokenKind::If => self.parse_if(token),
            TokenKind::Match => self.parse_match(token),
            TokenKind::InterpolatedString(_) => {
                let node = self.parse_interpolated_string(token)?;
                self.parse_postfix_ops(node, token)
            }
            TokenKind::Include => self.parse_include(token),
            TokenKind::Import => self.parse_import(token),
            TokenKind::Self_ => self.parse_self(token),
            TokenKind::Break => self.parse_break(token),
            TokenKind::Continue => self.parse_continue(token),
            TokenKind::Yield => self.parse_yield(token),
            TokenKind::Ident(name) => self.parse_ident(name, token),
            TokenKind::BoolLiteral(_) => self.parse_literal(token),
            TokenKind::StringLiteral(_) => self.parse_literal(token),
            TokenKind::BytesLiteral(_) => self.parse_literal(token),
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
            _ => Err(SyntaxError::UnexpectedToken(token.clone())),
        }
    }

    #[inline(never)]
    fn parse_module(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Module => match self.tokens.peek() {
                Some(_) => {
                    let ident_token = self
                        .tokens
                        .next()
                        .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?;

                    self.consume_colon_or_do();

                    let program = self.parse_program(ProgramKind::Block)?;

                    // Only allow 'let', 'def', or 'module' at the top-level of a module block
                    for node in &program {
                        match &node.expr {
                            Expr::Let(_, _) | Expr::Def(_, _, _) | Expr::Module(_, _) | Expr::Import(_, _) => {}
                            _ => {
                                return Err(SyntaxError::UnexpectedToken(self.token_arena[node.token_id].clone()));
                            }
                        }
                    }

                    Ok(Shared::new(Node {
                        token_id: self.alloc_token(token),
                        expr: Expr::Module(
                            IdentWithToken::new_with_token(
                                match &ident_token.kind {
                                    TokenKind::Ident(name) => name,
                                    _ => {
                                        return Err(SyntaxError::UnexpectedToken(ident_token.clone()));
                                    }
                                },
                                Some(self.alloc_ident_token(ident_token)),
                            ),
                            program,
                        ),
                    }))
                }
                None => Err(SyntaxError::UnexpectedToken(token.clone())),
            },
            _ => Err(SyntaxError::UnexpectedToken(token.clone())),
        }
    }

    #[inline(never)]
    fn parse_symbol(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Colon => {
                let next_token = match self.tokens.next() {
                    Some(t) => t,
                    None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                };
                match &next_token.kind {
                    TokenKind::Ident(name) => Ok(Shared::new(Node {
                        token_id: self.alloc_token(token),
                        expr: Expr::Literal(Literal::Symbol(Ident::new(name))),
                    })),
                    TokenKind::StringLiteral(s) => Ok(Shared::new(Node {
                        token_id: self.alloc_token(token),
                        expr: Expr::Literal(Literal::Symbol(Ident::new(s))),
                    })),
                    _ => Err(SyntaxError::UnexpectedToken(next_token.clone())),
                }
            }
            _ => Err(SyntaxError::UnexpectedToken(token.clone())),
        }
    }

    #[inline(never)]
    fn parse_paren(&mut self, lparen_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let opening = lparen_token.clone();
        let token_id = self.alloc_token(lparen_token);
        let expr_token = match self.tokens.next() {
            Some(t) => t,
            None => {
                return Err(SyntaxError::ExpectedClosingParen(
                    Token {
                        range: opening.range,
                        kind: TokenKind::Eof,
                        module_id: self.module_id,
                    },
                    Some(Box::new(opening)),
                ));
            }
        };

        let expr_node = self.parse_expr(expr_token)?;

        match self.tokens.next() {
            Some(t) if t.kind == TokenKind::RParen => {}
            Some(t) if t.kind == TokenKind::Eof => {
                return Err(SyntaxError::ExpectedClosingParen(t.clone(), Some(Box::new(opening))));
            }
            Some(t) => return Err(SyntaxError::UnexpectedToken(t.clone())),
            None => {
                return Err(SyntaxError::ExpectedClosingParen(
                    Token {
                        range: opening.range,
                        kind: TokenKind::Eof,
                        module_id: self.module_id,
                    },
                    Some(Box::new(opening)),
                ));
            }
        }

        let paren_node = Shared::new(Node {
            token_id,
            expr: Expr::Paren(expr_node),
        });

        // Handle postfix operations: (expr)(args), (expr)[N], (expr)(args)[N], etc.
        self.parse_postfix_ops(paren_node, lparen_token)
    }

    /// Tokens that can start the operand of a prefix `!` or `-`.
    pub(crate) fn is_prefix_operand_start(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::BoolLiteral(_)
                | TokenKind::None
                | TokenKind::StringLiteral(_)
                | TokenKind::BytesLiteral(_)
                | TokenKind::NumberLiteral(_)
                | TokenKind::If
                | TokenKind::Foreach
                | TokenKind::LBrace
                | TokenKind::LBracket
                | TokenKind::LParen
                | TokenKind::While
                | TokenKind::Loop
                | TokenKind::Match
                | TokenKind::Self_
                | TokenKind::Selector(_)
                | TokenKind::Env(_)
                | TokenKind::Not
                | TokenKind::Ident(_)
        )
    }

    #[inline(never)]
    fn parse_not(&mut self, not_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(not_token);

        let expr_token = match self.tokens.next() {
            Some(t) if t.kind == TokenKind::Eof => {
                return Err(SyntaxError::UnexpectedEOFAfterToken(not_token.clone()));
            }
            Some(t) => t,
            None => return Err(SyntaxError::UnexpectedEOFAfterToken(not_token.clone())),
        };

        if !Self::is_prefix_operand_start(&expr_token.kind) {
            return Err(SyntaxError::UnexpectedToken(expr_token.clone()));
        }

        let expr_node = self.parse_primary_expr(expr_token)?;

        Ok(Shared::new(Node {
            token_id,
            expr: Expr::UnaryOp(UnaryOp::Not, expr_node),
        }))
    }

    #[inline(never)]
    fn parse_negate(&mut self, minus_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(minus_token);

        let expr_token = match self.tokens.next() {
            Some(t) if t.kind == TokenKind::Eof => {
                return Err(SyntaxError::UnexpectedEOFAfterToken(minus_token.clone()));
            }
            Some(t) => t,
            None => return Err(SyntaxError::UnexpectedEOFAfterToken(minus_token.clone())),
        };

        if !Self::is_prefix_operand_start(&expr_token.kind) {
            return Err(SyntaxError::UnexpectedToken(expr_token.clone()));
        }

        let expr_node = self.parse_primary_expr(expr_token)?;

        Ok(Shared::new(Node {
            token_id,
            expr: Expr::UnaryOp(UnaryOp::Neg, expr_node),
        }))
    }

    /// Consumes the comma or closing `}` after a dict entry. Returns `true` once closed.
    fn parse_dict_separator(&mut self, opening: &Token) -> Result<bool, SyntaxError> {
        match self.tokens.peek() {
            Some(token) if token.kind == TokenKind::Comma => {
                self.tokens.next(); // Consume Comma
                if let Some(next_token) = self.tokens.peek()
                    && next_token.kind == TokenKind::RBrace
                {
                    self.tokens.next(); // Consume RBrace
                    return Ok(true);
                }
                Ok(false)
            }
            Some(token) if token.kind == TokenKind::RBrace => {
                self.tokens.next(); // Consume RBrace
                Ok(true)
            }
            Some(token) => Err(SyntaxError::ExpectedClosingBrace(
                (**token).clone(),
                Some(Box::new(opening.clone())),
            )),
            None => Err(SyntaxError::ExpectedClosingBrace(
                Token {
                    range: opening.range,
                    kind: TokenKind::Eof,
                    module_id: self.module_id,
                },
                Some(Box::new(opening.clone())),
            )),
        }
    }

    #[inline(never)]
    fn parse_dict(&mut self, lbrace_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let opening = lbrace_token.clone();
        let token_id = self.alloc_token(lbrace_token);
        let mut pairs = SmallVec::new();

        let eof_closing_err = |opening: &Token, module_id: ModuleId| {
            SyntaxError::ExpectedClosingBrace(
                Token {
                    range: opening.range,
                    kind: TokenKind::Eof,
                    module_id,
                },
                Some(Box::new(opening.clone())),
            )
        };

        loop {
            match self.tokens.peek() {
                Some(token) if token.kind == TokenKind::RBrace => {
                    self.tokens.next();
                    break;
                }
                Some(token) if token.kind == TokenKind::Eof => {
                    return Err(SyntaxError::ExpectedClosingBrace(
                        (**token).clone(),
                        Some(Box::new(opening.clone())),
                    ));
                }
                None => {
                    return Err(eof_closing_err(&opening, self.module_id));
                }
                _ => {}
            }

            // Parse key
            let key_token = match self.tokens.next() {
                Some(t) => t,
                None => return Err(eof_closing_err(&opening, self.module_id)),
            };

            if key_token.kind == TokenKind::DotDotDot {
                pairs.push(self.parse_spread_element(key_token)?);
                if self.parse_dict_separator(&opening)? {
                    break;
                }
                continue;
            }

            let key_node = match &key_token.kind {
                TokenKind::Ident(name) => Shared::new(Node {
                    token_id: self.alloc_token(key_token),
                    expr: Expr::Literal(Literal::Symbol(Ident::new(name))),
                }),
                TokenKind::StringLiteral(s) => Shared::new(Node {
                    token_id: self.alloc_token(key_token),
                    expr: Expr::Literal(Literal::String(s.clone())),
                }),
                _ => {
                    return Err(SyntaxError::UnexpectedToken(key_token.clone()));
                }
            };

            // Expect Colon
            match self.tokens.next() {
                Some(token) if token.kind == TokenKind::Colon => {}
                Some(token) => return Err(SyntaxError::UnexpectedToken(token.clone())),
                None => return Err(eof_closing_err(&opening, self.module_id)),
            }

            // Parse value
            let value_token = match self.tokens.next() {
                Some(t) => t,
                None => return Err(eof_closing_err(&opening, self.module_id)),
            };
            let value_node = self.parse_expr(value_token)?;

            pairs.push(Shared::new(Node {
                token_id,
                expr: Expr::Array(smallvec![key_node, value_node]),
            }));

            if self.parse_dict_separator(&opening)? {
                break;
            }
        }

        let dict_node = Shared::new(Node {
            token_id,
            expr: Expr::Dict(pairs),
        });

        self.parse_postfix_ops(dict_node, lbrace_token)
    }

    #[inline(never)]
    fn parse_env(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Env(s) => Ok(Shared::new(Node {
                token_id: self.alloc_token(token),
                expr: io_context::current()
                    .env_var(s)
                    .map_err(|e| match e {
                        crate::io::IoError::PermissionDenied(_) => {
                            SyntaxError::EnvNotAllowed(token.clone(), SmolStr::new(s))
                        }
                        _ => SyntaxError::EnvNotFound(token.clone(), SmolStr::new(s)),
                    })
                    .map(|s| Expr::Literal(Literal::String(s.to_owned())))?,
            })),
            TokenKind::Eof => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(SyntaxError::UnexpectedToken(token.clone())),
        }
    }

    fn parse_attribute_access(&mut self, base_node: Shared<Node>) -> Result<Shared<Node>, SyntaxError> {
        let selector_token = match self.tokens.peek() {
            Some(t) => *t,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        if let TokenKind::Selector(selector) = &selector_token.kind
            && selector.len() > 1
        {
            if !Selector::try_from(selector_token)
                .map_err(SyntaxError::UnknownSelector)?
                .is_attribute_selector()
            {
                return Err(SyntaxError::UnexpectedToken((*selector_token).clone()));
            }

            let attribute_name = &selector[1..]; // Skip the leading '.'
            let attr_literal_token_id = self.alloc_token(selector_token);
            let attr_literal = Shared::new(Node {
                token_id: attr_literal_token_id,
                expr: Expr::Literal(Literal::String(attribute_name.to_string())),
            });

            self.tokens.next(); // Consume selector token

            if self.is_next_token(|kind| matches!(kind, TokenKind::PipeEqual)) {
                self.tokens.next(); // consume '|='
                return self.parse_set_attr_call_with_selector(base_node, attr_literal);
            }

            Ok(Shared::new(Node {
                token_id: self.alloc_token(selector_token),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::ATTR, Some(attr_literal_token_id)),
                    smallvec![base_node, attr_literal],
                ),
            }))
        } else {
            Ok(base_node)
        }
    }

    #[inline(never)]
    fn parse_self(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(token);
        let self_node = Shared::new(Node {
            token_id,
            expr: Expr::Self_,
        });
        let node = self.parse_attribute_access(self_node)?;

        match self.tokens.peek().map(|t| &t.kind) {
            Some(TokenKind::LBracket) => self.parse_bracket_access(node, token),
            _ => Ok(node),
        }
    }

    #[inline(never)]
    fn parse_break(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(token);

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
            expr: Expr::Break(value),
        }))
    }

    #[inline(never)]
    fn parse_yield(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(token);

        // Check for colon and expression (yield: expr)
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
            expr: Expr::Yield(value),
        }))
    }

    #[inline(never)]
    fn parse_continue(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        Ok(Shared::new(Node {
            token_id: self.alloc_token(token),
            expr: Expr::Continue,
        }))
    }

    /// Parses a `...expr` spread element, wrapping it in a `SPREAD` marker call that
    /// `eval_builtin` expands in place when building the enclosing array/dict.
    fn parse_spread_element(&mut self, dots_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(dots_token);
        let next_token = self
            .tokens
            .next()
            .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?;
        let inner = self.parse_expr(next_token)?;

        Ok(Shared::new(Node {
            token_id,
            expr: Expr::Call(
                IdentWithToken::new_with_token(constants::builtins::SPREAD, Some(self.alloc_ident_token(dots_token))),
                smallvec![inner],
            ),
        }))
    }

    /// Requires `,` or `]` after an array element so `[1 :b]` is not read as two elements.
    fn expect_array_element_end(&mut self, opening: &Token) -> Result<(), SyntaxError> {
        match self.tokens.peek() {
            Some(token) if !matches!(token.kind, TokenKind::Comma | TokenKind::RBracket | TokenKind::Eof) => Err(
                SyntaxError::ExpectedClosingBracket((**token).clone(), Some(Box::new(opening.clone()))),
            ),
            _ => Ok(()),
        }
    }

    #[inline(never)]
    fn parse_array(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let opening = token.clone();
        let token_id = self.alloc_token(token);
        let mut elements: SmallVec<[Shared<Node>; 4]> = SmallVec::new();
        let mut closed = false;
        let mut after_element = false;

        while let Some(elem_token) = self.tokens.next() {
            match &elem_token.kind {
                TokenKind::RBracket => {
                    closed = true;
                    break;
                }
                TokenKind::Eof => {
                    return Err(SyntaxError::ExpectedClosingBracket(
                        elem_token.clone(),
                        Some(Box::new(opening)),
                    ));
                }
                TokenKind::Comma if after_element => after_element = false,
                TokenKind::Comma => return Err(SyntaxError::UnexpectedToken(elem_token.clone())),
                kind => {
                    let element = if matches!(kind, TokenKind::DotDotDot) {
                        self.parse_spread_element(elem_token)?
                    } else {
                        self.parse_expr(elem_token)?
                    };
                    elements.push(element);
                    self.expect_array_element_end(&opening)?;
                    after_element = true;
                }
            }
        }

        if !closed {
            return Err(SyntaxError::ExpectedClosingBracket(
                Token {
                    range: opening.range,
                    kind: TokenKind::Eof,
                    module_id: self.module_id,
                },
                Some(Box::new(opening)),
            ));
        }

        let array_node = Shared::new(Node {
            token_id,
            expr: Expr::Array(elements),
        });

        // Handle postfix bracket access: [1,2,3][0], [1,2,3][0:2], etc.
        self.parse_postfix_ops(array_node, token)
    }

    fn parse_all_nodes(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        Ok(Shared::new(Node {
            token_id: self.alloc_token(token),
            expr: Expr::Nodes,
        }))
    }

    fn next_or_eof(&mut self) -> Result<&'a Token, SyntaxError> {
        self.tokens
            .next()
            .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))
    }

    fn is_next_token(&mut self, expected: impl Fn(&TokenKind) -> bool) -> bool {
        self.tokens.peek().as_ref().map(|t| &t.kind).is_some_and(expected)
    }

    /// Tokens that may follow a primary expression anywhere, including call arguments and array
    /// elements where `expect_statement_end` does not apply.
    fn is_next_token_allowed(token_kind: Option<&TokenKind>) -> bool {
        matches!(
            token_kind,
            Some(TokenKind::And)
                | Some(TokenKind::As)
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
                | Some(TokenKind::DoubleDot)
                | Some(TokenKind::RBrace)
                | Some(TokenKind::RBracket)
                | Some(TokenKind::RParen)
                | Some(TokenKind::SemiColon)
                | Some(TokenKind::End)
                | Some(TokenKind::Slash)
                | Some(TokenKind::Coalesce)
                | Some(TokenKind::PlusEqual)
                | Some(TokenKind::MinusEqual)
                | Some(TokenKind::SlashEqual)
                | Some(TokenKind::StarEqual)
                | Some(TokenKind::DoubleSlashEqual)
                | Some(TokenKind::PercentEqual)
                | Some(TokenKind::TildeEqual)
                | Some(TokenKind::NotTildeEqual)
                | Some(TokenKind::LeftShift)
                | Some(TokenKind::RightShift)
                | Some(TokenKind::Convert)
                | None
        )
    }

    #[inline(never)]
    fn parse_literal(&mut self, literal_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let literal_node = match &literal_token.kind {
            TokenKind::BoolLiteral(b) => Ok(Shared::new(Node {
                token_id: self.alloc_token(literal_token),
                expr: Expr::Literal(Literal::Bool(*b)),
            })),
            TokenKind::StringLiteral(s) => Ok(Shared::new(Node {
                token_id: self.alloc_token(literal_token),
                expr: Expr::Literal(Literal::String(s.to_owned())),
            })),
            TokenKind::BytesLiteral(b) => Ok(Shared::new(Node {
                token_id: self.alloc_token(literal_token),
                expr: Expr::Literal(Literal::Bytes(b.clone())),
            })),
            TokenKind::NumberLiteral(n) => Ok(Shared::new(Node {
                token_id: self.alloc_token(literal_token),
                expr: Expr::Literal(Literal::Number(*n)),
            })),
            TokenKind::None => Ok(Shared::new(Node {
                token_id: self.alloc_token(literal_token),
                expr: Expr::Literal(Literal::None),
            })),
            TokenKind::Eof => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(SyntaxError::UnexpectedToken(literal_token.clone())),
        }?;

        let token = self.tokens.peek();

        if matches!(token.as_ref().map(|t| &t.kind), Some(TokenKind::LBracket)) {
            return self.parse_postfix_ops(literal_node, literal_token);
        }

        if Self::is_next_token_allowed(token.as_ref().map(|t| &t.kind)) {
            Ok(literal_node)
        } else {
            Err(
                token.map_or(SyntaxError::UnexpectedEOFDetected(self.module_id), |token| {
                    SyntaxError::UnexpectedToken((*token).clone())
                }),
            )
        }
    }

    #[inline(never)]
    fn parse_ident(&mut self, ident: &str, ident_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        match self.tokens.peek().map(|t| &t.kind) {
            Some(TokenKind::Selector(selector)) if selector.len() > 1 => {
                let token_id = self.alloc_token(ident_token);
                let base_node = Shared::new(Node {
                    token_id,
                    expr: Expr::Ident(IdentWithToken::new_with_token(
                        ident,
                        Some(self.alloc_ident_token(ident_token)),
                    )),
                });

                self.parse_attribute_access(base_node)
            }
            Some(TokenKind::DoubleColon) => {
                // Parse qualified access: module::function(), module::ident, or module::module2::method
                // Build the module path by collecting all identifiers separated by '::'
                let mut module_path = vec![IdentWithToken::new_with_token(
                    ident,
                    Some(self.alloc_ident_token(ident_token)),
                )];

                // Collect all module path segments
                while matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::DoubleColon)) {
                    self.tokens.next(); // consume '::'

                    let next_token = self
                        .tokens
                        .next()
                        .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?;

                    let next_ident = match &next_token.kind {
                        TokenKind::Ident(name) => name.clone(),
                        _ => return Err(SyntaxError::UnexpectedToken(next_token.clone())),
                    };

                    // Check if this is the last segment (followed by '(' or not '::')
                    match self.tokens.peek().map(|t| &t.kind) {
                        Some(TokenKind::DoubleColon) => {
                            // More segments to come, add to module path
                            module_path.push(IdentWithToken::new_with_token(
                                &next_ident,
                                Some(self.alloc_ident_token(next_token)),
                            ));
                        }
                        Some(TokenKind::LParen) => {
                            // This is a function call: module::...::function(args)
                            let args = self.parse_args()?;
                            let access_target = AccessTarget::Call(
                                IdentWithToken::new_with_token(&next_ident, Some(self.alloc_ident_token(next_token))),
                                Box::new(args),
                            );

                            let token_id = self.alloc_token(ident_token);
                            let qualified_node = Shared::new(Node {
                                token_id,
                                expr: Expr::QualifiedAccess(module_path, access_target),
                            });
                            // Check for bracket access after qualified function call (e.g., module::func()[:1])
                            if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                                return self.parse_bracket_access(qualified_node, ident_token);
                            }
                            return Ok(qualified_node);
                        }
                        _ => {
                            // This is an identifier: module::...::ident
                            let access_target = AccessTarget::Ident(IdentWithToken::new_with_token(
                                &next_ident,
                                Some(self.alloc_ident_token(next_token)),
                            ));

                            let token_id = self.alloc_token(ident_token);
                            let qualified_node = Shared::new(Node {
                                token_id,
                                expr: Expr::QualifiedAccess(module_path, access_target),
                            });
                            // Check for bracket access after a qualified identifier (e.g., module::value[0])
                            if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                                return self.parse_bracket_access(qualified_node, ident_token);
                            }
                            return Ok(qualified_node);
                        }
                    }
                }

                // This should not be reached, but handle it gracefully
                Err(SyntaxError::UnexpectedToken(ident_token.clone()))
            }
            Some(TokenKind::LParen) => {
                let mut args = self.parse_args()?;
                let token_id = self.alloc_token(ident_token);

                // Check for a call with a trailing do-block argument (e.g., foo(args) do ...)
                if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Do)) {
                    let do_token = self
                        .tokens
                        .next()
                        .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?; // consume 'do'
                    let block = self.parse_block(do_token)?;
                    args.push(block);

                    return Ok(Shared::new(Node {
                        token_id: self.alloc_token(ident_token),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(ident, Some(self.alloc_ident_token(ident_token))),
                            args,
                        ),
                    }));
                }

                let call_node = Shared::new(Node {
                    token_id,
                    expr: Expr::Call(
                        IdentWithToken::new_with_token(ident, Some(self.alloc_ident_token(ident_token))),
                        args,
                    ),
                });

                if self.is_next_token(|token_kind| matches!(token_kind, TokenKind::Question)) {
                    let question_token = self
                        .tokens
                        .next()
                        .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?;
                    let question_token_id = self.alloc_token(question_token);

                    return Ok(Shared::new(Node {
                        token_id: question_token_id,
                        expr: Expr::Try(
                            call_node,
                            None,
                            Shared::new(Node {
                                token_id,
                                expr: Expr::Literal(Literal::None),
                            }),
                        ),
                    }));
                }

                // Check for bracket access after function call (e.g., foo()[0])
                if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                    let result = self.parse_bracket_access(call_node, ident_token)?;
                    self.parse_postfix_ops(result, ident_token)
                } else if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Selector(_))) {
                    self.parse_attribute_access(call_node)
                } else if Self::is_next_token_allowed(self.tokens.peek().map(|t| &t.kind)) {
                    Ok(call_node)
                } else {
                    match self.tokens.peek() {
                        Some(token) => Err(SyntaxError::UnexpectedToken((**token).clone())),
                        None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                    }
                }
            }
            Some(TokenKind::LBracket) => {
                let ident_node = Shared::new(Node {
                    token_id: self.alloc_token(ident_token),
                    expr: Expr::Ident(IdentWithToken::new_with_token(
                        ident,
                        Some(self.alloc_ident_token(ident_token)),
                    )),
                });

                let result = self.parse_bracket_access(ident_node, ident_token)?;
                self.parse_postfix_ops(result, ident_token)
            }
            token if Self::is_next_token_allowed(token) => Ok(Shared::new(Node {
                token_id: self.alloc_token(ident_token),
                expr: Expr::Ident(IdentWithToken::new_with_token(
                    ident,
                    Some(self.alloc_ident_token(ident_token)),
                )),
            })),
            _ => Err(SyntaxError::UnexpectedToken(ident_token.clone())),
        }
    }

    // Parses bracket access operations recursively to handle nested access like arr[0][1][2]
    /// Chained accesses (`a[0][1]...`) recurse here, so each link counts toward the depth limit.
    fn parse_bracket_access(
        &mut self,
        target_node: Shared<Node>,
        original_token: &Token,
    ) -> Result<Shared<Node>, SyntaxError> {
        if self.depth >= MAX_PARSE_DEPTH {
            return Err(SyntaxError::TooDeeplyNested(original_token.clone(), MAX_PARSE_DEPTH));
        }
        self.depth += 1;
        let result = self.parse_bracket_access_inner(target_node, original_token);
        self.depth -= 1;
        result
    }

    fn parse_bracket_access_inner(
        &mut self,
        target_node: Shared<Node>,
        original_token: &Token,
    ) -> Result<Shared<Node>, SyntaxError> {
        let lbracket = self.tokens.next().cloned(); // consume '['

        // Check for [:N] or [:]  style slice (empty start index).
        // [:ident] and [:string] are dict key accesses using a symbol, not slices.
        let is_slice_from_start = matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Colon))
            && !matches!(
                self.tokens.clone().nth(1).map(|t| &t.kind),
                Some(TokenKind::Ident(_)) | Some(TokenKind::StringLiteral(_))
            );

        if is_slice_from_start {
            let _ = self.tokens.next(); // consume ':'
            let start_node = Shared::new(Node {
                token_id: self.alloc_token(original_token),
                expr: Expr::Literal(Literal::Number(0.into())),
            });

            let result_node = match self.tokens.next() {
                Some(t) if t.kind == TokenKind::RBracket => {
                    // [:] = slice(arr, 0, len(arr))
                    Shared::new(Node {
                        token_id: self.alloc_token(original_token),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(
                                constants::builtins::SLICE,
                                Some(self.alloc_ident_token(original_token)),
                            ),
                            smallvec![
                                Shared::clone(&target_node),
                                start_node,
                                Shared::new(Node {
                                    token_id: self.alloc_token(original_token),
                                    expr: Expr::Call(
                                        IdentWithToken::new_with_token(constants::builtins::LEN, None,),
                                        smallvec![target_node],
                                    ),
                                })
                            ],
                        ),
                    })
                }
                Some(t) => {
                    let end_node = self.parse_expr(t)?;
                    match self.tokens.peek() {
                        Some(token) if matches!(token.kind, TokenKind::RBracket) => {
                            let _ = self.tokens.next();
                        }
                        Some(token) => {
                            return Err(SyntaxError::ExpectedClosingBracket(
                                (**token).clone(),
                                lbracket.clone().map(Box::new),
                            ));
                        }
                        None => {
                            return Err(SyntaxError::ExpectedClosingBracket(
                                Token {
                                    range: original_token.range,
                                    kind: TokenKind::Eof,
                                    module_id: self.module_id,
                                },
                                lbracket.clone().map(Box::new),
                            ));
                        }
                    }
                    Shared::new(Node {
                        token_id: self.alloc_token(original_token),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(
                                constants::builtins::SLICE,
                                Some(self.alloc_ident_token(original_token)),
                            ),
                            smallvec![target_node, start_node, end_node],
                        ),
                    })
                }
                None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
            };

            let final_result = if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                self.parse_bracket_access(result_node, original_token)?
            } else {
                result_node
            };
            return Ok(final_result);
        }

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
                    token_id: self.alloc_token(original_token),
                    expr: Expr::Call(
                        IdentWithToken::new_with_token(
                            constants::builtins::SLICE,
                            Some(self.alloc_ident_token(original_token)),
                        ),
                        smallvec![
                            Shared::clone(&target_node),
                            first_node,
                            Shared::new(Node {
                                token_id: self.alloc_token(original_token),
                                expr: Expr::Call(
                                    IdentWithToken::new_with_token(constants::builtins::LEN, None),
                                    smallvec![target_node],
                                ),
                            })
                        ],
                    ),
                }),
                Some(t) => {
                    let second_node = self.parse_expr(t)?;

                    // Expect closing bracket
                    match self.tokens.peek() {
                        Some(token) if matches!(token.kind, TokenKind::RBracket) => {
                            let _ = self.tokens.next(); // consume ']'
                        }
                        Some(token) => {
                            return Err(SyntaxError::ExpectedClosingBracket(
                                (**token).clone(),
                                lbracket.clone().map(Box::new),
                            ));
                        }
                        None => {
                            return Err(SyntaxError::ExpectedClosingBracket(
                                Token {
                                    range: original_token.range,
                                    kind: TokenKind::Eof,
                                    module_id: self.module_id,
                                },
                                lbracket.clone().map(Box::new),
                            ));
                        }
                    }

                    Shared::new(Node {
                        token_id: self.alloc_token(original_token),
                        expr: Expr::Call(
                            IdentWithToken::new_with_token(
                                constants::builtins::SLICE,
                                Some(self.alloc_ident_token(original_token)),
                            ),
                            smallvec![target_node, first_node, second_node],
                        ),
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
                    return Err(SyntaxError::ExpectedClosingBracket(
                        (**token).clone(),
                        lbracket.clone().map(Box::new),
                    ));
                }
                None => {
                    return Err(SyntaxError::ExpectedClosingBracket(
                        Token {
                            range: original_token.range,
                            kind: TokenKind::Eof,
                            module_id: self.module_id,
                        },
                        lbracket.map(Box::new),
                    ));
                }
            }

            Shared::new(Node {
                token_id: self.alloc_token(original_token),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(
                        constants::builtins::GET,
                        Some(self.alloc_ident_token(original_token)),
                    ),
                    smallvec![target_node, first_node],
                ),
            })
        };

        // Check for additional bracket access (nested indexing)
        let final_result = if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
            self.parse_bracket_access(result_node, original_token)?
        } else {
            result_node
        };

        // Check for function call after bracket access (e.g., arr[0]() or arr[0][1]())
        // and handle further postfix operations (e.g., arr[0]()[1])
        if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LParen)) {
            let args = self.parse_args()?;
            let call_dynamic = Shared::new(Node {
                token_id: self.alloc_token(original_token),
                expr: Expr::CallDynamic(final_result, args),
            });
            self.parse_postfix_ops(call_dynamic, original_token)
        } else {
            Ok(final_result)
        }
    }

    /// Parses postfix operations (dynamic calls and bracket access) after a primary expression.
    /// Handles chains like `(expr)(args)[N]`, `expr[N](args)[M]`, etc.
    fn parse_postfix_ops(
        &mut self,
        mut current: Shared<Node>,
        original_token: &Token,
    ) -> Result<Shared<Node>, SyntaxError> {
        loop {
            if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LParen)) {
                let args = self.parse_args()?;
                current = Shared::new(Node {
                    token_id: self.alloc_token(original_token),
                    expr: Expr::CallDynamic(current, args),
                });
            } else if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::LBracket)) {
                current = self.parse_bracket_access(current, original_token)?;
            } else if matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Selector(s)) if s.len() > 1) {
                current = self.parse_attribute_access(current)?;
            } else {
                break;
            }
        }
        Ok(current)
    }

    #[inline(never)]
    fn parse_def(&mut self, def_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let ident_token = self.tokens.next();
        let ident = match &ident_token {
            Some(token) => match &token.kind {
                TokenKind::Ident(ident) => Ok(ident),
                _ => Err(SyntaxError::UnexpectedToken((**token).clone())),
            },
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;
        let def_token_id = self.alloc_token(def_token);
        let params = if self.is_next_token(|token| matches!(token, TokenKind::Colon | TokenKind::Do)) {
            Params::new()
        } else {
            self.parse_params()?
        };

        self.consume_colon_or_do();

        let program = self.parse_program(ProgramKind::Block)?;

        Ok(Shared::new(Node {
            token_id: def_token_id,
            expr: Expr::Def(
                IdentWithToken::new_with_token(ident, ident_token.map(|token| self.alloc_ident_token(token))),
                params,
                program,
            ),
        }))
    }

    #[inline(never)]
    fn parse_block(&mut self, do_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let do_token_id = self.alloc_token(do_token);
        let program = self.parse_program(ProgramKind::Block)?;

        // The End token is already consumed by parse_program when it encounters it
        // No need to expect another End token here

        Ok(Shared::new(Node {
            token_id: do_token_id,
            expr: Expr::Block(program),
        }))
    }

    #[inline(never)]
    fn parse_fn(&mut self, fn_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let fn_token_id = self.alloc_token(fn_token);
        let params = self.parse_params()?;

        self.consume_colon_or_do();

        let program = self.parse_program(ProgramKind::Block)?;

        let fn_node = Shared::new(Node {
            token_id: fn_token_id,
            expr: Expr::Fn(params, program),
        });

        // Handle postfix operations: fn(...): ... end(args), fn(...): ... end(args)[N], etc.
        self.parse_postfix_ops(fn_node, fn_token)
    }

    /// `fn: body`, parsed as `fn(%arg): %arg | body`.
    #[inline(never)]
    fn parse_implicit_fn(&mut self, fn_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let fn_token_id = self.alloc_token(fn_token);
        self.consume_colon();

        let mut program = self.parse_program(ProgramKind::Block)?;
        program.insert(
            0,
            Shared::new(Node {
                token_id: fn_token_id,
                expr: Expr::Ident(IdentWithToken::new(constants::identifiers::IMPLICIT_FN_ARG)),
            }),
        );

        let fn_node = Shared::new(Node {
            token_id: fn_token_id,
            expr: Expr::Fn(
                vec![Param {
                    ident: IdentWithToken::new(constants::identifiers::IMPLICIT_FN_ARG),
                    default: None,
                    is_variadic: false,
                }],
                program,
            ),
        });

        self.parse_postfix_ops(fn_node, fn_token)
    }

    #[inline(never)]
    fn parse_while(&mut self, while_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(while_token);
        let args = self.parse_args()?;

        let [cond] = args.as_slice() else {
            return Err(SyntaxError::UnexpectedToken(while_token.clone()));
        };

        self.consume_colon_or_do();

        match self.tokens.peek() {
            Some(_) => {
                let body_program = self.parse_program(ProgramKind::Block)?;

                Ok(Shared::new(Node {
                    token_id,
                    expr: Expr::While(Shared::clone(cond), body_program),
                }))
            }
            None => Err(SyntaxError::UnexpectedToken(while_token.clone())),
        }
    }

    #[inline(never)]
    fn parse_loop(&mut self, loop_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(loop_token);

        self.consume_colon_or_do();

        match self.tokens.peek() {
            Some(_) => {
                let body_program = self.parse_program(ProgramKind::Block)?;

                Ok(Shared::new(Node {
                    token_id,
                    expr: Expr::Loop(body_program),
                }))
            }
            None => Err(SyntaxError::UnexpectedToken(loop_token.clone())),
        }
    }

    #[inline(never)]
    fn parse_until(&mut self, until_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(until_token);
        let args = self.parse_args()?;

        let [cond] = args.as_slice() else {
            return Err(SyntaxError::UnexpectedToken(until_token.clone()));
        };

        self.consume_colon_or_do();

        match self.tokens.peek() {
            Some(_) => {
                let body_program = self.parse_program(ProgramKind::Block)?;

                Ok(Shared::new(Node {
                    token_id,
                    expr: Expr::Until(Shared::clone(cond), body_program),
                }))
            }
            None => Err(SyntaxError::UnexpectedToken(until_token.clone())),
        }
    }

    #[inline(never)]
    fn parse_unless(&mut self, unless_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(unless_token);
        let args = self.parse_args()?;

        let [cond] = args.as_slice() else {
            return Err(SyntaxError::UnexpectedToken(self.token_arena[token_id].clone()));
        };

        self.consume_colon();

        let then_expr = self.parse_next_expr(token_id)?;

        let branches: Branches = vec![(Some(Shared::clone(cond)), then_expr)];

        Ok(Shared::new(Node {
            token_id: self.alloc_token(unless_token),
            expr: Expr::Unless(branches),
        }))
    }

    #[inline(never)]
    fn parse_try(&mut self, try_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(try_token);

        self.consume_colon_or_do();

        // Parse try expression
        let try_expr = match self.tokens.next() {
            Some(token) => self.parse_expr(token)?,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        if !self.is_next_token(|token_kind| matches!(token_kind, TokenKind::Catch)) {
            return Ok(Shared::new(Node {
                token_id,
                expr: Expr::Try(
                    try_expr,
                    None,
                    Shared::new(Node {
                        token_id,
                        expr: Expr::Literal(Literal::None),
                    }),
                ),
            }));
        }

        // Expect 'catch' keyword
        let catch_token_id = self.next_token(|token_kind| matches!(token_kind, TokenKind::Catch))?;

        // Optional error binder: `catch(e):` binds `e` to a dict describing the failure.
        let error_binder = if self.is_next_token(|token_kind| matches!(token_kind, TokenKind::LParen)) {
            let args = self.parse_args()?;
            match args.as_slice() {
                [arg] => match &arg.expr {
                    Expr::Ident(ident) => Some(ident.clone()),
                    _ => return Err(SyntaxError::UnexpectedToken(self.token_arena[arg.token_id].clone())),
                },
                _ => {
                    return Err(SyntaxError::UnexpectedToken(self.token_arena[catch_token_id].clone()));
                }
            }
        } else {
            None
        };

        self.consume_colon_or_do();

        // Parse catch expression
        let catch_expr = match self.tokens.next() {
            Some(token) => self.parse_expr(token)?,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        Ok(Shared::new(Node {
            token_id,
            expr: Expr::Try(try_expr, error_binder, catch_expr),
        }))
    }

    #[inline(never)]
    fn parse_foreach(&mut self, foreach_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let args = self.parse_args()?;

        let [first_arg, _] = args.as_slice() else {
            return Err(SyntaxError::UnexpectedToken(foreach_token.clone()));
        };

        let first_arg = &first_arg.expr;

        match first_arg {
            Expr::Ident(IdentWithToken {
                name: ident,
                token: ident_token,
            }) => {
                self.consume_colon_or_do();

                let each_values = Shared::clone(&args[1]);
                let body_program = self.parse_program(ProgramKind::Block)?;

                Ok(Shared::new(Node {
                    token_id: self.alloc_token(foreach_token),
                    expr: Expr::Foreach(
                        IdentWithToken {
                            name: *ident,
                            token: *ident_token,
                        },
                        Shared::clone(&each_values),
                        body_program,
                    ),
                }))
            }
            _ => Err(SyntaxError::UnexpectedToken(foreach_token.clone())),
        }
    }

    #[inline(never)]
    fn parse_if(&mut self, if_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(if_token);
        let args = self.parse_args()?;

        let [cond] = args.as_slice() else {
            return Err(SyntaxError::UnexpectedToken(self.token_arena[token_id].clone()));
        };

        self.consume_colon();

        let then_expr = self.parse_next_expr(token_id)?;

        let mut branches: Branches = vec![(Some(Shared::clone(cond)), then_expr)];

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
            token_id: self.alloc_token(if_token),
            expr: Expr::If(branches),
        }))
    }

    #[inline(never)]
    fn parse_match(&mut self, match_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(match_token);

        // Parse the value expression: match (value):
        let args = self.parse_args()?;
        let [value] = args.as_slice() else {
            return Err(SyntaxError::UnexpectedToken(self.token_arena[token_id].clone()));
        };
        let value = Shared::clone(value);

        self.consume_colon_or_do();

        // Parse match arms
        let mut arms: super::node::MatchArms = Vec::new();

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
                    let if_token = Shared::new((*token).clone());
                    self.tokens.next(); // consume 'if'
                    let guard_args = self.parse_args()?;
                    let [guard] = guard_args.as_slice() else {
                        return Err(SyntaxError::UnexpectedToken((*if_token).clone()));
                    };
                    Some(Shared::clone(guard))
                } else {
                    None
                }
            } else {
                None
            };

            self.next_token(|token_kind| matches!(token_kind, TokenKind::Colon))?;

            // Parse body expression
            let body = self.parse_next_expr(token_id)?;

            arms.push(MatchArm { pattern, guard, body });
        }

        match self.tokens.peek() {
            Some(token) if matches!(token.kind, TokenKind::End) => {
                self.tokens.next();
            }
            _ => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }

        Ok(Shared::new(Node {
            token_id,
            expr: Expr::Match(value, arms),
        }))
    }

    fn parse_pattern(&mut self) -> Result<Pattern, SyntaxError> {
        let first = self.parse_single_pattern()?;
        if !matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Or)) {
            return Ok(first);
        }
        let mut patterns = vec![first];
        while matches!(self.tokens.peek().map(|t| &t.kind), Some(TokenKind::Or)) {
            self.tokens.next(); // consume ||
            patterns.push(self.parse_single_pattern()?);
        }
        Ok(Pattern::Or(patterns))
    }

    /// All nested patterns recurse through here, so the depth limit is enforced once.
    fn parse_single_pattern(&mut self) -> Result<Pattern, SyntaxError> {
        let token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };
        if self.depth >= MAX_PARSE_DEPTH {
            return Err(SyntaxError::TooDeeplyNested(token.clone(), MAX_PARSE_DEPTH));
        }
        self.depth += 1;
        let result = self.parse_single_pattern_inner(token);
        self.depth -= 1;
        result
    }

    fn parse_single_pattern_inner(&mut self, token: &Token) -> Result<Pattern, SyntaxError> {
        match &token.kind {
            // Wildcard pattern: _
            TokenKind::Ident(name) if name == constants::identifiers::PATTERN_MATCH_WILDCARD => Ok(Pattern::Wildcard),
            // Type pattern: :string, :number, etc.
            TokenKind::Colon => {
                let type_token = match self.tokens.next() {
                    Some(t) => t,
                    None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                };
                match &type_token.kind {
                    TokenKind::Ident(type_name) => Ok(Pattern::Type(Ident::new(type_name))),
                    _ => Err(SyntaxError::UnexpectedToken(type_token.clone())),
                }
            }
            // Literal patterns
            TokenKind::StringLiteral(s) => Ok(Pattern::Literal(Literal::String(s.clone()))),
            TokenKind::BytesLiteral(b) => Ok(Pattern::Literal(Literal::Bytes(b.clone()))),
            TokenKind::NumberLiteral(n) => Ok(Pattern::Literal(Literal::Number(*n))),
            TokenKind::BoolLiteral(b) => Ok(Pattern::Literal(Literal::Bool(*b))),
            TokenKind::None => Ok(Pattern::Literal(Literal::None)),
            // Array pattern: [pattern, pattern, ...]
            TokenKind::LBracket => self.parse_array_pattern(),
            // Dict pattern: {key, key: pattern}
            TokenKind::LBrace => self.parse_dict_pattern(),
            // Identifier pattern (binding)
            TokenKind::Ident(name) => Ok(Pattern::Ident(IdentWithToken::new(name))),
            _ => Err(SyntaxError::UnexpectedToken(token.clone())),
        }
    }

    fn parse_array_pattern(&mut self) -> Result<super::node::Pattern, SyntaxError> {
        let mut patterns = Vec::new();
        let mut rest_binding: Option<IdentWithToken> = None;

        loop {
            // Check for closing bracket
            if let Some(token) = self.tokens.peek() {
                if matches!(token.kind, TokenKind::RBracket) {
                    self.tokens.next(); // consume ]
                    break;
                }

                // Check for rest pattern: ...rest
                if matches!(token.kind, TokenKind::DoubleDot) {
                    self.tokens.next();
                    if let Some(ident_token) = self.tokens.next() {
                        if let TokenKind::Ident(name) = &ident_token.kind {
                            rest_binding = Some(IdentWithToken::new(name));
                        } else {
                            return Err(SyntaxError::UnexpectedToken(ident_token.clone()));
                        }
                    }
                    // Expect closing bracket after rest
                    if let Some(token) = self.tokens.next()
                        && !matches!(token.kind, TokenKind::RBracket)
                    {
                        return Err(SyntaxError::UnexpectedToken(token.clone()));
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
                    return Err(SyntaxError::UnexpectedToken((**token).clone()));
                }
            }
        }

        match rest_binding {
            Some(rest_binding) => Ok(Pattern::ArrayRest(patterns, rest_binding)),
            None => Ok(Pattern::Array(patterns)),
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
                _ => return Err(SyntaxError::UnexpectedToken(key_token.clone())),
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
                    return Err(SyntaxError::UnexpectedToken((**token).clone()));
                }
            }
        }

        Ok(super::node::Pattern::Dict(fields))
    }

    #[inline(always)]
    fn parse_next_expr(&mut self, token_id: TokenId) -> Result<Shared<Node>, SyntaxError> {
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(SyntaxError::UnexpectedToken(self.token_arena[token_id].clone())),
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

            let [cond] = args.as_slice() else {
                return Err(SyntaxError::UnexpectedToken(self.token_arena[token_id].clone()));
            };

            self.consume_colon();

            let expr_token = match self.tokens.next() {
                Some(token) => Ok(token),
                None => Err(SyntaxError::UnexpectedToken(self.token_arena[token_id].clone())),
            }?;

            let then_expr = self.parse_expr(expr_token)?;

            nodes.push((Some(Shared::clone(cond)), then_expr));
        }

        Ok(nodes)
    }

    fn parse_let_or_var_pattern(&mut self) -> Result<Pattern, SyntaxError> {
        // Clone kind to release the peek borrow before calling next()
        let token_kind = match self.tokens.peek() {
            Some(t) => t.kind.clone(),
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };
        match token_kind {
            TokenKind::LBracket => {
                self.tokens.next(); // consume [
                self.parse_array_pattern()
            }
            TokenKind::LBrace => {
                self.tokens.next(); // consume {
                self.parse_dict_pattern()
            }
            TokenKind::Ident(ref name) => {
                let ident_token = self.next_or_eof()?;
                Ok(Pattern::Ident(IdentWithToken::new_with_token(
                    name,
                    Some(self.alloc_ident_token(ident_token)),
                )))
            }
            _ => {
                let bad_token = self.next_or_eof()?;
                Err(SyntaxError::UnexpectedToken(bad_token.clone()))
            }
        }
    }

    #[inline(never)]
    fn parse_let(&mut self, let_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let let_token_id = self.alloc_token(let_token);
        let pattern = self.parse_let_or_var_pattern()?;

        self.next_token(|token_kind| matches!(token_kind, TokenKind::Equal))?;
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        if matches!(expr_token.kind, TokenKind::Let | TokenKind::Var) {
            return Err(SyntaxError::UnexpectedToken(expr_token.clone()));
        }

        let ast = self.parse_expr(expr_token)?;

        if let Some(token) = self.tokens.peek()
            && !matches!(
                token.kind,
                TokenKind::Pipe | TokenKind::Eof | TokenKind::SemiColon | TokenKind::End
            )
        {
            return Err(SyntaxError::UnexpectedToken((**token).clone()));
        }

        Ok(Shared::new(Node {
            token_id: let_token_id,
            expr: Expr::Let(pattern, ast),
        }))
    }

    #[inline(never)]
    fn parse_var(&mut self, var_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let var_token_id = self.alloc_token(var_token);
        let pattern = self.parse_let_or_var_pattern()?;

        self.next_token(|token_kind| matches!(token_kind, TokenKind::Equal))?;
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }?;

        if matches!(expr_token.kind, TokenKind::Let | TokenKind::Var) {
            return Err(SyntaxError::UnexpectedToken(expr_token.clone()));
        }

        let ast = self.parse_expr(expr_token)?;

        if let Some(token) = self.tokens.peek()
            && !matches!(
                token.kind,
                TokenKind::Pipe | TokenKind::Eof | TokenKind::SemiColon | TokenKind::End
            )
        {
            return Err(SyntaxError::UnexpectedToken((**token).clone()));
        }

        Ok(Shared::new(Node {
            token_id: var_token_id,
            expr: Expr::Var(pattern, ast),
        }))
    }

    #[inline(always)]
    fn parse_include(&mut self, include_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::StringLiteral(module) => {
                    self.tokens.next();
                    Ok(Shared::new(Node {
                        token_id: self.alloc_token(include_token),
                        expr: Expr::Include(Literal::String(module.to_owned())),
                    }))
                }
                _ => Err(SyntaxError::InsufficientTokens((**token).clone())),
            },
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }
    }

    #[inline(always)]
    fn parse_import(&mut self, import_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let token_id = self.alloc_token(import_token);
        let token = self
            .tokens
            .next()
            .ok_or(SyntaxError::UnexpectedEOFDetected(self.module_id))?;

        match &token.kind {
            TokenKind::StringLiteral(module) => {
                let module_name = module.to_owned();
                let alias = if let Some(token) = self.tokens.peek()
                    && matches!(token.kind, TokenKind::As)
                {
                    self.tokens.next();
                    let name_token = match self.tokens.next() {
                        Some(token) => token,
                        None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                    };

                    match &name_token.kind {
                        TokenKind::Ident(name) => Some(IdentWithToken::new_with_token(
                            name,
                            Some(self.alloc_ident_token(name_token)),
                        )),
                        _ => return Err(SyntaxError::UnexpectedToken(name_token.clone())),
                    }
                } else {
                    None
                };

                Ok(Shared::new(Node {
                    token_id,
                    expr: Expr::Import(Literal::String(module_name), alias),
                }))
            }
            _ => Err(SyntaxError::InsufficientTokens(token.clone())),
        }
    }

    #[inline(never)]
    fn parse_interpolated_string(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        if let TokenKind::InterpolatedString(segments) = &token.kind {
            let mut parsed_segments = Vec::new();

            for segment in segments {
                match segment {
                    lexer::token::StringSegment::Text(text, _) => {
                        parsed_segments.push(super::node::StringSegment::Text(text.clone()));
                    }
                    lexer::token::StringSegment::Expr(expr_str, range) => {
                        let expr_src = expr_str.as_str();
                        let expr_str = expr_src.trim();

                        // Handle special cases first
                        if expr_str == constants::identifiers::SELF {
                            parsed_segments.push(super::node::StringSegment::Self_);
                        } else if let Some(stripped) = expr_str.strip_prefix("$") {
                            // Environment variable
                            parsed_segments.push(super::node::StringSegment::Env(SmolStr::from(stripped)));
                        } else {
                            // Lex from the original text so token ranges point into the source.
                            let expr_start = Position {
                                line: range.start.line,
                                column: range.start.column + 2,
                            };
                            let tokens = Lexer::new(crate::lexer::Options::default()).tokenize_from(
                                expr_src,
                                token.module_id,
                                expr_start,
                            )?;

                            let mut parser = Parser::new(tokens.iter(), self.token_arena, token.module_id);
                            parser.depth = self.depth;
                            let expr_node = parser.parse_expr_from_tokens()?;
                            // Nothing may follow the expression or pipeline.
                            if let Some(rest) = parser.tokens.peek()
                                && rest.kind != TokenKind::Eof
                            {
                                return Err(SyntaxError::UnexpectedToken((**rest).clone()));
                            }

                            parsed_segments.push(super::node::StringSegment::Expr(expr_node));
                        }
                    }
                }
            }

            Ok(Shared::new(Node {
                token_id: self.alloc_token(token),
                expr: Expr::InterpolatedString(parsed_segments),
            }))
        } else {
            Err(SyntaxError::UnexpectedToken(token.clone()))
        }
    }

    /// Parses the expression of a `${...}` segment, or a pipeline of them (`${a | b}`).
    fn parse_expr_from_tokens(&mut self) -> Result<Shared<Node>, SyntaxError> {
        match self.tokens.next() {
            Some(token) => self.parse_arg_expr(token),
            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        }
    }

    /// Parse function parameters (supports default values)
    fn parse_params(&mut self) -> Result<Params, SyntaxError> {
        let opening_paren = match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::LParen => {
                    let t = (**token).clone();
                    self.tokens.next();
                    Some(t)
                }
                _ => return Err(SyntaxError::UnexpectedToken((**token).clone())),
            },
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        let mut params: Params = Vec::new();
        let mut prev_token: Option<&TokenKind> = None;
        let mut seen_default = false;
        let mut seen_variadic = false;

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::RParen => match prev_token {
                    Some(TokenKind::Comma) => {
                        return Err(SyntaxError::UnexpectedToken(token.clone()));
                    }
                    _ => break,
                },
                TokenKind::Eof => match prev_token {
                    Some(TokenKind::RParen) => break,
                    Some(_) | None => {
                        return Err(SyntaxError::ExpectedClosingParen(
                            token.clone(),
                            opening_paren.clone().map(Box::new),
                        ));
                    }
                },
                TokenKind::Comma => match prev_token {
                    Some(_) => {
                        let token = match self.tokens.peek() {
                            Some(token) => Ok(Shared::new((*token).clone())),
                            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                        }?;
                        match &token.kind {
                            TokenKind::Comma => {
                                return Err(SyntaxError::UnexpectedToken((*token).clone()));
                            }
                            _ => continue,
                        }
                    }
                    None => return Err(SyntaxError::UnexpectedToken(token.clone())),
                },
                TokenKind::SemiColon => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                TokenKind::Asterisk => {
                    // Variadic parameter: *name
                    if seen_variadic {
                        return Err(SyntaxError::MultipleVariadicParameters(token.clone()));
                    }
                    let ident_token = match self.tokens.next() {
                        Some(t) => t,
                        None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                    };
                    match &ident_token.kind {
                        TokenKind::Ident(name) => {
                            let ident = IdentWithToken::new_with_token(name, Some(self.alloc_ident_token(ident_token)));
                            params.push(Param::variadic(ident));
                            seen_variadic = true;
                        }
                        _ => {
                            return Err(SyntaxError::UnexpectedToken(ident_token.clone()));
                        }
                    }
                }
                TokenKind::Ident(name) => {
                    // Non-variadic param after variadic is an error
                    if seen_variadic {
                        return Err(SyntaxError::VariadicParameterMustBeLast(token.clone()));
                    }

                    // Parse parameter name
                    let ident = IdentWithToken::new_with_token(name, Some(self.alloc_ident_token(token)));

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
                            return Err(SyntaxError::ParameterWithoutDefaultAfterDefault(token.clone()));
                        }
                        None
                    };

                    params.push(Param::with_default(ident, default));
                }
                _ => {
                    return Err(SyntaxError::UnexpectedToken(token.clone()));
                }
            }

            prev_token = Some(&token.kind);

            if let Some(token) = self.tokens.peek()
                && !matches!(token.kind, TokenKind::RParen | TokenKind::Comma)
            {
                return Err(SyntaxError::ExpectedClosingParen(
                    (**token).clone(),
                    opening_paren.clone().map(Box::new),
                ));
            }
        }

        Ok(params)
    }

    fn parse_args(&mut self) -> Result<Args, SyntaxError> {
        let opening_paren = match self.tokens.peek() {
            Some(token) => match &token.kind {
                TokenKind::LParen => {
                    let t = (**token).clone();
                    self.tokens.next();
                    Some(t)
                }
                _ => return Err(SyntaxError::UnexpectedToken((**token).clone())),
            },
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };

        let mut args: Args = SmallVec::new();
        let mut prev_token: Option<&TokenKind> = None;

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::RParen => match prev_token {
                    Some(TokenKind::Comma) => {
                        return Err(SyntaxError::UnexpectedToken(token.clone()));
                    }
                    _ => break,
                },
                TokenKind::Eof => match prev_token {
                    Some(TokenKind::RParen) => break,
                    Some(_) | None => {
                        return Err(SyntaxError::ExpectedClosingParen(
                            token.clone(),
                            opening_paren.clone().map(Box::new),
                        ));
                    }
                },
                TokenKind::Comma => match prev_token {
                    Some(_) => {
                        let token = match self.tokens.peek() {
                            Some(token) => Ok(Shared::new((*token).clone())),
                            None => Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
                        }?;
                        match &token.kind {
                            TokenKind::Comma => {
                                return Err(SyntaxError::UnexpectedToken((*token).clone()));
                            }
                            _ => continue,
                        }
                    }
                    None => return Err(SyntaxError::UnexpectedToken(token.clone())),
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
                return Err(SyntaxError::ExpectedClosingParen(
                    (**token).clone(),
                    opening_paren.clone().map(Box::new),
                ));
            }
        }

        Ok(args)
    }

    // Helper to parse an argument that is expected to be a general expression.
    // This typically involves a recursive call to `parse_expr`.
    #[inline(always)]
    fn parse_arg_expr(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let first = self.parse_expr(token)?;
        if !self.is_next_token(|kind| matches!(kind, TokenKind::Pipe)) {
            return Ok(first);
        }

        let token_id = first.token_id;
        let mut program = vec![first];
        while self.is_next_token(|kind| matches!(kind, TokenKind::Pipe)) {
            let pipe_token = self.next_or_eof()?;
            let next_token = match self.tokens.next() {
                Some(token) if token.kind == TokenKind::Eof => {
                    return Err(SyntaxError::UnexpectedEOFAfterToken(pipe_token.clone()));
                }
                Some(token) => token,
                None => return Err(SyntaxError::UnexpectedEOFAfterToken(pipe_token.clone())),
            };
            program.push(self.parse_expr(next_token)?);
        }

        Ok(Shared::new(Node {
            token_id,
            expr: Expr::Block(program),
        }))
    }

    fn parse_set_attr_call_with_selector(
        &mut self,
        selector_node: Shared<Node>,
        attr_literal: Shared<Node>,
    ) -> Result<Shared<Node>, SyntaxError> {
        let token = match self.tokens.next() {
            Some(token) => token,
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };
        let value = self.parse_expr(token)?;

        // Create the set_attr() function call
        Ok(Shared::new(Node {
            token_id: self.alloc_token(token),
            expr: Expr::Call(
                IdentWithToken::new_with_token(constants::builtins::SET_ATTR, Some(self.alloc_ident_token(token))),
                smallvec![selector_node, attr_literal, value],
            ),
        }))
    }

    /// Build an attr() call given a pre-built base node and an attribute selector token.
    fn build_attr_call_for_node(
        &mut self,
        base_node: Shared<Node>,
        attr_token: &Token,
        token: &Token,
    ) -> Result<Shared<Node>, SyntaxError> {
        if let TokenKind::Selector(attr_selector) = &attr_token.kind {
            if !Selector::try_from(attr_token)
                .map_err(SyntaxError::UnknownSelector)?
                .is_attribute_selector()
            {
                return Err(SyntaxError::UnexpectedToken(attr_token.clone()));
            }

            let attribute = &attr_selector[1..]; // Skip the dot
            let attr_literal = Shared::new(Node {
                token_id: self.alloc_token(token),
                expr: Expr::Literal(Literal::String(attribute.to_string())),
            });

            if self.is_next_token(|kind| matches!(kind, TokenKind::PipeEqual)) {
                self.tokens.next(); // consume '|=' token
                return self.parse_set_attr_call_with_selector(base_node, attr_literal);
            }

            Ok(Shared::new(Node {
                token_id: self.alloc_token(token),
                expr: Expr::Call(
                    IdentWithToken::new_with_token(constants::builtins::ATTR, Some(self.alloc_ident_token(token))),
                    smallvec![base_node, attr_literal],
                ),
            }))
        } else {
            Err(SyntaxError::UnexpectedToken(token.clone()))
        }
    }

    /// Consumes any selector token(s) following an already-parsed `base_node`.
    fn parse_selector_tail(&mut self, token: &Token, base_node: Shared<Node>) -> Result<Shared<Node>, SyntaxError> {
        if !self.is_next_token(|kind| matches!(kind, TokenKind::Selector(_))) {
            return Ok(base_node);
        }
        let next_token = self.next_or_eof()?;
        let selector = Selector::try_from(next_token).map_err(SyntaxError::UnknownSelector)?;

        if selector.is_attribute_selector() {
            return self.build_attr_call_for_node(base_node, next_token, token);
        }

        // Descendant chain: `.blockquote .code` → Block([base, .., .code, ...])
        let mut nodes: Program = vec![base_node];
        let mut step_token = next_token;
        let mut step_selector = selector;

        loop {
            nodes.push(Shared::new(Node {
                token_id: self.alloc_token(token),
                expr: Expr::Selector(Selector::Recursive),
            }));

            let step_expr = if self.is_next_token(|kind| matches!(kind, TokenKind::LParen)) {
                Expr::SelectorCall(step_selector, Box::new(self.parse_args()?))
            } else {
                Expr::Selector(step_selector)
            };
            nodes.push(Shared::new(Node {
                token_id: self.alloc_token(step_token),
                expr: step_expr,
            }));

            if !self.is_next_token(|kind| matches!(kind, TokenKind::Selector(_))) {
                break;
            }
            let peeked_token = self.next_or_eof()?;
            let peeked_selector = Selector::try_from(peeked_token).map_err(SyntaxError::UnknownSelector)?;

            if peeked_selector.is_attribute_selector() {
                let chained = Shared::new(Node {
                    token_id: self.alloc_token(token),
                    expr: Expr::Block(nodes),
                });
                return self.build_attr_call_for_node(chained, peeked_token, token);
            }

            step_token = peeked_token;
            step_selector = peeked_selector;
        }

        Ok(Shared::new(Node {
            token_id: self.alloc_token(token),
            expr: Expr::Block(nodes),
        }))
    }

    /// Parse a selector without checking for attributes (to avoid infinite recursion)
    fn parse_selector_direct(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        match &token.kind {
            TokenKind::Selector(selector) => {
                if selector == "." {
                    if self.is_next_token(|token_kind| matches!(token_kind, TokenKind::LBracket)) {
                        self.parse_selector_table_args(token)
                    } else {
                        Ok(Shared::new(Node {
                            token_id: self.alloc_token(token),
                            expr: Expr::Self_,
                        }))
                    }
                } else {
                    let selector = Selector::try_from(token).map_err(SyntaxError::UnknownSelector)?;

                    if selector.is_attribute_selector() {
                        let token_id = self.alloc_token(token);
                        let self_node = Shared::new(Node {
                            token_id,
                            expr: Expr::Self_,
                        });
                        let attr_literal = Shared::new(Node {
                            token_id,
                            expr: Expr::Literal(Literal::String(selector.name())),
                        });
                        if self.is_next_token(|kind| matches!(kind, TokenKind::PipeEqual)) {
                            self.tokens.next();
                            return self.parse_set_attr_call_with_selector(self_node, attr_literal);
                        }
                        return Ok(Shared::new(Node {
                            token_id,
                            expr: Expr::Call(
                                IdentWithToken::new_with_token(
                                    constants::builtins::ATTR,
                                    Some(self.alloc_ident_token(token)),
                                ),
                                smallvec![self_node, attr_literal],
                            ),
                        }));
                    }

                    // ."key"[] or ."key"[n]: array iteration/indexing on the property value
                    if matches!(selector, Selector::Property(_))
                        && self.is_next_token(|kind| matches!(kind, TokenKind::LBracket))
                    {
                        let prop_node = Shared::new(Node {
                            token_id: self.alloc_token(token),
                            expr: Expr::Selector(selector),
                        });
                        return self.parse_property_iterator(token, vec![prop_node]);
                    }

                    Ok(Shared::new(Node {
                        token_id: self.alloc_token(token),
                        expr: Expr::Selector(selector),
                    }))
                }
            }
            TokenKind::DoubleDot => {
                let recursive_node = Shared::new(Node {
                    token_id: self.alloc_token(token),
                    expr: Expr::Selector(Selector::Recursive),
                });
                if let Some(next) = self.tokens.peek() {
                    match &next.kind {
                        // .."key" → recursive descent + Dict key access
                        TokenKind::StringLiteral(key) => {
                            let key = key.clone();
                            let next = self.next_or_eof()?;
                            let prop_node = Shared::new(Node {
                                token_id: self.alloc_token(next),
                                expr: Expr::Selector(Selector::Property(Ident::new(key.as_str()))),
                            });
                            let base_node = Shared::new(Node {
                                token_id: self.alloc_token(token),
                                expr: Expr::Block(vec![recursive_node, prop_node]),
                            });
                            return self.parse_selector_tail(token, base_node);
                        }
                        // ..text, ..h, ..code, etc. → recursive descent + Markdown node-type selector
                        // Attribute selectors (.value, .url, .level, etc.) are excluded here;
                        // use `.. | .value` instead, which routes through the proper attr() call.
                        TokenKind::Ident(name) => {
                            let selector_str = format!(".{}", name);
                            if let Some(selector) = Selector::from_selector_str(&selector_str)
                                && !selector.is_attribute_selector()
                            {
                                let next = self.next_or_eof()?;
                                let sel_node = Shared::new(Node {
                                    token_id: self.alloc_token(next),
                                    expr: Expr::Selector(selector),
                                });

                                let base_node = Shared::new(Node {
                                    token_id: self.alloc_token(token),
                                    expr: Expr::Block(vec![recursive_node, sel_node]),
                                });
                                return self.parse_selector_tail(token, base_node);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(recursive_node)
            }
            _ => Err(SyntaxError::InsufficientTokens(token.clone())),
        }
    }

    #[inline(never)]
    fn parse_selector(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        // Handle chained property access: .a.b.c → Block([Selector(Property("a")), ...])
        if let TokenKind::Selector(_) = &token.kind
            && matches!(Selector::try_from(token), Ok(Selector::Property(_)))
        {
            let next_is_property = self
                .tokens
                .peek()
                .is_some_and(|t| matches!(Selector::try_from(*t), Ok(Selector::Property(_))));
            if next_is_property {
                return self.parse_chained_property_selectors(token);
            }
        }

        if self.is_next_token(|kind| matches!(kind, TokenKind::Selector(_))) {
            let base_node = self.parse_selector_direct(token)?;
            return self.parse_selector_tail(token, base_node);
        }

        // Check for selector call: `.h(...)`, `.code(...)`
        if self.is_next_token(|kind| matches!(kind, TokenKind::LParen))
            && let TokenKind::Selector(s) = &token.kind
            && s != "."
        {
            let selector = Selector::try_from(token).map_err(SyntaxError::UnknownSelector)?;
            if !selector.is_attribute_selector() {
                let args = self.parse_args()?;
                let base_node = Shared::new(Node {
                    token_id: self.alloc_token(token),
                    expr: Expr::SelectorCall(selector, Box::new(args)),
                });
                // Check for attribute access or a descendant chain continuation: `.h(1).level`, `.h(1) .code`
                return self.parse_selector_tail(token, base_node);
            }
        }

        self.parse_selector_direct(token)
    }

    fn parse_chained_property_selectors(&mut self, first_token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let first_sel = Selector::try_from(first_token).map_err(SyntaxError::UnknownSelector)?;
        let mut nodes: Program = vec![Shared::new(Node {
            token_id: self.alloc_token(first_token),
            expr: Expr::Selector(first_sel),
        })];

        while self.is_next_token(|kind| matches!(kind, TokenKind::Selector(_))) {
            let next_is_property = self
                .tokens
                .peek()
                .is_some_and(|t| matches!(Selector::try_from(*t), Ok(Selector::Property(_))));
            if !next_is_property {
                break;
            }
            let next_token = self.next_or_eof()?;
            let sel = Selector::try_from(next_token).map_err(SyntaxError::UnknownSelector)?;
            nodes.push(Shared::new(Node {
                token_id: self.alloc_token(next_token),
                expr: Expr::Selector(sel),
            }));
        }

        // ."a"."b"[] or ."a"."b"[n]: array iteration/indexing after a chained property access
        if self.is_next_token(|kind| matches!(kind, TokenKind::LBracket)) {
            return self.parse_property_iterator(first_token, nodes);
        }

        Ok(Shared::new(Node {
            token_id: self.alloc_token(first_token),
            expr: Expr::Block(nodes),
        }))
    }

    fn parse_property_iterator(&mut self, token: &Token, mut nodes: Program) -> Result<Shared<Node>, SyntaxError> {
        let index = self.parse_bracket_expr()?;

        let list_selector = match &index {
            None => Selector::List(None, None),
            Some(node) => {
                if let Expr::Literal(Literal::Number(num)) = &node.expr {
                    Selector::List(Some(num.value() as usize), None)
                } else {
                    // Dynamic index expression: emit a SelectorCall so the index is evaluated at runtime
                    let token_id = self.alloc_token(token);
                    nodes.push(Shared::new(Node {
                        token_id,
                        expr: Expr::SelectorCall(Selector::List(None, None), Box::new(smallvec![Shared::clone(node)])),
                    }));
                    return Ok(Shared::new(Node {
                        token_id: self.alloc_token(token),
                        expr: Expr::Block(nodes),
                    }));
                }
            }
        };

        nodes.push(Shared::new(Node {
            token_id: self.alloc_token(token),
            expr: Expr::Selector(list_selector),
        }));

        Ok(Shared::new(Node {
            token_id: self.alloc_token(token),
            expr: Expr::Block(nodes),
        }))
    }

    // Parses arguments for table or list item selectors like `.[index1][index2]` (for tables) or `.[index1]` (for lists).
    fn parse_selector_table_args(&mut self, token: &Token) -> Result<Shared<Node>, SyntaxError> {
        let first = self.parse_bracket_expr()?;
        let has_second = self.is_next_token(|kind| matches!(kind, TokenKind::LBracket));
        let second = if has_second {
            Some(self.parse_bracket_expr()?)
        } else {
            None
        };

        // Only non-negative integer literals fit a static `usize` index; others are resolved at runtime.
        let is_dynamic_node = |opt: &Option<Shared<Node>>| {
            opt.as_ref().is_some_and(
                |n| !matches!(&n.expr, Expr::Literal(Literal::Number(num)) if num.is_int() && num.value() >= 0.0),
            )
        };
        let has_dynamic = is_dynamic_node(&first) || second.as_ref().is_some_and(is_dynamic_node);
        let has_explicit_args = self.is_next_token(|kind| matches!(kind, TokenKind::LParen));

        if has_dynamic || has_explicit_args {
            let is_table = second.is_some();
            let selector = if is_table {
                Selector::Table(None, None)
            } else {
                Selector::List(None, None)
            };

            let mut args: Args = SmallVec::new();

            // .[][v]: insert None as row placeholder so args[0]=row, args[1]=col positional encoding holds.
            if is_table && first.is_none() && second.as_ref().is_some_and(|s| s.is_some()) {
                let placeholder_token_id = self.alloc_token(token);
                args.push(Shared::new(Node {
                    token_id: placeholder_token_id,
                    expr: Expr::Literal(Literal::None),
                }));
            } else if let Some(node) = first {
                args.push(node);
            }

            if let Some(Some(node)) = second {
                args.push(node);
            }
            if has_explicit_args {
                args.extend(self.parse_args()?);
            }

            let token_id = self.alloc_token(token);
            return Ok(Shared::new(Node {
                token_id,
                expr: Expr::SelectorCall(selector, Box::new(args)),
            }));
        }

        let static_index = |opt: Option<Shared<Node>>| -> Option<usize> {
            opt.and_then(|n| {
                if let Expr::Literal(Literal::Number(num)) = &n.expr {
                    Some(num.value() as usize)
                } else {
                    None
                }
            })
        };
        let i1 = static_index(first);
        let selector = match second {
            None => Selector::List(i1, None),
            Some(opt) => Selector::Table(i1, static_index(opt)),
        };

        let token_id = self.alloc_token(token);
        Ok(Shared::new(Node {
            token_id,
            expr: Expr::Selector(selector),
        }))
    }

    fn parse_bracket_expr(&mut self) -> Result<Option<Shared<Node>>, SyntaxError> {
        let bracket_token = match self.tokens.peek() {
            Some(t) => (*t).clone(),
            None => return Err(SyntaxError::UnexpectedEOFDetected(self.module_id)),
        };
        self.next_token(|kind| matches!(kind, TokenKind::LBracket))?;

        if self.is_next_token(|kind| matches!(kind, TokenKind::RBracket)) {
            self.tokens.next();
            return Ok(None);
        }

        let expr_token = match self.tokens.next() {
            Some(t) => t,
            None => return Err(SyntaxError::InsufficientTokens(bracket_token.clone())),
        };
        let node = self.parse_expr(expr_token)?;
        self.next_token(|kind| matches!(kind, TokenKind::RBracket))?;
        Ok(Some(node))
    }

    fn next_token(&mut self, expected_kinds: fn(&TokenKind) -> bool) -> Result<TokenId, SyntaxError> {
        match self.tokens.peek() {
            // Token found and matches one of the expected kinds.
            Some(token) if expected_kinds(&token.kind) => {
                let token = self.next_or_eof()?;
                Ok(self.alloc_token(token))
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
mod tests;
