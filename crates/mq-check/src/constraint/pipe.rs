//! Pipe chain constraint generation for Block and Function/Macro body expressions.

use mq_hir::{Hir, SymbolId, SymbolKind};

use crate::infer::InferenceContext;
use crate::types::Type;

use super::helpers::{ChildrenIndex, get_children, get_symbol_range};
use super::{Constraint, ConstraintOrigin, generate_symbol_constraints};

/// Generates constraints for Block symbols (pipe chains).
///
/// In mq, `x | f(y)` means the output of `x` flows into `f` as an implicit first argument.
/// Block symbols represent pipe chains; their children are the expressions in sequence.
/// This function threads the output type of each child to the next child's piped input,
/// and sets the Block's type to its last child's type.
pub(super) fn generate_block_constraints(
    hir: &Hir,
    symbol_id: SymbolId,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    let children = get_children(children_index, symbol_id);

    if children.is_empty() {
        let ty_var = ctx.fresh_var();
        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        return;
    }

    // Thread types through the pipe chain sequentially:
    // Set piped input and re-process each child before moving to the next,
    // so that resolved types propagate correctly through the chain.
    for i in 1..children.len() {
        let prev_ty = ctx.get_or_create_symbol_type(children[i - 1]);
        ctx.set_piped_input(children[i], prev_ty);

        // Re-process Call/Ref children that received piped input,
        // since they were already processed in Pass 3 before piped inputs were set
        if let Some(child_symbol) = hir.symbol(children[i]) {
            match &child_symbol.kind {
                SymbolKind::Call | SymbolKind::Ref | SymbolKind::Assign | SymbolKind::Selector(_) => {
                    generate_symbol_constraints(hir, children[i], child_symbol.kind.clone(), ctx, children_index);
                }
                // For Variable (`let x = expr`), propagate piped input to the initializer
                // (last child) so that `items | let x = first()` correctly passes `items`
                // to `first()`.
                SymbolKind::Variable | SymbolKind::DestructuringBinding => {
                    propagate_piped_input_to_variable_initializer(hir, children[i], ctx, children_index);
                }
                _ => {}
            }
        }
    }

    // Propagate piped input through UnaryOp to inner Call/Ref children.
    // e.g., `x | !is_empty()` — piped input flows through `!` to `is_empty()`
    propagate_piped_input_through_unary_ops(hir, children, ctx, children_index);

    // The Block's type is the last meaningful (non-Keyword) child's type.
    // Keyword children like `end` are syntax delimiters with no semantic type;
    // using them would give a fresh Var that later gets unified away, losing
    // type information (e.g., the None possibility from a while-loop result).
    let last_meaningful = children
        .iter()
        .rev()
        .find(|&&id| {
            hir.symbol(id)
                .map(|s| !matches!(s.kind, SymbolKind::Keyword))
                .unwrap_or(true)
        })
        .unwrap_or_else(|| children.last().unwrap());
    let last_ty = ctx.get_or_create_symbol_type(*last_meaningful);
    ctx.set_symbol_type(symbol_id, last_ty);
}

/// Generates pipe constraints for implicit pipe chains in Function/Macro bodies.
///
/// When a function body has multiple non-parameter children (e.g., `def f(x): a | b`),
/// the output of each expression flows into the next as piped input.
pub(super) fn generate_function_body_pipe_constraints(
    hir: &Hir,
    symbol_id: SymbolId,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    let children = get_children(children_index, symbol_id);

    // Get non-parameter body children
    let body_children: Vec<SymbolId> = children
        .iter()
        .copied()
        .filter(|&child_id| {
            hir.symbol(child_id)
                .map(|s| !matches!(s.kind, SymbolKind::Parameter | SymbolKind::Keyword))
                .unwrap_or(false)
        })
        .collect();

    if body_children.len() <= 1 {
        return;
    }

    // Thread types through the pipe chain sequentially:
    // Set piped input and re-process each child before moving to the next,
    // so that resolved types propagate correctly through the chain.
    for i in 1..body_children.len() {
        let prev_ty = ctx.get_or_create_symbol_type(body_children[i - 1]);
        ctx.set_piped_input(body_children[i], prev_ty);

        // Re-process Call/Ref children that received piped input
        if let Some(child_symbol) = hir.symbol(body_children[i]) {
            match &child_symbol.kind {
                SymbolKind::Call | SymbolKind::Ref | SymbolKind::Assign | SymbolKind::Selector(_) => {
                    generate_symbol_constraints(hir, body_children[i], child_symbol.kind.clone(), ctx, children_index);
                }
                SymbolKind::Variable => {
                    propagate_piped_input_to_variable_initializer(hir, body_children[i], ctx, children_index);
                }
                _ => {}
            }
        }
    }

    // Propagate piped input through UnaryOp to inner Call/Ref children
    propagate_piped_input_through_unary_ops(hir, &body_children, ctx, children_index);
}

