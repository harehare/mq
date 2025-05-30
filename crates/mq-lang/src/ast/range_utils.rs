use crate::{
    Token, // Assuming Token is accessible from crate root
    arena::Arena,
    range::Range,
    ast::{
        expr_ref::ExprRef,
        node::{Expr, Literal, Selector, Ident, StringSegment, Params, Args, Branches}, // Added Params, Args, Branches
        pool::ExprPool,
    },
    TokenId, // Assuming TokenId is accessible
};
use std::rc::Rc; // For Arena<Rc<Token>>

// Helper function to get token range
fn get_token_range(token_id: TokenId, token_arena: &Arena<Rc<Token>>) -> Range {
    token_arena[token_id].range.clone()
}

pub fn get_expr_range(
    expr_ref: ExprRef,
    pool: &ExprPool,
    token_arena: &Arena<Rc<Token>>,
) -> Range {
    let (expr_data, token_id) = pool.get(expr_ref).expect("Invalid ExprRef: Expression not found in pool");

    match expr_data {
        Expr::Def(_, params, body_exprs) => {
            // Range ideally from 'def' keyword token to end of last body expression or last param.
            // The associated token_id for Expr::Def should be the 'def' keyword's token.
            let start_pos = get_token_range(*token_id, token_arena).start;

            let end_pos = if let Some(last_expr_ref) = body_exprs.last() {
                get_expr_range(*last_expr_ref, pool, token_arena).end
            } else if let Some(last_param_ref) = params.last() {
                // If no body, end with last param's own token or the param list closing token.
                // For simplicity, using last param's expression range.
                // This might need refinement if params are just Idents (tokens) not full ExprRefs.
                // Assuming params are ExprRefs for now.
                get_expr_range(*last_param_ref, pool, token_arena).end
            } else {
                // No body and no params, ends with the 'def' keyword token.
                get_token_range(*token_id, token_arena).end
            };
            Range { start: start_pos, end: end_pos }
        }
        Expr::Fn(_, params, body_exprs) => {
            // Similar to Def. token_id is for 'fn' keyword.
            let start_pos = get_token_range(*token_id, token_arena).start;

            let end_pos = if let Some(last_expr_ref) = body_exprs.last() {
                get_expr_range(*last_expr_ref, pool, token_arena).end
            } else if let Some(last_param_ref) = params.last() {
                get_expr_range(*last_param_ref, pool, token_arena).end
            } else {
                get_token_range(*token_id, token_arena).end
            };
            Range { start: start_pos, end: end_pos }
        }
        Expr::While(condition_ref, body_exprs) | Expr::Until(condition_ref, body_exprs) => {
            // token_id is for 'while'/'until' keyword.
            // Range from keyword start to last body expression end.
            let start_pos = get_token_range(*token_id, token_arena).start;
            let end_pos = if let Some(last_expr_ref) = body_exprs.last() {
                get_expr_range(*last_expr_ref, pool, token_arena).end
            } else {
                // If no body, range ends with condition expression.
                get_expr_range(*condition_ref, pool, token_arena).end
            };
            Range { start: start_pos, end: end_pos }
        }
        Expr::Foreach(ident, iterable_ref, body_exprs) => {
            // token_id is for 'foreach' keyword.
            // Range from 'foreach' keyword to last body expr.
            // ident is an Ident struct, not an ExprRef, its range comes from ident.token if available
            // or is part of the 'foreach' token itself.
            let start_pos = get_token_range(*token_id, token_arena).start; 
            let end_pos = if let Some(last_expr_ref) = body_exprs.last() {
                get_expr_range(*last_expr_ref, pool, token_arena).end
            } else {
                // If no body, end with the iterable expression
                get_expr_range(*iterable_ref, pool, token_arena).end
            };
            Range { start: start_pos, end: end_pos }
        }
        Expr::Call(ident, args, _) => {
            // token_id is for the identifier of the call.
            // Range from call identifier token to the end of the last argument.
            let start_pos = get_token_range(*token_id, token_arena).start; // ident.token.range.start
            let end_pos = if let Some(last_arg_ref) = args.last() {
                get_expr_range(*last_arg_ref, pool, token_arena).end
            } else {
                // If no args, range is just the call identifier token itself.
                get_token_range(*token_id, token_arena).end
            };
            Range { start: start_pos, end: end_pos }
        }
        Expr::Let(ident, value_ref) => {
            // token_id is for 'let' keyword.
            // Range from 'let' keyword token to end of value expression.
            // ident is an Ident struct.
            let start_pos = get_token_range(*token_id, token_arena).start;
            let end_pos = get_expr_range(*value_ref, pool, token_arena).end;
            Range { start: start_pos, end: end_pos }
        }
        Expr::If(branches) => {
            // token_id is for 'if' keyword.
            // Range from the 'if' keyword to the end of the last expression in the last branch.
            let start_pos = get_token_range(*token_id, token_arena).start;
            
            let mut current_end_pos = get_token_range(*token_id, token_arena).end; // Default if no branches

            for (condition_opt, body_expr_ref) in branches.iter() {
                 if let Some(condition_expr_ref) = condition_opt {
                    // Potentially extend based on condition, but mostly body drives the end
                 }
                 current_end_pos = get_expr_range(*body_expr_ref, pool, token_arena).end;
            }
            Range { start: start_pos, end: current_end_pos }
        }
        Expr::Literal(_)
        | Expr::Ident(_) // This token_id is the Ident's own token
        | Expr::Selector(_) // This token_id is the Selector's own token
        | Expr::Include(_) // This token_id is the Include keyword's token. Literal has its own.
        | Expr::InterpolatedString(_) // Spans its own tokens, token_id is for the start of string
        | Expr::Nodes // Represents a placeholder, range might be tricky. Use its own token.
        | Expr::Self_ => { // This token_id is for 'self' keyword
            // For these, the range is simply the range of their associated token.
            get_token_range(*token_id, token_arena)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::node::{Expr, Literal, Ident}; // Removed StringSegment, Selector as not used in current tests
    // use crate::ast::pool::ExprPool; // Already in scope via super::*
    // use crate::ast::expr_ref::ExprRef; // Already in scope via super::*
    use crate::arena::{Arena, ArenaId}; // ArenaId needed for dummy_module_id
    use crate::lexer::token::{Token, TokenKind};
    use crate::range::{Position, Range};
    // use crate::TokenId; // Already in scope via super::*
    use std::rc::Rc;
    // use smallvec::smallvec; // Not used in current tests

    // Helper to create a token and add to arena
    fn add_token(arena: &mut Arena<Rc<Token>>, kind: TokenKind, start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> TokenId {
        let token = Token {
            kind,
            range: Range::new(Position::new(start_line, start_col), Position::new(end_line, end_col)),
            module_id: ArenaId::first(), // Dummy module_id for tests
        };
        arena.alloc(Rc::new(token))
    }

    #[test]
    fn test_get_expr_range_literal() {
        let mut pool = ExprPool::new();
        let mut token_arena = Arena::new(10);

        let lit_token_id = add_token(&mut token_arena, TokenKind::Number, 1, 0, 1, 5); // Range for "12345"
        let lit_expr = Expr::Literal(Literal::Number(12345.into()));
        let lit_expr_ref = pool.add(lit_expr, lit_token_id);

        let range = get_expr_range(lit_expr_ref, &pool, &token_arena);
        assert_eq!(range, Range::new(Position::new(1,0), Position::new(1,5)));
    }

    #[test]
    fn test_get_expr_range_let() {
        let mut pool = ExprPool::new();
        let mut token_arena = Arena::new(10);

        let let_token_id = add_token(&mut token_arena, TokenKind::Let, 1, 0, 1, 3); // "let"
        // Token for 'x' (Ident) - its TokenId would be stored with the Ident Expr if it were a standalone expression.
        // For Expr::Let, the Ident is stored directly. Its range is part of the "let" keyword's idea or not covered by get_expr_range directly on Let.
        // The range of the Ident itself is not directly used by get_expr_range for the Let *statement* but would be if x was an Expr::Ident.
        add_token(&mut token_arena, TokenKind::Ident, 1, 4, 1, 5); // "x" - this token isn't directly linked to an ExprRef in this specific test structure for Let's Ident.
        add_token(&mut token_arena, TokenKind::Assign, 1, 6, 1, 7); // "="
        let val_token_id = add_token(&mut token_arena, TokenKind::Number, 1, 8, 1, 10); // "10"

        let val_expr = Expr::Literal(Literal::Number(10.into()));
        let val_expr_ref = pool.add(val_expr, val_token_id);
        
        // The Ident in Expr::Let is the struct itself, not an ExprRef to an Ident expression.
        // The token_id for the Expr::Let is let_token_id.
        let let_expr = Expr::Let(Ident::new("x"), val_expr_ref); // Ident::new("x") doesn't have its own TokenId stored in the pool here.
        let let_expr_ref = pool.add(let_expr, let_token_id);
        
        let range = get_expr_range(let_expr_ref, &pool, &token_arena);
        // Expected: starts at "let" (1,0), ends at "10" (1,10)
        assert_eq!(range, Range::new(Position::new(1,0), Position::new(1,10)));
    }

    // TODO: Add more tests for other Expr variants: Call, If, Def, Fn, While, etc.
}
