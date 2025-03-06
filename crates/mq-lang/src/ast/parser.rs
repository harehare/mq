use std::cell::RefCell;
use std::iter::Peekable;
use std::rc::Rc;

use crate::arena::Arena;
use crate::eval::module::ModuleId;
use crate::lexer::token::{Token, TokenKind};
use compact_str::CompactString;

use super::Program;
use super::error::ParseError;
use super::node::{Expr, Ident, Literal, Node, Selector, TokenId};

#[derive(Debug)]
struct ArrayIndex(Option<usize>);

pub struct Parser<'a> {
    tokens: Peekable<core::slice::Iter<'a, Rc<Token>>>,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    module_id: ModuleId,
}

impl<'a> Parser<'a> {
    pub fn new(
        tokens: core::slice::Iter<'a, Rc<Token>>,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
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
        let mut asts = Vec::with_capacity(1000);

        while let Some(token) = self.tokens.next() {
            match &token.kind {
                TokenKind::Pipe | TokenKind::Comment(_) => continue,
                TokenKind::Eof => break,
                TokenKind::SemiColon => {
                    if root {
                        if let Some(token) = self.tokens.peek() {
                            if let TokenKind::Eof = &token.kind {
                                break;
                            } else {
                                return Err(ParseError::UnexpectedEOFDetected(self.module_id));
                            }
                        }
                    }

                    break;
                }
                TokenKind::NewLine | TokenKind::Tab(_) | TokenKind::Whitespace(_) => unreachable!(),
                _ => {
                    let ast = self.parse_expr(Rc::clone(token))?;
                    asts.push(ast);
                }
            }
        }

        Ok(asts)
    }

    fn parse_expr(&mut self, token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        match &token.kind {
            TokenKind::Selector(_) => self.parse_selector(token),
            TokenKind::Let => self.parse_let(token),
            TokenKind::Def => self.parse_def(token),
            TokenKind::While => self.parse_while(token),
            TokenKind::Until => self.parse_until(token),
            TokenKind::Foreach => self.parse_foreach(token),
            TokenKind::If => self.parse_if(token),
            TokenKind::Include => self.parse_include(token),
            TokenKind::Self_ => self.parse_self(token),
            TokenKind::Ident(name) => self.parse_ident(name, Rc::clone(&token)),
            TokenKind::BoolLiteral(_) => self.parse_literal(token),
            TokenKind::StringLiteral(_) => self.parse_literal(token),
            TokenKind::NumberLiteral(_) => self.parse_literal(token),
            TokenKind::None => self.parse_literal(token),
            TokenKind::Eof => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
            _ => Err(ParseError::UnexpectedToken((*token).clone())),
        }
    }

    fn parse_self(&mut self, token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        Ok(Rc::new(Node {
            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
            expr: Rc::new(Expr::Self_),
        }))
    }

    fn parse_literal(&mut self, literal_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        let literal_node = match &literal_token.kind {
            TokenKind::BoolLiteral(b) => Ok(Rc::new(Node {
                token_id: self
                    .token_arena
                    .borrow_mut()
                    .alloc(Rc::clone(&literal_token)),
                expr: Rc::new(Expr::Literal(Literal::Bool(*b))),
            })),
            TokenKind::StringLiteral(s) => Ok(Rc::new(Node {
                token_id: self
                    .token_arena
                    .borrow_mut()
                    .alloc(Rc::clone(&literal_token)),
                expr: Rc::new(Expr::Literal(Literal::String(s.to_string()))),
            })),
            TokenKind::NumberLiteral(n) => Ok(Rc::new(Node {
                token_id: self
                    .token_arena
                    .borrow_mut()
                    .alloc(Rc::clone(&literal_token)),
                expr: Rc::new(Expr::Literal(Literal::Number(*n))),
            })),
            TokenKind::None => Ok(Rc::new(Node {
                token_id: self
                    .token_arena
                    .borrow_mut()
                    .alloc(Rc::clone(&literal_token)),
                expr: Rc::new(Expr::Literal(Literal::None)),
            })),
            _ => Err(ParseError::UnexpectedToken((*literal_token).clone())),
        }?;

        let token = self.tokens.peek();

        match token.map(|t| &t.kind) {
            Some(TokenKind::Comma)
            | Some(TokenKind::Else)
            | Some(TokenKind::Elif)
            | Some(TokenKind::RParen)
            | Some(TokenKind::Pipe)
            | Some(TokenKind::SemiColon)
            | Some(TokenKind::Eof)
            | None => Ok(literal_node),
            Some(_) => Err(ParseError::UnexpectedToken((***token.unwrap()).clone())),
        }
    }

