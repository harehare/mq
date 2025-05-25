// No Rc needed for Program type alias as it's Vec<NodeId>
use crate::{ast::IdentName, lexer::token::Token, arena::Arena as TokenArena};
use typed_arena::Arena as TypedArena;

use super::ast::node::{self as ast, AstArena, NodeId, NodeData, Expr, Args, Program, Ident, Literal}; // Explicitly import all used types

#[derive(Debug, Default)]
pub struct Optimizer {
    constant_table: FxHashMap<ast::Ident, ast::NodeId>, // Stores NodeId of the Literal node
}

impl Optimizer {
    pub fn new() -> Self {
        Self {
            constant_table: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
        }
    }

    #[inline]
    fn alloc_node<'ast_alloc>(
        &self,
        node_data: ast::NodeData<'ast_alloc>,
        ast_arena: &'ast_alloc AstArena<'ast_alloc>,
    ) -> ast::NodeId {
        ast::NodeId(ast_arena.alloc(node_data) as *const _ as usize)
    }
    

    fn collect_used_identifiers_in_node<'ast>(
        &self,
        node_id: ast::NodeId,
        ast_arena: &'ast AstArena<'ast>,
        used_idents: &mut FxHashSet<IdentName>,
    ) {
        let node = &ast_arena[node_id]; // Get reference to NodeData
        match &node.expr {
            ast::Expr::Ident(ident) => {
                used_idents.insert(ident.name.clone());
            }
            ast::Expr::Call(func_ident, args_ids, _) => {
                used_idents.insert(func_ident.name.clone());
                for arg_id in args_ids {
                    self.collect_used_identifiers_in_node(*arg_id, ast_arena, used_idents);
                }
            }
            ast::Expr::Let(_ident, value_node_id) => {
                self.collect_used_identifiers_in_node(*value_node_id, ast_arena, used_idents);
            }
            ast::Expr::Def(_ident, _params, program_node_ids) => {
                for stmt_id in program_node_ids {
                    self.collect_used_identifiers_in_node(*stmt_id, ast_arena, used_idents);
                }
            }
            ast::Expr::Fn(_params, program_node_ids) => {
                for stmt_id in program_node_ids {
                    self.collect_used_identifiers_in_node(*stmt_id, ast_arena, used_idents);
                }
            }
            ast::Expr::If(conditions) => {
                for (cond_node_id_opt, body_node_id) in conditions {
                    if let Some(cond_id) = cond_node_id_opt {
                        self.collect_used_identifiers_in_node(*cond_id, ast_arena, used_idents);
                    }
                    self.collect_used_identifiers_in_node(*body_node_id, ast_arena, used_idents);
                }
            }
            ast::Expr::While(cond_node_id, program_node_ids)
            | ast::Expr::Until(cond_node_id, program_node_ids) => {
                self.collect_used_identifiers_in_node(*cond_node_id, ast_arena, used_idents);
                for stmt_id in program_node_ids {
                    self.collect_used_identifiers_in_node(*stmt_id, ast_arena, used_idents);
                }
            }
            ast::Expr::Foreach(_item_ident, collection_node_id, program_node_ids) => {
                self.collect_used_identifiers_in_node(*collection_node_id, ast_arena, used_idents);
                for stmt_id in program_node_ids {
                    self.collect_used_identifiers_in_node(*stmt_id, ast_arena, used_idents);
                }
            }
            ast::Expr::InterpolatedString(segments) => {
                for segment in segments {
                    if let ast::StringSegment::Ident(ident) = segment {
                        used_idents.insert(ident.name.clone());
                    }
                }
            }
            ast::Expr::Literal(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_
            | ast::Expr::Include(_) => {
                // No idents to collect from these directly.
            }
        }
    }

    fn collect_used_identifiers<'ast>(
        &self,
        program: &Vec<ast::NodeId>,
        ast_arena: &'ast AstArena<'ast>,
    ) -> FxHashSet<IdentName> {
        let mut used_idents = FxHashSet::default();
        for node_id in program {
            self.collect_used_identifiers_in_node(*node_id, ast_arena, &mut used_idents);
        }
        used_idents
    }

    pub fn optimize<'ast>(
        &mut self,
        program: &Vec<ast::NodeId>,
        ast_arena: &'ast AstArena<'ast>,
    ) -> Vec<ast::NodeId> {
        let used_identifiers = self.collect_used_identifiers(program, ast_arena);
        let mut optimized_program = Vec::new();

        for node_id_ref in program.iter() {
            let original_node_id = *node_id_ref;
            let node_data = &ast_arena[original_node_id]; // Initial lookup

            match &node_data.expr {
                ast::Expr::Let(ident, value_node_id) => {
                    if used_identifiers.contains(&ident.name) {
                        // If used, optimize the Let node itself.
                        // This will handle optimizing the value and potentially updating constant_table.
                        let optimized_let_node_id = self.optimize_node(original_node_id, ast_arena);
                        optimized_program.push(optimized_let_node_id);
                    } else {
                        // If not used, remove from constant_table if it was a const.
                        // The Let node is not added to optimized_program, effectively removing it.
                        // Check if this ident was a constant. The value in constant_table is NodeId of the literal.
                        // We need to check if the *value* of this specific Let was that literal.
                        if let Some(const_node_id) = self.constant_table.get(ident) {
                             if *const_node_id == *value_node_id || 
                                (ast_arena[*const_node_id].expr == ast_arena[*value_node_id].expr && 
                                 matches!(ast_arena[*value_node_id].expr, ast::Expr::Literal(_))) {
                                // This specific Let binding was making 'ident' a constant.
                                // Since 'ident' is unused, remove it from the table.
                                self.constant_table.remove(ident);
                            }
                        }
                    }
                }
                _ => {
                    // For all other node types, optimize them as usual.
                    optimized_program.push(self.optimize_node(original_node_id, ast_arena));
                }
            }
        }
        optimized_program
    }

    fn optimize_node<'ast>(
        &mut self,
        node_id: ast::NodeId,
        ast_arena: &'ast AstArena<'ast>,
    ) -> ast::NodeId {
        let original_node_data = &ast_arena[node_id];
        let original_token_id = original_node_data.token_id; 

        match &original_node_data.expr {
            ast::Expr::Call(ident, args_ids, optional) => {
                let mut optimized_args_changed = false;
                let optimized_args_ids: ast::Args<'ast> = args_ids
                    .iter()
                    .map(|arg_id| {
                        let optimized_arg_id = self.optimize_node(*arg_id, ast_arena);
                        if optimized_arg_id != *arg_id {
                            optimized_args_changed = true;
                        }
                        optimized_arg_id
                    })
                    .collect::<SmallVec<_>>();

                if optimized_args_ids.len() == 2 {
                    let arg1_data = &ast_arena[optimized_args_ids[0]];
                    let arg2_data = &ast_arena[optimized_args_ids[1]];

                    if let (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) = (&arg1_data.expr, &arg2_data.expr) {
                        let new_expr_opt = match ident.name.as_str() {
                            "add" => Some(ast::Expr::Literal(ast::Literal::Number(*a + *b))),
                            "sub" => Some(ast::Expr::Literal(ast::Literal::Number(*a - *b))),
                            "mul" => Some(ast::Expr::Literal(ast::Literal::Number(*a * *b))),
                            "div" => if b.value() != 0.0 { Some(ast::Expr::Literal(ast::Literal::Number(*a / *b))) } else { None },
                            "mod" => if b.value() != 0.0 { Some(ast::Expr::Literal(ast::Literal::Number(*a % *b))) } else { None },
                            _ => None,
                        };
                        if let Some(new_expr) = new_expr_opt {
                            return self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena);
                        }
                    } else if let (ast::Expr::Literal(ast::Literal::String(a)), ast::Expr::Literal(ast::Literal::String(b))) = (&arg1_data.expr, &arg2_data.expr) {
                         if ident.name.as_str() == "add" {
                            let new_expr = ast::Expr::Literal(ast::Literal::String(format!("{}{}", a, b)));
                            return self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena);
                         }
                    }
                }

                if optimized_args_changed {
                    let new_expr = ast::Expr::Call(ident.clone(), optimized_args_ids, *optional);
                    self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena)
                } else {
                    node_id 
                }
            }
            ast::Expr::Ident(ident) => {
                if let Some(const_literal_node_id) = self.constant_table.get(ident) {
                    *const_literal_node_id
                } else {
                    node_id 
                }
            }
            ast::Expr::Let(ident, value_node_id) => {
                let optimized_value_node_id = self.optimize_node(*value_node_id, ast_arena);
                
                let optimized_value_node_data = &ast_arena[optimized_value_node_id];
                if let ast::Expr::Literal(_) = &optimized_value_node_data.expr {
                    self.constant_table.insert(ident.clone(), optimized_value_node_id);
                } else {
                    // If it was constant but the value changed to non-literal, remove from table.
                    // This handles cases where a constant might be part of an expression that doesn't fold to a literal.
                    self.constant_table.remove(ident); 
                }

                if optimized_value_node_id != *value_node_id {
                    let new_expr = ast::Expr::Let(ident.clone(), optimized_value_node_id);
                    self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena)
                } else {
                    // If value didn't change, but it became a constant (first time seeing it),
                    // it's already added to constant_table. No need to reallocate the Let node.
                    node_id 
                }
            }
            ast::Expr::If(conditions) => {
                let mut optimized_conditions_changed = false;
                let new_conditions: ast::Branches<'ast> = conditions
                    .iter()
                    .map(|(cond_opt_id, body_id)| {
                        let new_cond_opt_id = cond_opt_id.map(|cond_id| {
                            let optimized_cond_id = self.optimize_node(cond_id, ast_arena);
                            if optimized_cond_id != cond_id { optimized_conditions_changed = true; }
                            optimized_cond_id
                        });
                        let new_body_id = self.optimize_node(*body_id, ast_arena);
                        if new_body_id != *body_id { optimized_conditions_changed = true; }
                        (new_cond_opt_id, new_body_id)
                    })
                    .collect();
                if optimized_conditions_changed {
                    let new_expr = ast::Expr::If(new_conditions);
                    self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena)
                } else {
                    node_id
                }
            }
            ast::Expr::Def(ident, params, program_ids) => {
                let mut prog_changed = false;
                let new_program_ids: Vec<ast::NodeId> = program_ids.iter().map(|id| {
                    let new_id = self.optimize_node(*id, ast_arena);
                    if new_id != *id { prog_changed = true; }
                    new_id
                }).collect();
                if prog_changed {
                    let new_expr = ast::Expr::Def(ident.clone(), params.clone(), new_program_ids);
                    self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena)
                } else { node_id }
            }
            ast::Expr::Fn(params, program_ids) => {
                let mut prog_changed = false;
                let new_program_ids: Vec<ast::NodeId> = program_ids.iter().map(|id| {
                    let new_id = self.optimize_node(*id, ast_arena);
                    if new_id != *id { prog_changed = true; }
                    new_id
                }).collect();
                if prog_changed {
                    let new_expr = ast::Expr::Fn(params.clone(), new_program_ids);
                    self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena)
                } else { node_id }
            }
             ast::Expr::While(cond_id, program_ids) => {
                let mut changed = false;
                let new_cond_id = self.optimize_node(*cond_id, ast_arena);
                if new_cond_id != *cond_id { changed = true; }
                let new_program_ids: Vec<ast::NodeId> = program_ids.iter().map(|id| {
                    let new_id = self.optimize_node(*id, ast_arena);
                    if new_id != *id { changed = true; }
                    new_id
                }).collect();
                if changed {
                    let new_expr = ast::Expr::While(new_cond_id, new_program_ids);
                    self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena)
                } else { node_id }
            }
            ast::Expr::Until(cond_id, program_ids) => {
                let mut changed = false;
                let new_cond_id = self.optimize_node(*cond_id, ast_arena);
                if new_cond_id != *cond_id { changed = true; }
                let new_program_ids: Vec<ast::NodeId> = program_ids.iter().map(|id| {
                    let new_id = self.optimize_node(*id, ast_arena);
                    if new_id != *id { changed = true; }
                    new_id
                }).collect();
                if changed {
                    let new_expr = ast::Expr::Until(new_cond_id, new_program_ids);
                    self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena)
                } else { node_id }
            }
            ast::Expr::Foreach(item_ident, coll_id, program_ids) => {
                let mut changed = false;
                let new_coll_id = self.optimize_node(*coll_id, ast_arena);
                if new_coll_id != *coll_id { changed = true; }
                let new_program_ids: Vec<ast::NodeId> = program_ids.iter().map(|id| {
                    let new_id = self.optimize_node(*id, ast_arena);
                    if new_id != *id { changed = true; }
                    new_id
                }).collect();
                if changed {
                    let new_expr = ast::Expr::Foreach(item_ident.clone(), new_coll_id, new_program_ids);
                    self.alloc_node(ast::NodeData { token_id: original_token_id, expr: new_expr }, ast_arena)
                } else { node_id }
            }
            ast::Expr::InterpolatedString(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Include(_)
            | ast::Expr::Literal(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_ => node_id, 
        }
    }
}

