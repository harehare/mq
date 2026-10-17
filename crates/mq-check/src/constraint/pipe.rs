//! Pipe chain constraint generation for Block and Function body expressions.

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
        thread_pipe_stage(hir, children[i - 1], children[i], ctx, children_index);
    }

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

/// Generates pipe constraints for implicit pipe chains in Function bodies.
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

    // Connect the function's declared return type variable to the last body expression type.
    // This is the key HM inference step: without it, user-defined function return types remain
    // as free type variables and `propagate_user_call_returns` cannot resolve them, making
    // cross-function type errors (e.g. `c() + 1` where c returns Bool) invisible.
    if let Some(&last_body_id) = body_children
        .last()
        .filter(|_| ctx.function_yields(symbol_id).is_empty())
    {
        let body_ty = ctx.get_or_create_symbol_type(last_body_id);
        let func_ty = ctx.get_or_create_symbol_type(symbol_id);
        if let Type::Function(_, ret_ty) = func_ty {
            let range = get_symbol_range(hir, symbol_id);
            ctx.add_constraint(Constraint::Equal(*ret_ty, body_ty, range, ConstraintOrigin::General));
        }
    }

    if body_children.len() <= 1 {
        return;
    }

    // Thread types through the pipe chain sequentially:
    // Set piped input and re-process each child before moving to the next,
    // so that resolved types propagate correctly through the chain.
    for i in 1..body_children.len() {
        thread_pipe_stage(hir, body_children[i - 1], body_children[i], ctx, children_index);
    }
}

/// Hands the output of `prev` to `stage` as its piped input and re-processes the stages
/// that read it, since they were already processed before the input was known.
pub(super) fn thread_pipe_stage(
    hir: &Hir,
    prev: SymbolId,
    stage: SymbolId,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    let (prev_ty, prev_source) = pipe_stage_output(hir, ctx, prev);
    ctx.set_piped_input(stage, prev_ty.clone());
    if let Some(source) = prev_source {
        ctx.set_piped_source(stage, source);
    }

    let Some(symbol) = hir.symbol(stage) else {
        return;
    };
    match &symbol.kind {
        SymbolKind::Call | SymbolKind::Ref | SymbolKind::Assign | SymbolKind::Selector(_) => {
            generate_symbol_constraints(hir, stage, symbol.kind.clone(), ctx, children_index);
        }
        // For Variable (`let x = expr`), propagate piped input to the initializer
        // (last child) so that `items | let x = first()` passes `items` to `first()`.
        SymbolKind::Variable | SymbolKind::DestructuringBinding => {
            propagate_piped_input_to_variable_initializer(hir, stage, ctx, children_index);
        }
        // `x | !f()` and `x | f() == y`: the piped value flows through the operator to `f()`,
        // and the operator's own type is then re-derived from its operands.
        SymbolKind::UnaryOp | SymbolKind::BinaryOp => {
            propagate_into_operands(hir, stage, &prev_ty, ctx, children_index);
            generate_symbol_constraints(hir, stage, symbol.kind.clone(), ctx, children_index);
        }
        SymbolKind::If => forward_into_if(hir, stage, &prev_ty, ctx, children_index),
        _ => {}
    }
}

/// Hands the piped input of an `if` to its condition and to the bodies of its branches, since
/// `x | if (is_none()): None else: to_text()` calls both with `x`.
pub(super) fn forward_into_if(
    hir: &Hir,
    if_id: SymbolId,
    piped_ty: &Type,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    for &child_id in get_children(children_index, if_id) {
        let Some(child) = hir.symbol(child_id) else {
            continue;
        };
        match child.kind {
            SymbolKind::Elif | SymbolKind::Else => {
                for &part_id in get_children(children_index, child_id) {
                    forward_into_expression(hir, part_id, piped_ty, ctx, children_index);
                }
            }
            _ => forward_into_expression(hir, child_id, piped_ty, ctx, children_index),
        }
    }
}

fn forward_into_expression(
    hir: &Hir,
    id: SymbolId,
    piped_ty: &Type,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    let Some(symbol) = hir.symbol(id) else {
        return;
    };
    match &symbol.kind {
        SymbolKind::Call | SymbolKind::Ref | SymbolKind::Assign | SymbolKind::Selector(_) => {
            ctx.set_piped_input(id, piped_ty.clone());
            generate_symbol_constraints(hir, id, symbol.kind.clone(), ctx, children_index);
        }
        SymbolKind::UnaryOp | SymbolKind::BinaryOp => {
            propagate_into_operands(hir, id, piped_ty, ctx, children_index);
            generate_symbol_constraints(hir, id, symbol.kind.clone(), ctx, children_index);
        }
        SymbolKind::Variable | SymbolKind::DestructuringBinding => {
            ctx.set_piped_input(id, piped_ty.clone());
            propagate_piped_input_to_variable_initializer(hir, id, ctx, children_index);
        }
        SymbolKind::If => forward_into_if(hir, id, piped_ty, ctx, children_index),
        SymbolKind::Block => {
            if let Some(&first) = get_children(children_index, id).first() {
                forward_into_expression(hir, first, piped_ty, ctx, children_index);
            }
        }
        _ => {}
    }
}

/// Calls `f` with every Call/Ref operand of the unary/binary operator `operator_id`,
/// looking through nested operators.
pub(super) fn for_each_operand_call(
    hir: &Hir,
    operator_id: SymbolId,
    children_index: &ChildrenIndex,
    f: &mut impl FnMut(SymbolId, &SymbolKind),
) {
    for &operand_id in get_children(children_index, operator_id) {
        let Some(operand) = hir.symbol(operand_id) else {
            continue;
        };
        match operand.kind {
            SymbolKind::Call | SymbolKind::Ref => f(operand_id, &operand.kind),
            SymbolKind::UnaryOp | SymbolKind::BinaryOp => for_each_operand_call(hir, operand_id, children_index, f),
            _ => {}
        }
    }
}

fn propagate_into_operands(
    hir: &Hir,
    operator_id: SymbolId,
    piped_ty: &Type,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    for_each_operand_call(hir, operator_id, children_index, &mut |id, kind| {
        ctx.set_piped_input(id, piped_ty.clone());
        generate_symbol_constraints(hir, id, kind.clone(), ctx, children_index);
    });
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

/// What a pipe stage passes on to the next one, as its type and the symbol that holds it.
/// A function definition passes on its own input unchanged instead of its function type.
pub(super) fn pipe_stage_output(hir: &Hir, ctx: &mut InferenceContext, stage: SymbolId) -> (Type, Option<SymbolId>) {
    let is_definition = hir
        .symbol(stage)
        .is_some_and(|symbol| matches!(symbol.kind, SymbolKind::Function(_)));
    if is_definition && let Some(input) = ctx.get_piped_input(stage).cloned() {
        return (input, ctx.get_piped_source(stage));
    }
    (ctx.get_or_create_symbol_type(stage), Some(stage))
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
