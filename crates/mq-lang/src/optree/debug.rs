//! OpTree debugging and visualization utilities.
//!
//! This module provides tools for inspecting and visualizing OpTree instructions,
//! useful for debugging, optimization, and understanding program structure.

use super::{Op, OpPool, OpRef, SourceMap};
use std::fmt::{self, Write};

/// Dumps the entire OpTree as a human-readable string.
///
/// This function creates a formatted representation of all instructions in the OpTree,
/// showing their index, source location, and content. Useful for debugging and
/// understanding the flattened structure.
///
/// # Example
///
/// ```rust,ignore
/// let transformer = OpTreeTransformer::new();
/// let (pool, source_map, root) = transformer.transform(&program);
///
/// let dump = dump_optree(&pool, &source_map, root);
/// println!("{}", dump);
/// ```
///
/// Output format:
/// ```text
/// 0000 [TokenId(42)] Literal(Number(1.0))
/// 0001 [TokenId(43)] Literal(Number(2.0))
/// 0002 [TokenId(44)] Call { name: "add", args: [OpRef(0), OpRef(1)] }
/// ```
pub fn dump_optree(pool: &OpPool, source_map: &SourceMap, root: OpRef) -> String {
    let mut output = String::new();
    let debugger = OpTreeDebugger::new(pool, source_map);

    writeln!(&mut output, "=== OpTree Dump (root: OpRef({})) ===", root.id()).unwrap();
    writeln!(&mut output, "Total instructions: {}", pool.len()).unwrap();
    writeln!(&mut output).unwrap();

    debugger.dump_op(root, 0, &mut output, true).unwrap();

    output
}

/// Dumps all instructions in the OpPool sequentially.
///
/// This provides a flat, sequential view of all instructions without
/// following the tree structure. Useful for seeing the entire pool contents.
pub fn dump_optree_sequential(pool: &OpPool, source_map: &SourceMap) -> String {
    let mut output = String::new();

    writeln!(&mut output, "=== OpTree Sequential Dump ===").unwrap();
    writeln!(&mut output, "Total instructions: {}", pool.len()).unwrap();
    writeln!(&mut output).unwrap();

    for (op_ref, op) in pool.iter() {
        let token_id = source_map.get(op_ref);
        writeln!(&mut output, "{:04} [TokenId({:?})] {:?}", op_ref.id(), token_id, op).unwrap();
    }

    output
}

/// OpTree debugger for structured visualization.
struct OpTreeDebugger<'a> {
    pool: &'a OpPool,
    source_map: &'a SourceMap,
}

impl<'a> OpTreeDebugger<'a> {
    fn new(pool: &'a OpPool, source_map: &'a SourceMap) -> Self {
        Self { pool, source_map }
    }

    /// Dumps an operation and its children recursively.
    fn dump_op(&self, op_ref: OpRef, indent: usize, output: &mut String, recurse: bool) -> fmt::Result {
        let op = self.pool.get(op_ref);
        let token_id = self.source_map.get(op_ref);
        let indent_str = "  ".repeat(indent);

        // Write the instruction
        writeln!(
            output,
            "{}{:04} [TokenId({:?})] {}",
            indent_str,
            op_ref.id(),
            token_id,
            format_op_brief(op)
        )?;

        // Recursively dump child ops
        if recurse {
            let children = self.get_child_ops(op);
            for child in children {
                self.dump_op(child, indent + 1, output, true)?;
            }
        }

        Ok(())
    }

    /// Extracts child OpRefs from an Op variant.
    fn get_child_ops(&self, op: &Op) -> Vec<OpRef> {
        let mut children = Vec::new();

        match op {
            Op::Let { value, .. } | Op::Var { value, .. } | Op::Assign { value, .. } => {
                children.push(*value);
            }

            Op::If { branches } => {
                for (cond, body) in branches {
                    if let Some(c) = cond {
                        children.push(*c);
                    }
                    children.push(*body);
                }
            }

            Op::While { condition, body } => {
                children.push(*condition);
                children.push(*body);
            }

            Op::Foreach { iterator, body, .. } => {
                children.push(*iterator);
                children.push(*body);
            }

            Op::Match { value, arms } => {
                children.push(*value);
                for arm in arms {
                    if let Some(guard) = arm.guard {
                        children.push(guard);
                    }
                    children.push(arm.body);
                }
            }

            Op::Def { params, body, .. } | Op::Fn { params, body } => {
                children.extend(params.iter().copied());
                children.push(*body);
            }

            Op::Call { args, .. } => {
                children.extend(args.iter().copied());
            }

            Op::CallDynamic { callable, args } => {
                children.push(*callable);
                children.extend(args.iter().copied());
            }

            Op::Block(body) | Op::Paren(body) => {
                children.push(*body);
            }

            Op::Sequence(ops) => {
                children.extend(ops.iter().copied());
            }

            Op::And(left, right) | Op::Or(left, right) => {
                children.push(*left);
                children.push(*right);
            }

            Op::InterpolatedString(segments) => {
                for segment in segments {
                    if let super::StringSegment::Expr(expr_ref) = segment {
                        children.push(*expr_ref);
                    }
                }
            }

            Op::QualifiedAccess { target, .. } => {
                if let super::AccessTarget::Call(_, args) = target {
                    children.extend(args.iter().copied());
                }
            }

            Op::Module { body, .. } => {
                children.push(*body);
            }

            Op::Try { try_expr, catch_expr } => {
                children.push(*try_expr);
                children.push(*catch_expr);
            }

            // Leaf nodes with no children
            Op::Literal(_)
            | Op::Ident(_)
            | Op::Self_
            | Op::Nodes
            | Op::Break
            | Op::Continue
            | Op::Selector(_)
            | Op::Include(_)
            | Op::Import(_) => {}
        }

        children
    }
}