#[cfg(test)]
// #[ignore] // Removing ignore to enable tests
mod tests {
    use super::*;
    use crate::{
        ast::node::{Expr as AstExpr, Ident, Literal, NodeData, NodeId, AstProgram}, // Use specific types
        arena::ArenaId, // For dummy TokenId
        number::Number, // For Literal::Number
        lexer::token::Token, // For creating dummy Rc<Token>
    };
    use rstest::rstest;
    use smallvec::smallvec;
    use typed_arena::Arena as TypedArena;
    use std::rc::Rc; // For Rc<Token>

    // Helper to allocate a NodeData into the AstArena for tests
    fn alloc_node_test<'node_arena, 'ast_lifetime>(
        ast_arena: &'node_arena TypedArena<NodeData<'ast_lifetime>>,
        token_id: ArenaId<Rc<Token>>, // Using ArenaId<Rc<Token>> as TokenId
        expr: ast::Expr<'ast_lifetime>,
    ) -> ast::NodeId
    where
        'node_arena: 'ast_lifetime,
    {
        ast::NodeId(ast_arena.alloc(ast::NodeData { token_id, expr }) as *const _ as usize)
    }

    // Unsafe helper to get NodeData from NodeId
    unsafe fn get_node_data_test<'a>(node_id: ast::NodeId, arena: &'a AstArena<'a>) -> &'a NodeData<'a> {
        &*(node_id.0 as *const NodeData<'a>)
    }
    