    fn parse_ident(&mut self, ident: &str, ident_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        match self.tokens.peek().map(|t| &t.kind) {
            Some(TokenKind::LParen) => {
                let args = self.parse_args()?;

                let optional = if let Some(token) = &self.tokens.peek() {
                    matches!(&token.kind, TokenKind::Question)
                } else {
                    false
                };

                if optional {
                    let _ = self.tokens.next();
                }

                Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&ident_token)),
                    expr: Rc::new(Expr::Call(
                        Ident::new_with_token(ident, Some(Rc::clone(&ident_token))),
                        args,
                        optional,
                    )),
                }))
            }
            Some(TokenKind::Comma)
            | Some(TokenKind::RParen)
            | Some(TokenKind::Pipe)
            | Some(TokenKind::Else)
            | Some(TokenKind::SemiColon)
            | Some(TokenKind::Eof)
            | None => Ok(Rc::new(Node {
                token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&ident_token)),
                expr: Rc::new(Expr::Ident(Ident::new_with_token(
                    ident,
                    Some(Rc::clone(&ident_token)),
                ))),
            })),
            _ => Err(ParseError::UnexpectedToken((*ident_token).clone())),
        }
    }

    fn parse_def(&mut self, def_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
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
        let def_token_id = self.token_arena.borrow_mut().alloc(def_token);
        let args = self.parse_args()?;

        if !args.is_empty() && !args.iter().all(|a| matches!(&*a.expr, Expr::Ident(_))) {
            return Err(ParseError::UnexpectedToken(
                (*self.token_arena.borrow()[def_token_id]).clone(),
            ));
        }

        let token_id = args
            .last()
            .map(|last| last.token_id)
            .unwrap_or(def_token_id);
        self.next_token_without_eof(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        let program = self.parse_program(false)?;
        Ok(Rc::new(Node {
            token_id: def_token_id,
            expr: Rc::new(Expr::Def(
                Ident::new_with_token(ident, ident_token.map(Rc::clone)),
                args,
                program,
            )),
        }))
    }

    fn parse_while(&mut self, while_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        let token_id = self.token_arena.borrow_mut().alloc(Rc::clone(&while_token));
        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(ParseError::UnexpectedToken((*while_token).clone()));
        }

        self.next_token_without_eof(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        match self.tokens.peek() {
            Some(_) => {
                let cond = args.first().unwrap();
                let body_program = self.parse_program(false)?;

                Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&while_token)),
                    expr: Rc::new(Expr::While(
                        Rc::clone(cond),
                        body_program.iter().map(Rc::clone).collect(),
                    )),
                }))
            }
            None => Err(ParseError::UnexpectedToken((*while_token).clone())),
        }
    }

    fn parse_until(&mut self, until_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        let token_id = self.token_arena.borrow_mut().alloc(Rc::clone(&until_token));
        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(ParseError::UnexpectedToken((*until_token).clone()));
        }

        self.next_token_without_eof(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        match self.tokens.peek() {
            Some(_) => {
                let cond = args.first().unwrap();
                let body_program = self.parse_program(false)?;

                Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&until_token)),
                    expr: Rc::new(Expr::Until(
                        Rc::clone(cond),
                        body_program.iter().map(Rc::clone).collect(),
                    )),
                }))
            }
            None => Err(ParseError::UnexpectedToken((*until_token).clone())),
        }
    }

    fn parse_foreach(&mut self, foreach_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        let token_id = self
            .token_arena
            .borrow_mut()
            .alloc(Rc::clone(&foreach_token));
        let args = self.parse_args()?;

        if args.len() != 2 {
            return Err(ParseError::UnexpectedToken((*foreach_token).clone()));
        }

        let first_arg = &*args.first().unwrap().expr;

        match first_arg {
            Expr::Ident(Ident {
                name: ident,
                token: ident_token,
            }) => {
                self.next_token_without_eof(token_id, |token_kind| {
                    matches!(token_kind, TokenKind::Colon)
                })?;

                let each_values = Rc::clone(&args[1]);
                let body_program = self.parse_program(false)?;

                Ok(Rc::new(Node {
                    token_id: self
                        .token_arena
                        .borrow_mut()
                        .alloc(Rc::clone(&foreach_token)),
                    expr: Rc::new(Expr::Foreach(
                        Ident::new_with_token(ident, ident_token.clone()),
                        Rc::clone(&each_values),
                        body_program.iter().map(Rc::clone).collect(),
                    )),
                }))
            }
            _ => Err(ParseError::UnexpectedToken((*foreach_token).clone())),
        }
    }

    fn parse_if(&mut self, if_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        let mut nodes = Vec::with_capacity(10);
        let token_id = self.token_arena.borrow_mut().alloc(Rc::clone(&if_token));
        let args = self.parse_args()?;

        if args.len() != 1 {
            return Err(ParseError::UnexpectedToken(
                (*self.token_arena.borrow()[token_id]).clone(),
            ));
        }

        let token_id = self.next_token_without_eof(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        let if_expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(ParseError::UnexpectedToken(
                (*self.token_arena.borrow()[token_id]).clone(),
            )),
        }?;

        let cond = args.first().unwrap();
        let then_expr = self.parse_expr(Rc::clone(if_expr_token))?;

        nodes.push((Some(Rc::clone(cond)), then_expr));

        while let Some(token) = self.tokens.peek() {
            if matches!(token.kind, TokenKind::Else) {
                break;
            }

            let token_id = self.next_token_without_eof(token_id, |token_kind| {
                matches!(token_kind, TokenKind::Elif)
            })?;
            let args = self.parse_args()?;

            if args.len() != 1 {
                return Err(ParseError::UnexpectedToken(
                    (*self.token_arena.borrow()[token_id]).clone(),
                ));
            }

            let token_id = self.next_token_without_eof(token_id, |token_kind| {
                matches!(token_kind, TokenKind::Colon)
            })?;

            let expr_token = match self.tokens.next() {
                Some(token) => Ok(token),
                None => Err(ParseError::UnexpectedToken(
                    (*self.token_arena.borrow()[token_id]).clone(),
                )),
            }?;

            let cond = args.first().unwrap();
            let then_expr = self.parse_expr(Rc::clone(expr_token))?;

            nodes.push((Some(Rc::clone(cond)), then_expr));
        }

        let token_id = self
            .next_token_without_eof(token_id, |token_kind| matches!(token_kind, TokenKind::Else))?;
        let token_id = self.next_token_without_eof(token_id, |token_kind| {
            matches!(token_kind, TokenKind::Colon)
        })?;

        let else_expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(ParseError::UnexpectedToken(
                (*self.token_arena.borrow()[token_id]).clone(),
            )),
        }?;

        let else_expr = self.parse_expr(Rc::clone(else_expr_token))?;

        nodes.push((None, else_expr));

        Ok(Rc::new(Node {
            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&if_token)),
            expr: Rc::new(Expr::If(nodes)),
        }))
    }

    fn parse_let(&mut self, let_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
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

        let let_token_id = self.token_arena.borrow_mut().alloc(Rc::clone(&let_token));
        self.next_token_without_eof(let_token_id, |token_kind| {
            matches!(token_kind, TokenKind::Equal)
        })?;
        let expr_token = match self.tokens.next() {
            Some(token) => Ok(token),
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }?;

        let ast = self.parse_expr(Rc::clone(expr_token))?;

        self.next_token_with_eof(let_token_id, |token_kind| {
            matches!(token_kind, TokenKind::Pipe) || matches!(token_kind, TokenKind::Eof)
        })?;

        Ok(Rc::new(Node {
            token_id: let_token_id,
            expr: Rc::new(Expr::Let(
                Ident::new_with_token(ident, ident_token.map(Rc::clone)),
                ast,
            )),
        }))
    }

    fn parse_include(&mut self, include_token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        match self.tokens.peek() {
            Some(token) => match &***token {
                Token {
                    range: _,
                    kind: TokenKind::StringLiteral(module),
                    module_id: _,
                } => {
                    self.tokens.next();
                    Ok(Rc::new(Node {
                        token_id: self
                            .token_arena
                            .borrow_mut()
                            .alloc(Rc::clone(&include_token)),
                        expr: Rc::new(Expr::Include(Literal::String(module.to_string()))),
                    }))
                }
                token => Err(ParseError::InsufficientTokens((*token).clone())),
            },
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Rc<Node>>, ParseError> {
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

        let mut args: Vec<Rc<Node>> = Vec::new();
        let mut prev_token: Option<&Token> = None;

        while let Some(token) = self.tokens.next() {
            match &**token {
                Token {
                    range: _,
                    kind: TokenKind::Ident(_),
                    module_id: _,
                } => {
                    let expr = self.parse_expr(Rc::clone(token))?;
                    args.push(expr);
                }
                Token {
                    range: _,
                    kind: TokenKind::Selector(_),
                    module_id: _,
                } => {
                    let expr = self.parse_expr(Rc::clone(token))?;
                    args.push(expr);
                }
                Token {
                    range: _,
                    kind: TokenKind::BoolLiteral(b),
                    module_id: _,
                } => {
                    args.push(Rc::new(Node {
                        token_id: self.token_arena.borrow_mut().alloc(Rc::clone(token)),
                        expr: Rc::new(Expr::Literal(Literal::Bool(*b))),
                    }));
                }
                Token {
                    range: _,
                    kind: TokenKind::NumberLiteral(n),
                    module_id: _,
                } => {
                    args.push(Rc::new(Node {
                        token_id: self.token_arena.borrow_mut().alloc(Rc::clone(token)),
                        expr: Rc::new(Expr::Literal(Literal::Number(*n))),
                    }));
                }
                Token {
                    range: _,
                    kind: TokenKind::StringLiteral(s),
                    module_id: _,
                } => {
                    args.push(Rc::new(Node {
                        token_id: self.token_arena.borrow_mut().alloc(Rc::clone(token)),
                        expr: Rc::new(Expr::Literal(Literal::String(s.to_string()))),
                    }));
                }
                Token {
                    range: _,
                    kind: TokenKind::None,
                    module_id: _,
                } => {
                    args.push(Rc::new(Node {
                        token_id: self.token_arena.borrow_mut().alloc(Rc::clone(token)),
                        expr: Rc::new(Expr::Literal(Literal::None)),
                    }));
                }
                Token {
                    range: _,
                    kind: TokenKind::Env(s),
                    module_id: _,
                } => {
                    args.push(Rc::new(Node {
                        token_id: self.token_arena.borrow_mut().alloc(Rc::clone(token)),
                        expr: std::env::var(s)
                            .map_err(|_| {
                                ParseError::EnvNotFound((**token).clone(), CompactString::new(s))
                            })
                            .map(|s| Rc::new(Expr::Literal(Literal::String(s.to_string()))))?,
                    }));
                }
                Token {
                    range: _,
                    kind: TokenKind::LParen,
                    module_id: _,
                } => {
                    return Err(ParseError::UnexpectedToken((**token).clone()));
                }
                Token {
                    range: _,
                    kind: TokenKind::Self_,
                    module_id: _,
                } => {
                    args.push(Rc::new(Node {
                        token_id: self.token_arena.borrow_mut().alloc(Rc::clone(token)),
                        expr: Rc::new(Expr::Self_),
                    }));
                }
                Token {
                    range: _,
                    kind: TokenKind::If,
                    module_id: _,
                } => {
                    let expr = self.parse_expr(Rc::clone(token))?;
                    args.push(expr);
                }
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
                    kind: TokenKind::Else,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Elif,
                    module_id: _,
                } => {
                    return Err(ParseError::UnexpectedToken((**token).clone()));
                }
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
                    Some(_) => {
                        return Err(ParseError::UnexpectedEOFDetected(self.module_id));
                    }
                    None => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
                },
                Token {
                    range: _,
                    kind: TokenKind::Comma,
                    module_id: _,
                } => continue,
                Token {
                    range: _,
                    kind: TokenKind::SemiColon,
                    module_id: _,
                } => return Err(ParseError::UnexpectedEOFDetected(self.module_id)),
                Token {
                    range: _,
                    kind: TokenKind::Def,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Include,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Equal,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::LBracket,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::RBracket,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Pipe,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Colon,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Let,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::While,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Until,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Foreach,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Comment(_),
                    module_id: _,
                } => return Err(ParseError::UnexpectedToken((**token).clone())),
                Token {
                    range: _,
                    kind: TokenKind::NewLine,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Whitespace(_),
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Question,
                    module_id: _,
                }
                | Token {
                    range: _,
                    kind: TokenKind::Tab(_),
                    module_id: _,
                } => {
                    return Err(ParseError::UnexpectedToken((**token).clone()));
                }
            }
            prev_token = Some(token);
        }

        Ok(args)
    }

    fn parse_head(&mut self, token: Rc<Token>, depth: u8) -> Result<Rc<Node>, ParseError> {
        Ok(Rc::new(Node {
            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
            expr: Rc::new(Expr::Selector(Selector::Heading(Some(depth)))),
        }))
    }

    fn parse_selector(&mut self, token: Rc<Token>) -> Result<Rc<Node>, ParseError> {
        if let TokenKind::Selector(selector) = &token.kind {
            match selector.as_str() {
                ".h" => {
                    if let Ok(depth) = self.parse_int_arg(Rc::clone(&token)) {
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::Heading(Some(depth as u8)))),
                        }))
                    } else {
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::Heading(None))),
                        }))
                    }
                }
                ".h1" | ".#" => self.parse_head(token, 1),
                ".h2" | ".##" => self.parse_head(token, 2),
                ".h3" | ".###" => self.parse_head(token, 3),
                ".h4" | ".####" => self.parse_head(token, 4),
                ".h5" | ".#####" => self.parse_head(token, 5),
                ".>" | ".blockquote" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Blockquote)),
                })),
                ".^" | ".footnote" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Footnote)),
                })),
                ".<" | ".mdx_jsx_flow_element" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::MdxJsxFlowElement)),
                })),
                ".**" | ".emphasis" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Emphasis)),
                })),
                ".$$" | ".math" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Math)),
                })),
                ".horizontal_rule" | ".---" | ".***" | ".___" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::HorizontalRule)),
                })),
                ".{}" | ".mdx_text_expression" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::MdxTextExpression)),
                })),
                ".[^]" | ".footnote_ref" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::FootnoteRef)),
                })),
                ".definition" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Definition)),
                })),
                ".break" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Break)),
                })),
                ".delete" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Delete)),
                })),
                ".<>" | ".html" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Html)),
                })),
                ".image" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Image)),
                })),
                ".image_ref" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::ImageRef)),
                })),
                ".code_inline" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::InlineCode)),
                })),
                ".math_inline" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::InlineMath)),
                })),
                ".link" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Link)),
                })),
                ".link_ref" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::LinkRef)),
                })),
                ".list.checked" => {
                    if let Ok(i) = self.parse_int_arg(Rc::clone(&token)) {
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::List(
                                Some(i as usize),
                                Some(true),
                            ))),
                        }))
                    } else {
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::List(None, Some(true)))),
                        }))
                    }
                }
                ".list" => {
                    if let Ok(i) = self.parse_int_arg(Rc::clone(&token)) {
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::List(Some(i as usize), None))),
                        }))
                    } else {
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::List(None, None))),
                        }))
                    }
                }
                ".toml" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Toml)),
                })),
                ".strong" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Strong)),
                })),
                ".yaml" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::Yaml)),
                })),
                ".code" => {
                    if let Ok(s) = self.parse_string_arg(Rc::clone(&token)) {
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::Code(Some(
                                CompactString::new(s),
                            )))),
                        }))
                    } else {
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::Code(None))),
                        }))
                    }
                }
                ".mdx_js_esm" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::MdxJsEsm)),
                })),
                ".mdx_jsx_text_element" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::MdxJsxTextElement)),
                })),
                ".mdx_flow_expression" => Ok(Rc::new(Node {
                    token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                    expr: Rc::new(Expr::Selector(Selector::MdxFlowExpression)),
                })),
                // .[], .[n] .[][], .[n][n]
                "." => {
                    let token1 = match self.tokens.peek() {
                        Some(token) => Ok(Rc::clone(token)),
                        None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
                    }?;

                    let ArrayIndex(i1) = self.parse_int_array_arg(&token1)?;
                    let token2 = match self.tokens.peek() {
                        Some(token) => Ok(Rc::clone(token)),
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
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::Table(i1, i2))),
                        }))
                    } else {
                        // .[n]
                        Ok(Rc::new(Node {
                            token_id: self.token_arena.borrow_mut().alloc(Rc::clone(&token)),
                            expr: Rc::new(Expr::Selector(Selector::List(i1, None))),
                        }))
                    }
                }
                _ => Err(ParseError::UnexpectedToken((*token).clone())),
            }
        } else {
            Err(ParseError::InsufficientTokens((*token).clone()))
        }
    }

    fn parse_int_arg(&mut self, token: Rc<Token>) -> Result<i64, ParseError> {
        let args = self.parse_int_args(Rc::clone(&token))?;

        if args.len() == 1 {
            Ok(args[0])
        } else {
            Err(ParseError::UnexpectedToken((*token).clone()))
        }
    }

    fn parse_string_arg(&mut self, token: Rc<Token>) -> Result<String, ParseError> {
        let args = self.parse_string_args(Rc::clone(&token))?;

        if args.len() == 1 {
            Ok(args[0].clone())
        } else {
            Err(ParseError::UnexpectedToken((*token).clone()))
        }
    }

    fn parse_int_array_arg(&mut self, token: &Rc<Token>) -> Result<ArrayIndex, ParseError> {
        let token_id = self.token_arena.borrow_mut().alloc(Rc::clone(token));
        self.next_token_without_eof(token_id, |token_kind| {
            matches!(token_kind, TokenKind::LBracket)
        })?;

        let token = match self.tokens.peek() {
            Some(token) => Ok(Rc::clone(token)),
            None => return Err(ParseError::InsufficientTokens((**token).clone())),
        }?;

        if let Token {
            range: _,
            kind: TokenKind::NumberLiteral(n),
            module_id: _,
        } = &*token
        {
            let token_id = self.token_arena.borrow_mut().alloc(Rc::clone(&token));
            self.tokens.next();
            self.next_token_without_eof(token_id, |token_kind| {
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

    fn parse_int_args(&mut self, arg_token: Rc<Token>) -> Result<Vec<i64>, ParseError> {
        let mut args = Vec::new();
        let token_id = self.token_arena.borrow_mut().alloc(Rc::clone(&arg_token));

        self.next_token_without_eof(token_id, |token_kind| {
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

    fn parse_string_args(&mut self, arg_token: Rc<Token>) -> Result<Vec<String>, ParseError> {
        let token_id = self.token_arena.borrow_mut().alloc(Rc::clone(&arg_token));
        self.next_token_without_eof(token_id, |token_kind| {
            matches!(token_kind, TokenKind::LParen)
        })?;

        let mut args = Vec::new();

        loop {
            match self.tokens.next() {
                Some(token) => match &**token {
                    Token {
                        range: _,
                        kind: TokenKind::StringLiteral(s),
                        module_id: _,
                    } => {
                        args.push(s.to_string());
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

    fn next_token_with_eof(
        &mut self,
        current_token_id: TokenId,
        expected_kinds: fn(&TokenKind) -> bool,
    ) -> Result<TokenId, ParseError> {
        self.next_token(current_token_id, expected_kinds, true)
    }

    fn next_token_without_eof(
        &mut self,
        current_token_id: TokenId,
        expected_kinds: fn(&TokenKind) -> bool,
    ) -> Result<TokenId, ParseError> {
        self.next_token(current_token_id, expected_kinds, false)
    }

    fn next_token(
        &mut self,
        current_token_id: TokenId,
        expected_kinds: fn(&TokenKind) -> bool,
        expected_eof: bool,
    ) -> Result<TokenId, ParseError> {
        match self.tokens.peek() {
            Some(token) if expected_kinds(&token.kind) => Ok(self
                .token_arena
                .borrow_mut()
                .alloc(Rc::clone(self.tokens.next().unwrap()))),
            Some(token) => Err(ParseError::UnexpectedToken(Token {
                range: token.range.clone(),
                kind: token.kind.clone(),
                module_id: token.module_id,
            })),
            None if expected_eof => {
                let range = self.token_arena.borrow()[current_token_id].range.clone();
                let module_id = self.token_arena.borrow()[current_token_id].module_id;
                Ok(Rc::clone(&self.token_arena)
                    .borrow_mut()
                    .alloc(Rc::new(Token {
                        range,
                        kind: TokenKind::Eof,
                        module_id,
                    })))
            }
            None => Err(ParseError::UnexpectedEOFDetected(self.module_id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Module, range::Range};

    use super::*;
    use compact_str::CompactString;
    use itertools::Itertools;
    use rstest::rstest;

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
            token(TokenKind::Ident(CompactString::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(CompactString::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test".to_string())),
            token(TokenKind::RParen),
            token(TokenKind::Comma),
            token(TokenKind::Ident(CompactString::new("startswith"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("test2".to_string())),
            token(TokenKind::RParen),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Rc::new(Node {
                token_id: 4.into(),
                expr: Rc::new(Expr::Call(
                    Ident::new_with_token("and", Some(Rc::new(token(TokenKind::Ident(CompactString::new("and")))))),
                    vec![
                        Rc::new(Node {
                            token_id: 1.into(),
                            expr: Rc::new(Expr::Call(
                                Ident::new_with_token("contains", Some(Rc::new(token(TokenKind::Ident(CompactString::new("contains")))))),
                                vec![Rc::new(Node {
                                    token_id: 0.into(),
                                    expr: Rc::new(Expr::Literal(Literal::String("test".to_string())))
                                })],
                                false,
                            ))
                        }),
                        Rc::new(Node {
                            token_id: 3.into(),
                            expr: Rc::new(Expr::Call(
                                Ident::new_with_token("startswith", Some(Rc::new(token(TokenKind::Ident(CompactString::new("startswith")))))),
                                vec![Rc::new(Node {
                                    token_id: 2.into(),
                                    expr: Rc::new(Expr::Literal(Literal::String("test2".to_string())))
                                })],
                                false
                            ))
                        })
                    ],
                    false,
                ))
            })
        ]))]
    #[case::ident2(
        vec![
            token(TokenKind::Ident(CompactString::new("and"))),
            token(TokenKind::LParen),
            token(TokenKind::Selector(CompactString::new(".h1"))),
            token(TokenKind::Comma),
            token(TokenKind::Selector(CompactString::new("."))),
            token(TokenKind::LBracket),
            token(TokenKind::NumberLiteral(2.into())),
            token(TokenKind::RBracket),
            token(TokenKind::LBracket),
            token(TokenKind::RBracket),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Rc::new(Node {
                token_id: 8.into(),
                expr: Rc::new(Expr::Call(
                    Ident::new_with_token("and", Some(Rc::new(token(TokenKind::Ident(CompactString::new("and")))))),
                    vec![
                        Rc::new(Node {
                            token_id: 0.into(),
                            expr: Rc::new(Expr::Selector(Selector::Heading(Some(1)))),
                        }),
                        Rc::new(Node {
                            token_id: 7.into(),
                            expr: Rc::new(Expr::Selector(Selector::Table(Some(2), None))),
                        }),
                    ],
                    false
                ))
            })
        ]))]
    #[case::ident3(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(CompactString::new("filter"))),
            token(TokenKind::LParen),
            token(TokenKind::Ident(CompactString::new("arg1"))),
            token(TokenKind::Comma),
            token(TokenKind::Ident(CompactString::new("arg2"))),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::Ident(CompactString::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("arg1".to_string())),
            token(TokenKind::Comma),
            token(TokenKind::StringLiteral("arg2".to_string())),
            token(TokenKind::RParen),
        ],
        Ok(vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(Expr::Def(
                    Ident::new_with_token("filter", Some(Rc::new(token(TokenKind::Ident(CompactString::new("filter")))))),
                    vec![
                        Rc::new(Node {
                            token_id: 1.into(),
                            expr: Rc::new(Expr::Ident(Ident::new_with_token("arg1", Some(Rc::new(token(TokenKind::Ident(CompactString::new("arg1")))))))),
                        }),
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(Expr::Ident(Ident::new_with_token("arg2", Some(Rc::new(token(TokenKind::Ident(CompactString::new("arg2")))))))),
                        }),
                    ],
                    vec![Rc::new(Node {
                        token_id: 6.into(),
                        expr: Rc::new(Expr::Call(
                            Ident::new_with_token("contains", Some(Rc::new(token(TokenKind::Ident(CompactString::new("contains")))))),
                            vec![
                                Rc::new(Node {
                                    token_id: 4.into(),
                                    expr: Rc::new(Expr::Literal(Literal::String("arg1".to_string()))),
                                }),
                                Rc::new(Node {
                                    token_id: 5.into(),
                                    expr: Rc::new(Expr::Literal(Literal::String("arg2".to_string()))),
                                }),
                            ],
                            false,
                        )),
                    })],
                )),
            }),
        ]))]
    #[case::error(
        vec![
            token(TokenKind::Ident(CompactString::new("contains"))),
            token(TokenKind::LParen),
            token(TokenKind::Selector(CompactString::new("inline_code"))),
            token(TokenKind::Eof)
        ],
        Err(ParseError::UnexpectedToken(token(TokenKind::Selector(CompactString::new("inline_code"))))))]
    #[case::def1(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(CompactString::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::RParen),
            token(TokenKind::Colon),
            token(TokenKind::StringLiteral("value".to_string())),
            token(TokenKind::SemiColon)
        ],
        Ok(vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(Expr::Def(
                        Ident::new_with_token("name", Some(Rc::new(token(TokenKind::Ident(CompactString::new("name")))))),
                        vec![],
                        vec![Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(Expr::Literal(Literal::String("value".to_string()))),
                        })],
                )),
            }),
        ]))]
    #[case::def2(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(CompactString::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::Comma),
            token(TokenKind::RParen),
        ],
        Err(ParseError::UnexpectedEOFDetected(Module::TOP_LEVEL_MODULE_ID)))]
    #[case::def3(
        vec![
            token(TokenKind::Def),
            token(TokenKind::Ident(CompactString::new("name"))),
            token(TokenKind::LParen),
            token(TokenKind::StringLiteral("value".to_string())),
            token(TokenKind::Comma),
            token(TokenKind::RParen),
        ],
        Err(ParseError::UnexpectedToken(Token{range: Range::default(), kind:TokenKind::Def, module_id: 1.into()})))]
    #[case::let_1(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(CompactString::new("x"))),
                token(TokenKind::Equal),
                token(TokenKind::NumberLiteral(42.into())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Rc::new(Node {
                    token_id: 0.into(),
                    expr: Rc::new(Expr::Let(
                        Ident::new_with_token("x", Some(Rc::new(token(TokenKind::Ident(CompactString::new("x")))))),
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(Expr::Literal(Literal::Number(42.into()))),
                        }),
                    )),
                })
            ]))]
    #[case::let_2(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(CompactString::new("y"))),
                token(TokenKind::Equal),
                token(TokenKind::StringLiteral("hello".to_string())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Rc::new(Node {
                    token_id: 0.into(),
                    expr: Rc::new(Expr::Let(
                        Ident::new_with_token("y", Some(Rc::new(token(TokenKind::Ident(CompactString::new("y")))))),
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(Expr::Literal(Literal::String("hello".to_string()))),
                        }),
                    )),
                })
            ]))]
    #[case::let_3(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(CompactString::new("flag"))),
                token(TokenKind::Equal),
                token(TokenKind::BoolLiteral(true)),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Rc::new(Node {
                    token_id: 0.into(),
                    expr: Rc::new(Expr::Let(
                        Ident::new_with_token("flag", Some(Rc::new(token(TokenKind::Ident(CompactString::new("flag")))))),
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(Expr::Literal(Literal::Bool(true))),
                        }),
                    )),
                })
            ]))]
    #[case::let_4(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(CompactString::new("z"))),
                token(TokenKind::Equal),
                token(TokenKind::Ident(CompactString::new("some_var"))),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Rc::new(Node {
                    token_id: 0.into(),
                    expr: Rc::new(Expr::Let(
                        Ident::new_with_token("z", Some(Rc::new(token(TokenKind::Ident("z".into()))))),
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(
                                Expr::Ident(Ident::new_with_token("some_var",
                                                 Some(Rc::new(token(TokenKind::Ident(CompactString::new("some_var"))))))))
                        }),
                    )),
                })
            ]))]
    #[case::let_5(
            vec![
                token(TokenKind::Let),
                token(TokenKind::Ident(CompactString::new("z"))),
                token(TokenKind::Equal),
                token(TokenKind::Ident(CompactString::new("some_var"))),
                token(TokenKind::Pipe),
            ],
            Ok(vec![
                Rc::new(Node {
                    token_id: 0.into(),
                    expr: Rc::new(Expr::Let(
                        Ident::new_with_token("z", Some(Rc::new(token(TokenKind::Ident("z".into()))))),
                        Rc::new(Node {
                            token_id: 2.into(),
                            expr: Rc::new(
                                Expr::Ident(Ident::new_with_token("some_var", Some(Rc::new(token(TokenKind::Ident(CompactString::new("some_var")))))))),
                        }),
                    )),
                })
            ]))]
    #[case::root_semicolon_error(
            vec![
                token(TokenKind::Ident(CompactString::new("x"))),
                token(TokenKind::SemiColon),
                token(TokenKind::Ident(CompactString::new("y"))),
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
                token(TokenKind::StringLiteral("true branch".to_string())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("false branch".to_string())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Rc::new(Node {
                    token_id: 7.into(),
                    expr: Rc::new(Expr::If(vec![
                        (
                            Some(Rc::new(Node {
                                token_id: 1.into(),
                                expr: Rc::new(Expr::Literal(Literal::Bool(true))),
                            })),
                            Rc::new(Node {
                                token_id: 3.into(),
                                expr: Rc::new(Expr::Literal(Literal::String("true branch".to_string()))),
                            })
                        ),
                        (
                            None,
                            Rc::new(Node {
                                token_id: 6.into(),
                                expr: Rc::new(Expr::Literal(Literal::String("false branch".to_string()))),
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
                token(TokenKind::StringLiteral("true branch".to_string())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(false)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_string())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("else branch".to_string())),
                token(TokenKind::Eof)
            ],
            Ok(vec![
                Rc::new(Node {
                    token_id: 11.into(),
                    expr: Rc::new(Expr::If(vec![
                        (
                            Some(Rc::new(Node {
                                token_id: 1.into(),
                                expr: Rc::new(Expr::Literal(Literal::Bool(true))),
                            })),
                            Rc::new(Node {
                                token_id: 3.into(),
                                expr: Rc::new(Expr::Literal(Literal::String("true branch".to_string()))),
                            })
                        ),
                        (
                            Some(Rc::new(Node {
                                token_id: 5.into(),
                                expr: Rc::new(Expr::Literal(Literal::Bool(false))),
                            })),
                            Rc::new(Node {
                                token_id: 7.into(),
                                expr: Rc::new(Expr::Literal(Literal::String("elif branch".to_string()))),
                            })
                        ),
                        (
                            None,
                            Rc::new(Node {
                                token_id: 10.into(),
                                expr: Rc::new(Expr::Literal(Literal::String("else branch".to_string()))),
                            })
                        )
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
                token(TokenKind::StringLiteral("true branch".to_string())),
                token(TokenKind::Elif),
                token(TokenKind::LParen),
                token(TokenKind::BoolLiteral(false)),
                token(TokenKind::RParen),
                token(TokenKind::Colon),
                token(TokenKind::StringLiteral("elif branch".to_string())),
                token(TokenKind::Else),
                token(TokenKind::Colon),
                token(TokenKind::Eof)
            ],
            Err(ParseError::UnexpectedEOFDetected(0.into())))]
    #[case::h_selector(
        vec![
            token(TokenKind::Selector(CompactString::new(".h"))),
            token(TokenKind::LParen),
            token(TokenKind::NumberLiteral(3.into())),
            token(TokenKind::RParen),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Rc::new(Node {
                token_id: 2.into(),
                expr: Rc::new(Expr::Selector(Selector::Heading(Some(3)))),
            })
        ]))]
    #[case::h_selector_without_number(
        vec![
            token(TokenKind::Selector(CompactString::new(".h"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Rc::new(Node {
                token_id: 1.into(),
                expr: Rc::new(Expr::Selector(Selector::Heading(None))),
            })
        ]))]
    #[case::h1_shorthand(
        vec![
            token(TokenKind::Selector(CompactString::new(".#"))),
            token(TokenKind::Eof)
        ],
        Ok(vec![
            Rc::new(Node {
                token_id: 0.into(),
                expr: Rc::new(Expr::Selector(Selector::Heading(Some(1)))),
            })
        ]))]
    fn test(#[case] input: Vec<Token>, #[case] expected: Result<Program, ParseError>) {
        let arena = Arena::new(10);
        assert_eq!(
            Parser::new(
                input.into_iter().map(Rc::new).collect_vec().iter(),
                Rc::new(RefCell::new(arena)),
                Module::TOP_LEVEL_MODULE_ID
            )
            .parse(),
            expected
        );
    }
}