/// Formats an Op for brief display (one-line summary).
fn format_op_brief(op: &Op) -> String {
    match op {
        Op::Literal(lit) => format!("Literal({:?})", lit),
        Op::Ident(ident) => format!("Ident({})", ident),
        Op::Self_ => "Self_".to_string(),
        Op::Nodes => "Nodes".to_string(),

        Op::Let { name, value } => format!("Let {{ name: {}, value: {} }}", name, value),
        Op::Var { name, value } => format!("Var {{ name: {}, value: {} }}", name, value),
        Op::Assign { name, value } => format!("Assign {{ name: {}, value: {} }}", name, value),

        Op::If { branches } => format!("If {{ branches: {} }}", branches.len()),
        Op::While { condition, body } => format!("While {{ condition: {}, body: {} }}", condition, body),
        Op::Foreach { name, iterator, body } => {
            format!("Foreach {{ name: {}, iterator: {}, body: {} }}", name, iterator, body)
        }
        Op::Match { value, arms } => format!("Match {{ value: {}, arms: {} }}", value, arms.len()),
        Op::Break => "Break".to_string(),
        Op::Continue => "Continue".to_string(),

        Op::Def { name, params, body } => {
            format!("Def {{ name: {}, params: {}, body: {} }}", name, params.len(), body)
        }
        Op::Fn { params, body } => format!("Fn {{ params: {}, body: {} }}", params.len(), body),
        Op::Call { name, args } => format!("Call {{ name: {}, args: {} }}", name, args.len()),
        Op::CallDynamic { callable, args } => {
            format!("CallDynamic {{ callable: {}, args: {} }}", callable, args.len())
        }

        Op::Block(body) => format!("Block({})", body),
        Op::Sequence(ops) => format!("Sequence([{}])", ops.len()),

        Op::And(left, right) => format!("And({}, {})", left, right),
        Op::Or(left, right) => format!("Or({}, {})", left, right),
        Op::Paren(expr) => format!("Paren({})", expr),

        Op::InterpolatedString(segments) => format!("InterpolatedString(segments: {})", segments.len()),

        Op::Selector(sel) => format!("Selector({:?})", sel),
        Op::QualifiedAccess { module_path, target } => {
            format!("QualifiedAccess {{ path: {:?}, target: {:?} }}", module_path, target)
        }

        Op::Module { name, body } => format!("Module {{ name: {}, body: {} }}", name, body),
        Op::Include(lit) => format!("Include({:?})", lit),
        Op::Import(lit) => format!("Import({:?})", lit),

        Op::Try { try_expr, catch_expr } => {
            format!("Try {{ try: {}, catch: {} }}", try_expr, catch_expr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Shared,
        arena::ArenaId,
        ast::node::{self as ast, Literal},
        number::Number,
        optree::{Op, OpTreeTransformer},
    };

    fn create_test_node(expr: ast::Expr, token_id: crate::TokenId) -> Shared<ast::Node> {
        Shared::new(ast::Node {
            token_id,
            expr: Shared::new(expr),
        })
    }

    #[test]
    fn test_dump_optree_simple() {
        let transformer = OpTreeTransformer::new();
        let node = create_test_node(ast::Expr::Literal(Literal::Number(Number::from(42.0))), ArenaId::new(1));
        let program = vec![node];

        let (pool, source_map, root) = transformer.transform(&program);
        let dump = dump_optree(&pool, &source_map, root);

        assert!(dump.contains("OpTree Dump"));
        assert!(dump.contains("Literal"));
        assert!(!dump.is_empty());
    }

    #[test]
    fn test_dump_optree_sequential() {
        let mut pool = OpPool::new();
        let mut source_map = SourceMap::new();

        let _ = source_map.register(ArenaId::new(1));
        pool.alloc(Op::Literal(Literal::Number(Number::from(1.0))));

        let _ = source_map.register(ArenaId::new(2));
        pool.alloc(Op::Literal(Literal::Number(Number::from(2.0))));

        let dump = dump_optree_sequential(&pool, &source_map);

        assert!(dump.contains("Sequential Dump"));
        assert!(dump.contains("0000"));
        assert!(dump.contains("0001"));
    }

    #[test]
    fn test_format_op_brief() {
        let op = Op::Literal(Literal::Number(Number::from(42.0)));
        let brief = format_op_brief(&op);
        assert!(brief.contains("Literal"));

        let op = Op::Ident("x".into());
        let brief = format_op_brief(&op);
        assert!(brief.contains("Ident"));
        assert!(brief.contains("x"));
    }
}
