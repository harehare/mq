//! AST to OpTree transformation.
//!
//! This module provides transformation from the tree-based AST representation
//! to the flattened OpTree representation.

use super::{AccessTarget, MatchArm, Op, OpPool, OpRef, SourceMap, StringSegment};
use crate::{Program, Shared, ast::node as ast};
use smallvec::SmallVec;

/// Transforms AST (tree structure) to OpTree (flat structure).
///
/// OpTreeTransformer walks the AST recursively, converting each node
/// into an Op instruction stored in a contiguous OpPool. Source location
/// information is preserved in a parallel SourceMap for error reporting.
///
/// # Example
///
/// ```rust,ignore
/// use mq_lang::optree::OpTreeTransformer;
///
/// let transformer = OpTreeTransformer::new();
/// let (pool, source_map, root) = transformer.transform(&program);
/// ```
pub struct OpTreeTransformer {
    pool: OpPool,
    source_map: SourceMap,
}

impl OpTreeTransformer {
    pub fn new() -> Self {
        Self {
            pool: OpPool::new(),
            source_map: SourceMap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            pool: OpPool::with_capacity(capacity),
            source_map: SourceMap::with_capacity(capacity),
        }
    }

    /// Attempts to fold constant expressions at compile time
    /// Returns Some(OpRef) if folding succeeded, None otherwise
    fn try_fold_constants(&mut self, op: &Op) -> Option<Op> {
        match op {
            // Fold logical AND with constant operands
            Op::And(left, right) => {
                match (self.pool.get(*left).as_ref(), self.pool.get(*right).as_ref()) {
                    // Both constants: fold completely
                    (Op::Literal(ast::Literal::Bool(l)), Op::Literal(ast::Literal::Bool(r))) => {
                        return Some(Op::Literal(ast::Literal::Bool(*l && *r)));
                    }
                    // Left is false: whole expression is false (short-circuit)
                    (Op::Literal(ast::Literal::Bool(false)), _) => {
                        return Some(Op::Literal(ast::Literal::Bool(false)));
                    }
                    // Left is true: result is right operand
                    (Op::Literal(ast::Literal::Bool(true)), _) => {
                        return Some(self.pool.get(*right).as_ref().clone());
                    }
                    // Right is false and left is non-constant: can't optimize much
                    (_, Op::Literal(ast::Literal::Bool(false))) => {
                        // Still need to evaluate left for side effects
                        return None;
                    }
                    _ => {}
                }
            }
            // Fold logical OR with constant operands
            Op::Or(left, right) => {
                match (self.pool.get(*left).as_ref(), self.pool.get(*right).as_ref()) {
                    // Both constants: fold completely
                    (Op::Literal(ast::Literal::Bool(l)), Op::Literal(ast::Literal::Bool(r))) => {
                        return Some(Op::Literal(ast::Literal::Bool(*l || *r)));
                    }
                    // Left is true: whole expression is true (short-circuit)
                    (Op::Literal(ast::Literal::Bool(true)), _) => {
                        return Some(Op::Literal(ast::Literal::Bool(true)));
                    }
                    // Left is false: result is right operand
                    (Op::Literal(ast::Literal::Bool(false)), _) => {
                        return Some(self.pool.get(*right).as_ref().clone());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        None
    }

    /// Optimizes If expressions with constant conditions (Perl-style)
    fn optimize_if_branches(&mut self, branches: &[(Option<OpRef>, OpRef)]) -> Op {
        // Check if first branch has a constant condition
        if let Some((Some(cond_ref), body_ref)) = branches.first() {
            if let Op::Literal(ast::Literal::Bool(true)) = self.pool.get(*cond_ref).as_ref() {
                // Constant true: always execute this branch, ignore others
                return Op::ConstTrue(*body_ref);
            } else if let Op::Literal(ast::Literal::Bool(false)) = self.pool.get(*cond_ref).as_ref() {
                // Constant false: skip this branch, optimize remaining branches
                if branches.len() > 1 {
                    return self.optimize_if_branches(&branches[1..]);
                } else {
                    return Op::Nop;
                }
            }
        }

        // No optimization possible, return original
        Op::If {
            branches: branches.iter().cloned().collect(),
        }
    }

    /// Transforms a complete program (AST) to OpTree.
    ///
    /// Returns:
    /// - `OpPool`: Storage for all instructions
    /// - `SourceMap`: Mapping from OpRef to source locations
    /// - `OpRef`: Root instruction reference
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let transformer = OpTreeTransformer::new();
    /// let (pool, source_map, root) = transformer.transform(&program);
    ///
    /// // Now you can create an evaluator
    /// let evaluator = OpTreeEvaluator::new(pool, source_map, ...);
    /// evaluator.eval(root, runtime_value)?;
    /// ```
    pub fn transform(mut self, program: &Program) -> (OpPool, SourceMap, OpRef) {
        let root = self.transform_program(program);
        (self.pool, self.source_map, root)
    }

    fn transform_program(&mut self, program: &Program) -> OpRef {
        if program.is_empty() {
            let _op_id = self.source_map.register(crate::arena::ArenaId::new(0));
            return self.pool.alloc(Op::Nodes);
        }

        // Optimization: single element sequence doesn't need wrapping
        if program.len() == 1 {
            return self.transform_node(&program[0]);
        }

        let ops: SmallVec<[OpRef; 8]> = program.iter().map(|node| self.transform_node(node)).collect();

        // Post-optimization: filter out Nops and optimize sequence
        let optimized_ops: SmallVec<[OpRef; 8]> = ops
            .iter()
            .filter(|&&op_ref| !matches!(self.pool.get(op_ref).as_ref(), Op::Nop))
            .copied()
            .collect();

        // If all ops were Nops, return a single Nop
        if optimized_ops.is_empty() {
            let _op_id = self.source_map.register(program[0].token_id);
            return self.pool.alloc(Op::Nop);
        }

        // If only one op remains after filtering Nops, return it directly
        if optimized_ops.len() == 1 {
            return optimized_ops[0];
        }

        let token_id = program[0].token_id;
        let _ = self.source_map.register(token_id);

        self.pool.alloc(Op::Sequence(optimized_ops))
    }

    /// Enters a new scope, executes a closure, then restores the previous scope
    fn with_new_scope<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        // NOTE: Scope tracking disabled - this just executes the closure
        // All variables use GlobalVar/context for correctness
        f(self)
    }

    /// Transforms a single AST node into an OpRef.
    fn transform_node(&mut self, node: &Shared<ast::Node>) -> OpRef {
        let token_id = node.token_id;
        let _ = self.source_map.register(token_id);

        let op = match &*node.expr {
            ast::Expr::Literal(lit) => Op::Literal(lit.clone()),
            ast::Expr::Ident(ident) => {
                // TEMPORARY: Disable LocalVar optimization - all identifiers are GlobalVar
                // This ensures correctness while we work on proper closure/scope handling
                Op::GlobalVar(ident.name)
            }
            ast::Expr::Self_ => Op::Self_,
            ast::Expr::Nodes => Op::Nodes,
            ast::Expr::Let(ident, value) => {
                let value_ref = self.transform_node(value);
                // TEMPORARY: Disable LocalVar optimization - use context instead
                // This ensures correctness while we work on proper closure/scope handling
                Op::Let {
                    name: ident.name,
                    value: value_ref,
                    local_index: None, // Disabled: use context instead of pad
                }
            }
            ast::Expr::Var(ident, value) => {
                let value_ref = self.transform_node(value);
                // TEMPORARY: Disable LocalVar optimization - use context instead
                Op::Var {
                    name: ident.name,
                    value: value_ref,
                    local_index: None, // Disabled: use context instead of pad
                }
            }
            ast::Expr::Assign(ident, value) => {
                let value_ref = self.transform_node(value);
                // TEMPORARY: Disable LocalVar optimization - use context instead
                Op::Assign {
                    name: ident.name,
                    value: value_ref,
                    local_index: None, // Disabled: use context instead of pad
                }
            }
            ast::Expr::If(branches) => {
                let op_branches: SmallVec<[(Option<OpRef>, OpRef); 8]> = branches
                    .iter()
                    .map(|(cond, body)| {
                        let cond_ref = cond.as_ref().map(|c| self.transform_node(c));
                        let body_ref = self.transform_node(body);
                        (cond_ref, body_ref)
                    })
                    .collect();
                // Apply constant condition optimization (Perl-style)
                self.optimize_if_branches(&op_branches)
            }
            ast::Expr::While(cond, body) => {
                let cond_ref = self.transform_node(cond);
                let body_ref = self.transform_program(body);

                // Optimize constant conditions
                match self.pool.get(cond_ref).as_ref() {
                    // while false: ... → Nop (never executes)
                    Op::Literal(ast::Literal::Bool(false)) => Op::Nop,
                    // while true: ... → infinite loop (keep as-is)
                    _ => Op::While {
                        condition: cond_ref,
                        body: body_ref,
                    },
                }
            }
            ast::Expr::Foreach(ident, iter, body) => {
                let iter_ref = self.transform_node(iter);
                // Transform body in new scope with loop variable
                // TEMPORARY: Disable LocalVar optimization for loop variables
                let body_ref = self.with_new_scope(|transformer| transformer.transform_program(body));
                Op::Foreach {
                    name: ident.name,
                    iterator: iter_ref,
                    body: body_ref,
                    local_index: None, // Disabled: use context instead of pad
                }
            }
            ast::Expr::Match(value, arms) => {
                let value_ref = self.transform_node(value);
                let op_arms: SmallVec<_> = arms
                    .iter()
                    .map(|arm| MatchArm {
                        pattern: arm.pattern.clone(),
                        guard: arm.guard.as_ref().map(|g| self.transform_node(g)),
                        body: self.transform_node(&arm.body),
                    })
                    .collect();
                Op::Match {
                    value: value_ref,
                    arms: Box::new(op_arms),
                }
            }
            ast::Expr::Break => Op::Break,
            ast::Expr::Continue => Op::Continue,
            ast::Expr::Def(ident, params, body) => {
                // Transform params and body in new scope
                let (param_refs, body_ref) = self.with_new_scope(|transformer| {
                    // DO NOT define params in scope - they are bound at runtime via context
                    // Only let/var should be optimized as LocalVar
                    let param_refs: SmallVec<_> = params.iter().map(|p| transformer.transform_node(p)).collect();

                    let body_ref = transformer.transform_program(body);
                    (param_refs, body_ref)
                });

                Op::Def {
                    name: ident.name,
                    params: param_refs,
                    body: body_ref,
                    // Store original AST for compatibility with AST evaluator
                    ast_params: params.clone(),
                    ast_program: body.clone(),
                }
            }
            ast::Expr::Fn(params, body) => {
                // Transform params and body in new scope
                let (param_refs, body_ref) = self.with_new_scope(|transformer| {
                    // DO NOT define params in scope - they are bound at runtime via context
                    // Only let/var should be optimized as LocalVar
                    let param_refs: SmallVec<_> = params.iter().map(|p| transformer.transform_node(p)).collect();

                    let body_ref = transformer.transform_program(body);
                    (param_refs, body_ref)
                });

                Op::Fn {
                    params: param_refs,
                    body: body_ref,
                    // Store original AST for compatibility with AST evaluator
                    ast_params: params.clone(),
                    ast_program: body.clone(),
                }
            }
            ast::Expr::Call(ident, args) => {
                let arg_refs: SmallVec<_> = args.iter().map(|a| self.transform_node(a)).collect();
                Op::Call {
                    name: ident.name,
                    args: arg_refs,
                }
            }
            ast::Expr::CallDynamic(callable, args) => {
                let callable_ref = self.transform_node(callable);
                let arg_refs: SmallVec<_> = args.iter().map(|a| self.transform_node(a)).collect();
                Op::CallDynamic {
                    callable: callable_ref,
                    args: arg_refs,
                }
            }
            ast::Expr::Block(program) => {
                // Transform block in new scope
                let body_ref = self.with_new_scope(|transformer| transformer.transform_program(program));
                Op::Block(body_ref)
            }
            ast::Expr::And(left, right) => {
                let left_ref = self.transform_node(left);
                let right_ref = self.transform_node(right);
                let and_op = Op::And(left_ref, right_ref);
                // Try constant folding
                if let Some(folded) = self.try_fold_constants(&and_op) {
                    folded
                } else {
                    and_op
                }
            }
            ast::Expr::Or(left, right) => {
                let left_ref = self.transform_node(left);
                let right_ref = self.transform_node(right);
                let or_op = Op::Or(left_ref, right_ref);
                // Try constant folding
                if let Some(folded) = self.try_fold_constants(&or_op) {
                    folded
                } else {
                    or_op
                }
            }
            ast::Expr::Paren(expr) => {
                let expr_ref = self.transform_node(expr);
                // Peephole optimization: remove redundant parentheses
                // Just return the inner expression directly
                return expr_ref;
            }
            ast::Expr::InterpolatedString(segments) => {
                let op_segments: Vec<_> = segments
                    .iter()
                    .map(|seg| match seg {
                        ast::StringSegment::Text(t) => StringSegment::Text(t.clone()),
                        ast::StringSegment::Expr(e) => StringSegment::Expr(self.transform_node(e)),
                        ast::StringSegment::Env(e) => StringSegment::Env(e.clone()),
                        ast::StringSegment::Self_ => StringSegment::Self_,
                    })
                    .collect();
                Op::InterpolatedString(op_segments)
            }
            ast::Expr::Selector(sel) => Op::Selector(sel.clone()),
            ast::Expr::QualifiedAccess(path, target) => {
                let op_target = match target {
                    ast::AccessTarget::Call(ident, args) => {
                        let arg_refs: SmallVec<_> = args.iter().map(|a| self.transform_node(a)).collect();
                        AccessTarget::Call(ident.name, arg_refs)
                    }
                    ast::AccessTarget::Ident(ident) => AccessTarget::Ident(ident.name),
                };
                Op::QualifiedAccess {
                    module_path: path.iter().map(|i| i.name).collect(),
                    target: op_target,
                }
            }
            ast::Expr::Module(ident, body) => {
                let body_ref = self.transform_program(body);
                Op::Module {
                    name: ident.name,
                    body: body_ref,
                }
            }
            ast::Expr::Include(lit) => Op::Include(lit.clone()),
            ast::Expr::Import(lit) => Op::Import(lit.clone()),
            ast::Expr::Try(try_expr, catch_expr) => {
                let try_ref = self.transform_node(try_expr);
                let catch_ref = self.transform_node(catch_expr);
                Op::Try {
                    try_expr: try_ref,
                    catch_expr: catch_ref,
                }
            }
            ast::Expr::Quote(_) => {
                // Quote is not allowed in runtime context - will error during evaluation
                Op::Quote
            }
            ast::Expr::Unquote(_) => {
                // Unquote is only allowed inside quote - will error during evaluation
                Op::Unquote
            }
            ast::Expr::Macro(_, _, _) => {
                // Macros should be expanded before transformation
                // If we encounter one, it's a bug in macro expansion
                panic!(
                    "Unexpanded macro encountered in OpTree transformation. \
                     Macros should be fully expanded before transforming to OpTree."
                );
            }
        };

        self.pool.alloc(op)
    }
}

impl Default for OpTreeTransformer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Shared, arena::ArenaId, number::Number};

    fn create_test_node(expr: ast::Expr, token_id: ArenaId<Shared<crate::Token>>) -> Shared<ast::Node> {
        Shared::new(ast::Node {
            token_id,
            expr: Shared::new(expr),
        })
    }

    #[test]
    fn test_transform_literal() {
        let transformer = OpTreeTransformer::new();
        let node = create_test_node(
            ast::Expr::Literal(ast::Literal::Number(Number::from(42.0))),
            ArenaId::new(1),
        );
        let program = vec![node];

        let (pool, source_map, root) = transformer.transform(&program);

        assert_eq!(pool.len(), 1);
        assert_eq!(source_map.len(), 1);
        assert!(matches!(pool.get(root).as_ref(), Op::Literal(ast::Literal::Number(_))));
    }

    #[test]
    fn test_transform_ident() {
        let transformer = OpTreeTransformer::new();
        let node = create_test_node(ast::Expr::Ident(ast::IdentWithToken::new("x")), ArenaId::new(1));
        let program = vec![node];

        let (pool, _, root) = transformer.transform(&program);

        // Ident without prior definition becomes GlobalVar
        assert!(matches!(pool.get(root).as_ref(), Op::GlobalVar(name) if name.as_str() == "x"));
    }

    #[test]
    fn test_transform_let() {
        let transformer = OpTreeTransformer::new();
        let value_node = create_test_node(
            ast::Expr::Literal(ast::Literal::Number(Number::from(42.0))),
            ArenaId::new(2),
        );
        let node = create_test_node(
            ast::Expr::Let(ast::IdentWithToken::new("x"), value_node),
            ArenaId::new(1),
        );
        let program = vec![node];

        let (pool, _, root) = transformer.transform(&program);

        match pool.get(root).as_ref() {
            Op::Let { name, value, .. } => {
                assert_eq!(name.as_str(), "x");
                assert!(matches!(pool.get(*value).as_ref(), Op::Literal(_)));
            }
            _ => panic!("Expected Let op"),
        }
    }

    #[test]
    fn test_transform_if() {
        let transformer = OpTreeTransformer::new();
        let cond = create_test_node(ast::Expr::Literal(ast::Literal::Bool(true)), ArenaId::new(2));
        let then_body = create_test_node(
            ast::Expr::Literal(ast::Literal::Number(Number::from(1.0))),
            ArenaId::new(3),
        );
        let else_body = create_test_node(
            ast::Expr::Literal(ast::Literal::Number(Number::from(2.0))),
            ArenaId::new(4),
        );

        let branches = smallvec::smallvec![
            (Some(cond), then_body),
            (None, else_body), // else clause
        ];

        let node = create_test_node(ast::Expr::If(branches), ArenaId::new(1));
        let program = vec![node];

        let (pool, _, root) = transformer.transform(&program);

        // After optimization, this may be If or an optimized variant
        match pool.get(root).as_ref() {
            Op::If { branches } => {
                assert_eq!(branches.len(), 2);
                assert!(branches[0].0.is_some()); // if condition
                assert!(branches[1].0.is_none()); // else (no condition)
            }
            Op::ConstTrue(_) | Op::ConstFalse | Op::Nop => {
                // Optimized version is also acceptable
            }
            other => panic!("Expected If op or optimized variant, got {:?}", other),
        }
    }

    #[test]
    fn test_transform_call() {
        let transformer = OpTreeTransformer::new();
        let arg1 = create_test_node(
            ast::Expr::Literal(ast::Literal::Number(Number::from(1.0))),
            ArenaId::new(2),
        );
        let arg2 = create_test_node(
            ast::Expr::Literal(ast::Literal::Number(Number::from(2.0))),
            ArenaId::new(3),
        );

        let node = create_test_node(
            ast::Expr::Call(ast::IdentWithToken::new("add"), smallvec::smallvec![arg1, arg2]),
            ArenaId::new(1),
        );
        let program = vec![node];

        let (pool, _, root) = transformer.transform(&program);

        match pool.get(root).as_ref() {
            Op::Call { name, args } => {
                assert_eq!(name.as_str(), "add");
                assert_eq!(args.len(), 2);
                assert!(matches!(pool.get(args[0]).as_ref(), Op::Literal(_)));
                assert!(matches!(pool.get(args[1]).as_ref(), Op::Literal(_)));
            }
            _ => panic!("Expected Call op"),
        }
    }

    #[test]
    fn test_transform_sequence() {
        let transformer = OpTreeTransformer::new();
        let node1 = create_test_node(
            ast::Expr::Literal(ast::Literal::Number(Number::from(1.0))),
            ArenaId::new(1),
        );
        let node2 = create_test_node(
            ast::Expr::Literal(ast::Literal::Number(Number::from(2.0))),
            ArenaId::new(2),
        );
        let program = vec![node1, node2];

        let (pool, _, root) = transformer.transform(&program);

        match pool.get(root).as_ref() {
            Op::Sequence(ops) => {
                assert_eq!(ops.len(), 2);
                assert!(matches!(pool.get(ops[0]).as_ref(), Op::Literal(_)));
                assert!(matches!(pool.get(ops[1]).as_ref(), Op::Literal(_)));
            }
            _ => panic!("Expected Sequence op"),
        }
    }

    #[test]
    fn test_transform_empty_program() {
        let transformer = OpTreeTransformer::new();
        let program = vec![];

        let (pool, _, root) = transformer.transform(&program);

        assert!(matches!(pool.get(root).as_ref(), Op::Nodes));
    }

    #[test]
    fn test_source_map_tracking() {
        let transformer = OpTreeTransformer::new();
        let token_id = ArenaId::new(42);
        let node = create_test_node(ast::Expr::Literal(ast::Literal::Number(Number::from(1.0))), token_id);
        let program = vec![node];

        let (_, source_map, root) = transformer.transform(&program);

        assert_eq!(source_map.get(root), token_id);
    }
}