/// Propagates piped input through UnaryOp nodes to their inner Call/Ref children.
///
/// When a pipe chain contains `x | !f()`, the piped value from `x` flows through
/// the `!` (UnaryOp) to `f()` (Call). This function handles that propagation and
/// re-processes the inner Call/Ref to pick up the piped input.
pub(super) fn propagate_piped_input_through_unary_ops(
    hir: &Hir,
    children: &[SymbolId],
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    for &child_id in children.iter().skip(1) {
        if let Some(child_symbol) = hir.symbol(child_id)
            && matches!(child_symbol.kind, SymbolKind::UnaryOp)
            && let Some(piped_ty) = ctx.get_piped_input(child_id).cloned()
        {
            let inner_children = get_children(children_index, child_id);
            for &inner_id in inner_children {
                if let Some(inner_sym) = hir.symbol(inner_id)
                    && matches!(inner_sym.kind, SymbolKind::Call | SymbolKind::Ref)
                {
                    ctx.set_piped_input(inner_id, piped_ty.clone());
                    generate_symbol_constraints(hir, inner_id, inner_sym.kind.clone(), ctx, children_index);
                }
            }
        }
    }
}

/// Propagates piped input from a Variable to its initializer expression (last child).
///
/// When `let x = first()` appears inside a pipe chain (e.g., `items | let x = first()`),
/// the piped input set on the Variable must be forwarded to the inner Call/Ref so that
/// `first()` sees `items` as its implicit first argument.
fn propagate_piped_input_to_variable_initializer(
    hir: &Hir,
    variable_id: SymbolId,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    let Some(piped_ty) = ctx.get_piped_input(variable_id).cloned() else {
        return;
    };

    let children = get_children(children_index, variable_id);
    let Some(&init_id) = children.last() else {
        return;
    };

    let Some(init_sym) = hir.symbol(init_id) else {
        return;
    };

    match &init_sym.kind {
        SymbolKind::Call | SymbolKind::Ref | SymbolKind::Assign | SymbolKind::Selector(_) => {
            ctx.set_piped_input(init_id, piped_ty);
            generate_symbol_constraints(hir, init_id, init_sym.kind.clone(), ctx, children_index);
        }
        _ => {}
    }
}

/// Resolves the body type of an elif/else branch.
///
/// For Elif: children are [condition, body] — constrains condition to Bool, returns body type.
/// For Else: children are [body] — returns body type.
/// For other kinds: returns the symbol's type directly.
pub(super) fn resolve_branch_body_type(
    hir: &Hir,
    branch_id: SymbolId,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) -> Type {
    if let Some(symbol) = hir.symbol(branch_id) {
        let children = get_children(children_index, branch_id);
        match symbol.kind {
            SymbolKind::Elif => {
                if children.len() >= 2 {
                    let range = get_symbol_range(hir, branch_id);
                    // First child is condition
                    let cond_ty = ctx.get_or_create_symbol_type(children[0]);
                    ctx.add_constraint(Constraint::Equal(cond_ty, Type::Bool, range, ConstraintOrigin::General));
                    // Last child is the body
                    let body_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
                    ctx.set_symbol_type(branch_id, body_ty.clone());
                    body_ty
                } else {
                    ctx.get_or_create_symbol_type(branch_id)
                }
            }
            SymbolKind::Else => {
                if let Some(&last_child) = children.last() {
                    let body_ty = ctx.get_or_create_symbol_type(last_child);
                    ctx.set_symbol_type(branch_id, body_ty.clone());
                    body_ty
                } else {
                    ctx.get_or_create_symbol_type(branch_id)
                }
            }
            _ => ctx.get_or_create_symbol_type(branch_id),
        }
    } else {
        ctx.get_or_create_symbol_type(branch_id)
    }
}
