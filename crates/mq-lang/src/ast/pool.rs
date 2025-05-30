// crates/mq-lang/src/ast/pool.rs
use crate::ast::expr_ref::ExprRef;
use crate::ast::node::Expr; // Will be defined in node.rs
use crate::TokenId; // Defined in ast.rs, accessible via crate root

pub struct ExprPool {
    nodes: Vec<(Expr, TokenId)>,
    // Potentially add Arena<Rc<Token>> here if TokenId refers to tokens stored alongside
}

impl ExprPool {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add(&mut self, expr: Expr, token_id: TokenId) -> ExprRef {
        let next_id = self.nodes.len() as u32;
        self.nodes.push((expr, token_id));
        ExprRef(next_id)
    }

    pub fn get(&self, expr_ref: ExprRef) -> Option<&(Expr, TokenId)> {
        self.nodes.get(expr_ref.0 as usize)
    }
    
    pub fn get_expr(&self, expr_ref: ExprRef) -> Option<&Expr> {
        self.nodes.get(expr_ref.0 as usize).map(|(expr, _)| expr)
    }

    pub fn get_token_id(&self, expr_ref: ExprRef) -> Option<TokenId> {
        self.nodes.get(expr_ref.0 as usize).map(|(_, token_id)| *token_id)
    }
}

impl Default for ExprPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::node::{Expr, Literal, Ident};
    use crate::TokenId; 
    use crate::arena::ArenaId; // For simple_dummy_token_id

    // A simplified dummy TokenId for pool tests if we don't involve a real Token Arena here
    fn simple_dummy_token_id() -> TokenId {
        ArenaId::first() // Placeholder, assuming TokenId is ArenaId<something>
    }

    #[test]
    fn test_expr_pool_add_get() {
        let mut pool = ExprPool::new();
        let token_id = simple_dummy_token_id();

        let expr1_data = Expr::Literal(Literal::Number(123.into()));
        let expr1_ref = pool.add(expr1_data.clone(), token_id);

        let expr2_data = Expr::Ident(Ident::new("x")); // Ident::new needs a &str
        let expr2_ref = pool.add(expr2_data.clone(), token_id);

        assert_eq!(expr1_ref, ExprRef(0));
        assert_eq!(expr2_ref, ExprRef(1));

        let retrieved_expr1 = pool.get_expr(expr1_ref).unwrap();
        assert_eq!(retrieved_expr1, &expr1_data);

        let retrieved_expr2 = pool.get_expr(expr2_ref).unwrap();
        assert_eq!(retrieved_expr2, &expr2_data);

        let retrieved_token_id1 = pool.get_token_id(expr1_ref).unwrap();
        assert_eq!(retrieved_token_id1, token_id);
    }

    #[test]
    fn test_expr_pool_get_out_of_bounds() {
        let pool = ExprPool::new();
        assert!(pool.get(ExprRef(0)).is_none());
        assert!(pool.get_expr(ExprRef(0)).is_none());
    }
}