    // Dummy TokenId for test nodes that don't have a specific token source
    fn dummy_token_id() -> ArenaId<Rc<Token>> {
        ArenaId::new(0) // Assuming TokenId 0 is a valid dummy/default
    }

    // Note: Deep comparison of ASTs is complex. These tests will focus on:
    // 1. Correctness of node count for DCE.
    // 2. Correctness of the top-level expression for constant folding/propagation.
    // 3. For constant propagation/DCE, correctness of the constant_table.

    #[rstest]
    #[case::constant_folding_add_numbers()]
    fn test_constant_folding_add_numbers(
        #[values(true, false)] optimize_flag: bool, // Test with and without optimization globally (though this test is specific to it)
    ) {
        let ast_arena = TypedArena::new();
        let mut optimizer = Optimizer::new();
        optimizer.options.optimize = optimize_flag; // Not used by Optimizer struct directly, but good for consistency if Engine passes it

        let tok_id = dummy_token_id();

        let arg1_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Literal(ast::Literal::Number(2.0.into())));
        let arg2_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Literal(ast::Literal::Number(3.0.into())));
        
        let call_expr = ast::Expr::Call(
            ast::Ident::new("add"),
            smallvec![arg1_id, arg2_id],
            false,
        );
        let call_node_id = alloc_node_test(&ast_arena, tok_id, call_expr);
        let program = vec![call_node_id];

        let optimized_program = optimizer.optimize(&program, &ast_arena);
        
        assert_eq!(optimized_program.len(), 1);
        let result_node_data = unsafe { get_node_data_test(optimized_program[0], &ast_arena) };
        assert_eq!(result_node_data.expr, ast::Expr::Literal(ast::Literal::Number(5.0.into())));
    }

    #[rstest]
    #[case::constant_folding_add_strings()]
    fn test_constant_folding_add_strings() {
        let ast_arena = TypedArena::new();
        let mut optimizer = Optimizer::new();
        let tok_id = dummy_token_id();

        let arg1_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Literal(ast::Literal::String("hello".to_string())));
        let arg2_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Literal(ast::Literal::String("world".to_string())));
        
        let call_expr = ast::Expr::Call(
            ast::Ident::new("add"),
            smallvec![arg1_id, arg2_id],
            false,
        );
        let call_node_id = alloc_node_test(&ast_arena, tok_id, call_expr);
        let program = vec![call_node_id];

        let optimized_program = optimizer.optimize(&program, &ast_arena);
        
        assert_eq!(optimized_program.len(), 1);
        let result_node_data = unsafe { get_node_data_test(optimized_program[0], &ast_arena) };
        assert_eq!(result_node_data.expr, ast::Expr::Literal(ast::Literal::String("helloworld".to_string())));
    }


    #[rstest]
    #[case::constant_propagation()]
    fn test_constant_propagation() {
        let ast_arena = TypedArena::new();
        let mut optimizer = Optimizer::new();
        let tok_id = dummy_token_id();
        let tok_id_ident_use = dummy_token_id(); // Potentially different token for usage

        let literal_val_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Literal(ast::Literal::Number(5.0.into())));
        let ident_x = ast::Ident::new("x");
        
        let let_node_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Let(ident_x.clone(), literal_val_id));
        let ident_use_node_id = alloc_node_test(&ast_arena, tok_id_ident_use, ast::Expr::Ident(ident_x.clone()));
        
        let program = vec![let_node_id, ident_use_node_id];
        let optimized_program = optimizer.optimize(&program, &ast_arena);

        assert_eq!(optimized_program.len(), 2); // Let node + (now) Literal node

        // Check that the Let node is still there (or could be if not DCE'd and value changed)
        let opt_let_node_data = unsafe { get_node_data_test(optimized_program[0], &ast_arena) };
        if let ast::Expr::Let(let_ident, let_val_id) = &opt_let_node_data.expr {
            assert_eq!(let_ident.name, ident_x.name);
            let let_val_data = unsafe { get_node_data_test(*let_val_id, &ast_arena) };
            assert_eq!(let_val_data.expr, ast::Expr::Literal(ast::Literal::Number(5.0.into())));
        } else {
            panic!("Expected first node to be Let, found {:?}", opt_let_node_data.expr);
        }
        
        // Check that the Ident use was replaced by the Literal's NodeId (which points to the literal)
        // In this setup, optimized_program[1] should be the NodeId of the literal itself.
        let result_node_data = unsafe { get_node_data_test(optimized_program[1], &ast_arena) };
        assert_eq!(result_node_data.expr, ast::Expr::Literal(ast::Literal::Number(5.0.into())));
        assert_eq!(optimized_program[1], literal_val_id); // Important: It should be the ID of the original literal node
        assert!(optimizer.constant_table.contains_key(&ident_x));
        assert_eq!(optimizer.constant_table.get(&ident_x), Some(&literal_val_id));
    }

    #[rstest]
    #[case::dead_code_elimination_simple_unused()]
    fn test_dce_simple_unused() {
        let ast_arena = TypedArena::new();
        let mut optimizer = Optimizer::new();
        let tok_id = dummy_token_id();

        let ident_unused = ast::Ident::new("unused_var");
        let val_unused_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Literal(Literal::Number(10.0.into())));
        let let_unused_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Let(ident_unused.clone(), val_unused_id));

        let ident_used = ast::Ident::new("used_var");
        let val_used_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Literal(Literal::Number(20.0.into())));
        let let_used_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Let(ident_used.clone(), val_used_id));
        
        let ident_used_ref_id = alloc_node_test(&ast_arena, tok_id, ast::Expr::Ident(ident_used.clone()));

        let program = vec![let_unused_id, let_used_id, ident_used_ref_id];
        let optimized_program = optimizer.optimize(&program, &ast_arena);
        
        assert_eq!(optimized_program.len(), 2); // unused_var Let node removed

        // Check that used_var Let node is present and its value is the literal
        let opt_let_node_data = unsafe { get_node_data_test(optimized_program[0], &ast_arena) };
         if let ast::Expr::Let(let_ident, let_val_id) = &opt_let_node_data.expr {
            assert_eq!(let_ident.name, ident_used.name);
            let let_val_data = unsafe { get_node_data_test(*let_val_id, &ast_arena) };
            assert_eq!(let_val_data.expr, ast::Expr::Literal(ast::Literal::Number(20.0.into())));
        } else {
            panic!("Expected first node to be Let(used_var), found {:?}", opt_let_node_data.expr);
        }

        // Check that ident_used_ref was replaced by the literal's NodeId
        let result_node_data = unsafe { get_node_data_test(optimized_program[1], &ast_arena) };
        assert_eq!(result_node_data.expr, ast::Expr::Literal(ast::Literal::Number(20.0.into())));
        assert_eq!(optimized_program[1], val_used_id);


        assert!(!optimizer.constant_table.contains_key(&ident_unused), "unused_var should be removed from constant_table");
        assert!(optimizer.constant_table.contains_key(&ident_used));
    }
}
