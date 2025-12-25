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
    /// Creates a new transformer with default capacity.
    pub fn new() -> Self {
        Self {
            pool: OpPool::new(),
            source_map: SourceMap::new(),
        }
    }

    /// Creates a new transformer with specified capacity hint.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            pool: OpPool::with_capacity(capacity),
            source_map: SourceMap::with_capacity(capacity),
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

    /// Transforms a program (sequence of nodes) into an OpRef.
    fn transform_program(&mut self, program: &Program) -> OpRef {
        if program.is_empty() {
            // Empty program → return Nodes (no-op that passes through input)
            let _op_id = self.source_map.register(crate::arena::ArenaId::new(0));
            return self.pool.alloc(Op::Nodes);
        }

        if program.len() == 1 {
            // Single expression → transform directly
            return self.transform_node(&program[0]);
        }

        // Multiple expressions → create Sequence
        let ops: SmallVec<[OpRef; 8]> = program.iter().map(|node| self.transform_node(node)).collect();

        // Use first node's token for sequence
        let token_id = program[0].token_id;
        let _ = self.source_map.register(token_id);

        self.pool.alloc(Op::Sequence(ops))
    }

    /// Transforms a single AST node into an OpRef.
    fn transform_node(&mut self, node: &Shared<ast::Node>) -> OpRef {
        let token_id = node.token_id;
        let _ = self.source_map.register(token_id);

        let op = match &*node.expr {
            // ===== Literals & Values =====
            ast::Expr::Literal(lit) => Op::Literal(lit.clone()),

            ast::Expr::Ident(ident) => Op::Ident(ident.name),

            ast::Expr::Self_ => Op::Self_,

            ast::Expr::Nodes => Op::Nodes,

            // ===== Variables =====
            ast::Expr::Let(ident, value) => {
                let value_ref = self.transform_node(value);
                Op::Let {
                    name: ident.name,
                    value: value_ref,
                }
            }

            ast::Expr::Var(ident, value) => {
                let value_ref = self.transform_node(value);
                Op::Var {
                    name: ident.name,
                    value: value_ref,
                }
            }

            ast::Expr::Assign(ident, value) => {
                let value_ref = self.transform_node(value);
                Op::Assign {
                    name: ident.name,
                    value: value_ref,
                }
            }

            // ===== Control Flow =====
            ast::Expr::If(branches) => {
                let op_branches: SmallVec<_> = branches
                    .iter()
                    .map(|(cond, body)| {
                        let cond_ref = cond.as_ref().map(|c| self.transform_node(c));
                        let body_ref = self.transform_node(body);
                        (cond_ref, body_ref)
                    })
                    .collect();
                Op::If { branches: op_branches }
            }

            ast::Expr::While(cond, body) => {
                let cond_ref = self.transform_node(cond);
                let body_ref = self.transform_program(body);
                Op::While {
                    condition: cond_ref,
                    body: body_ref,
                }
            }

            ast::Expr::Foreach(ident, iter, body) => {
                let iter_ref = self.transform_node(iter);
                let body_ref = self.transform_program(body);
                Op::Foreach {
                    name: ident.name,
                    iterator: iter_ref,
                    body: body_ref,
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
                    arms: op_arms,
                }
            }

            ast::Expr::Break => Op::Break,

            ast::Expr::Continue => Op::Continue,

            // ===== Functions =====
            ast::Expr::Def(ident, params, body) => {
                let param_refs: SmallVec<_> = params.iter().map(|p| self.transform_node(p)).collect();
                let body_ref = self.transform_program(body);
                Op::Def {
                    name: ident.name,
                    params: param_refs,
                    body: body_ref,
                }
            }

            ast::Expr::Fn(params, body) => {
                let param_refs: SmallVec<_> = params.iter().map(|p| self.transform_node(p)).collect();
                let body_ref = self.transform_program(body);
                Op::Fn {
                    params: param_refs,
                    body: body_ref,
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

            // ===== Blocks & Sequences =====
            ast::Expr::Block(program) => {
                let body_ref = self.transform_program(program);
                Op::Block(body_ref)
            }

            // ===== Operators =====
            ast::Expr::And(left, right) => {
                let left_ref = self.transform_node(left);
                let right_ref = self.transform_node(right);
                Op::And(left_ref, right_ref)
            }

            ast::Expr::Or(left, right) => {
                let left_ref = self.transform_node(left);
                let right_ref = self.transform_node(right);
                Op::Or(left_ref, right_ref)
            }

            ast::Expr::Paren(expr) => {
                let expr_ref = self.transform_node(expr);
                Op::Paren(expr_ref)
            }

            // ===== String Operations =====
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

            // ===== Selectors =====
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

            // ===== Modules =====
            ast::Expr::Module(ident, body) => {
                let body_ref = self.transform_program(body);
                Op::Module {
                    name: ident.name,
                    body: body_ref,
                }
            }

            ast::Expr::Include(lit) => Op::Include(lit.clone()),

            ast::Expr::Import(lit) => Op::Import(lit.clone()),

            // ===== Error Handling =====
            ast::Expr::Try(try_expr, catch_expr) => {
                let try_ref = self.transform_node(try_expr);
                let catch_ref = self.transform_node(catch_expr);
                Op::Try {
                    try_expr: try_ref,
                    catch_expr: catch_ref,
                }
            }

            // ===== Macros (should be expanded before transformation) =====
            ast::Expr::Macro(_, _, _) | ast::Expr::Quote(_) | ast::Expr::Unquote(_) => {
                // These should never appear in the AST after macro expansion.
                // If they do, it indicates a bug in macro expansion or incorrect usage.
                panic!(
                    "Unexpanded macro construct encountered in OpTree transformation. \
                     Macros should be expanded before transforming to OpTree."
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

        assert!(matches!(pool.get(root).as_ref(), Op::Ident(name) if name.as_str() == "x"));
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
            Op::Let { name, value } => {
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

        match pool.get(root).as_ref() {
            Op::If { branches } => {
                assert_eq!(branches.len(), 2);
                assert!(branches[0].0.is_some()); // if condition
                assert!(branches[1].0.is_none()); // else (no condition)
            }
            _ => panic!("Expected If op"),
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
