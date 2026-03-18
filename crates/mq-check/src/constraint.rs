//! Constraint generation for type inference.

mod categories;
mod helpers;
mod pipe;

// Re-export pub(crate) items that external modules import from `constraint`
pub(crate) use helpers::{
    ChildrenIndex, attr_kind_to_type, build_children_index, get_children, get_non_keyword_children,
};

use categories::categorize_symbols;
use helpers::{
    build_piped_call_args, collect_break_value_types, collect_pattern_variable_descendants, find_enclosing_function,
    find_lambda_function_child, get_post_loop_siblings, get_symbol_range, is_foreach_iterable_ref,
    is_inside_quote_block, merge_loop_types, might_receive_piped_input, resolve_builtin_call, resolve_pattern_type,
};
use pipe::{generate_block_constraints, generate_function_body_pipe_constraints, resolve_branch_body_type};

use crate::infer::{
    DeferredOverload, DeferredParameterCall, DeferredUserCall, InferenceContext, NarrowingEntry, TypeNarrowing,
};
use crate::narrowing::analyze_condition;
use crate::types::Type;
use crate::unify::range_to_span;
use crate::{TypeError, infer};
use mq_hir::{Hir, SymbolId, SymbolKind};

use smol_str::SmolStr;
use std::fmt;

/// The origin/reason a type constraint was generated.
///
/// Used to provide contextual help messages in type error diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ConstraintOrigin {
    /// Constraint from argument N to a function call
    Argument { fn_name: SmolStr, arg_index: usize },
    /// Constraint from piped input to a function
    PipedInput { fn_name: SmolStr },
    /// Constraint from a binary or unary operator operand
    Operator { op: SmolStr },
    /// Constraint from a function return type
    ReturnType { fn_name: SmolStr },
    /// Constraint from a variable assignment
    Assignment { var_name: SmolStr },
    /// General constraint with no specific origin
    #[default]
    General,
}

impl ConstraintOrigin {
    /// Returns a human-readable context string for help messages, if applicable.
    pub fn to_context_string(&self) -> Option<String> {
        match self {
            ConstraintOrigin::Argument { fn_name, arg_index } => {
                Some(format!("in argument {} to '{}'", arg_index + 1, fn_name))
            }
            ConstraintOrigin::PipedInput { fn_name } => Some(format!("in piped input to '{}'", fn_name)),
            ConstraintOrigin::Operator { op } => Some(format!("in operand of '{}'", op)),
            ConstraintOrigin::ReturnType { fn_name } => Some(format!("in return type of '{}'", fn_name)),
            ConstraintOrigin::Assignment { var_name } => Some(format!("in assignment to '{}'", var_name)),
            ConstraintOrigin::General => None,
        }
    }
}

/// Type constraint for unification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// Two types must be equal
    Equal(Type, Type, Option<mq_lang::Range>, ConstraintOrigin),
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constraint::Equal(t1, t2, _, _) => write!(f, "{} ~ {}", t1, t2),
        }
    }
}

/// Generates type constraints from HIR
pub fn generate_constraints(hir: &Hir, ctx: &mut InferenceContext) {
    // Build children index once to avoid O(n) scans in get_children()
    let children_index = build_children_index(hir);

    // Categorize symbols in a single pass (replaces 5 separate iterations)
    let cats = categorize_symbols(hir);

    // Pass 1: Assign types to literals, variables, and simple constructs.
    //
    // Process in dependency order so that when a Variable generates its constraint
    // `Equal(Var(tX), type_of_initializer)`, the initializer already has a concrete
    // type rather than an orphan type variable.  The required order is:
    //   1. Literals (no dependencies)
    //   2. Parameters / PatternVariables (no dependencies)
    //   3. Functions / Macros (depend on parameters via get_or_create)
    //   4. Variables (depend on their initializer expression)
    //
    // Without this ordering, a `let f = fn(x): x - 1;` pattern breaks: the Variable
    // for `f` is inserted before the lambda Function in the HIR, so processing in
    // insertion order creates a stale orphan type variable for the lambda before its
    // proper `Function(...)` type is established.  This severs the chain between the
    // call-site argument type and the lambda's parameter type variable, preventing
    // call-site type errors from being detected (e.g. `f("str")` not flagged as error).
    let mut sorted_pass1 = cats.pass1_symbols.clone();
    sorted_pass1.sort_by_key(|(_, kind)| match kind {
        SymbolKind::Number | SymbolKind::String | SymbolKind::Boolean | SymbolKind::Symbol | SymbolKind::None => 0u8,
        SymbolKind::Parameter | SymbolKind::PatternVariable { .. } => 1,
        SymbolKind::Function(_) | SymbolKind::Macro(_) => 2,
        SymbolKind::Variable | SymbolKind::DestructuringBinding => 3,
        _ => 4,
    });
    for (symbol_id, kind) in &sorted_pass1 {
        generate_symbol_constraints(hir, *symbol_id, kind.clone(), ctx, &children_index);
    }

    // Pass 2: Set up piped inputs for root-level symbols.
    //
    // Root-level symbols (parent=None, not builtin/module) form an implicit pipe chain.
    // In the same pass, propagate piped input into Variable initializer expressions so that
    // Pass 3 can resolve calls like `items | let x = first()` without a second iteration
    // (avoids re-scanning root_symbols and an extra `get_piped_input` lookup per Variable).
    for i in 1..cats.root_symbols.len() {
        let prev_ty = ctx.get_or_create_symbol_type(cats.root_symbols[i - 1]);
        ctx.set_piped_input(cats.root_symbols[i], prev_ty.clone());

        // For root-level Variables (e.g. `let x = first()`), forward the piped type to
        // the initializer Call/Ref so Pass 3 sees it before resolving the overload.
        if let Some(sym) = hir.symbol(cats.root_symbols[i])
            && matches!(sym.kind, SymbolKind::Variable | SymbolKind::DestructuringBinding)
        {
            let init_children = get_children(&children_index, cats.root_symbols[i]);
            if let Some(&init_id) = init_children.last() {
                ctx.set_piped_input(init_id, prev_ty);
            }
        }
    }

    // Pass 2.5: Process Assign symbols before other operators/calls.
    // This ensures that variable types are updated by assignments before
    // Refs and Calls in Pass 3 resolve against the (potentially stale) type.
    // e.g., `var x = 10 | x = "hello" | upcase(x)` — the Assign updates
    // x's type to String before upcase(x) resolves its argument type.
    for (symbol_id, kind) in &cats.assign_symbols {
        generate_symbol_constraints(hir, *symbol_id, kind.clone(), ctx, &children_index);
    }

    // Pass 3: Process operators, calls, etc.
    // Process children before parents: symbols inserted later (children) must be
    // typed before their parents.  The original `.rev()` on the pass3 vec relied on
    // children having *higher* SlotMap IDs than parents, which holds on a fresh HIR
    // but breaks on subsequent `add_nodes` calls because SlotMap reuses freed slots
    // in LIFO order (most-recently-freed first), reversing parent/child IDs.
    // Using the explicit insertion-order counter avoids this fragility.
    let mut pass3_sorted = cats.pass3_symbols;
    pass3_sorted.sort_by_key(|(id, _)| std::cmp::Reverse(hir.symbol_insertion_order(*id)));
    for (symbol_id, kind) in pass3_sorted {
        generate_symbol_constraints(hir, symbol_id, kind, ctx, &children_index);
    }

    // Pass 4: Process Block symbols and Function body pipe chains
    // Children are now typed, so we can thread output types through the chain
    for symbol_id in &cats.pass4_blocks {
        generate_block_constraints(hir, *symbol_id, ctx, &children_index);
    }
    for symbol_id in &cats.pass4_functions {
        generate_function_body_pipe_constraints(hir, *symbol_id, ctx, &children_index);
    }
}

/// Generates constraints for a single symbol
pub(super) fn generate_symbol_constraints(
    hir: &Hir,
    symbol_id: SymbolId,
    kind: SymbolKind,
    ctx: &mut InferenceContext,
    children_index: &ChildrenIndex,
) {
    match kind {
        // Literals have concrete types
        SymbolKind::Number => {
            let ty = Type::Number;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::String => {
            let ty = Type::String;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::Boolean => {
            let ty = Type::Bool;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::Symbol => {
            let ty = Type::Symbol;
            ctx.set_symbol_type(symbol_id, ty);
        }
        SymbolKind::None => {
            let ty = Type::None;
            ctx.set_symbol_type(symbol_id, ty);
        }

        // Parameters and pattern variables get fresh type variables
        SymbolKind::Parameter | SymbolKind::PatternVariable { .. } => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        // Variables get fresh type variables, constrained to their initializer
        SymbolKind::Variable => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));

            // Connect variable type to its initializer expression (last child)
            let children = get_children(children_index, symbol_id);
            if let Some(&last_child) = children.last() {
                let child_ty = ctx.get_or_create_symbol_type(last_child);
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(
                    Type::Var(ty_var),
                    child_ty,
                    range,
                    ConstraintOrigin::General,
                ));
            }
        }

        // Destructuring bindings (`let [a, b] = expr` or `let {a, b} = expr`).
        //
        // After HIR lowering both patterns produce PatternVariable descendants, but
        // their structure differs:
        //   Array `let [a, b]`: DestructuringBinding → Pattern → Pattern("a") → PatternVariable("a")
        //   Dict  `let {a, b}`: DestructuringBinding → Pattern → PatternVariable("a")
        //
        // The outer Pattern's direct children reveal the kind:
        //   - direct PatternVariable children → dict pattern
        //   - direct Pattern children         → array pattern
        SymbolKind::DestructuringBinding => {
            let children = get_children(children_index, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            // Determine pattern kind from the `is_dict` flag stored in the outer Pattern symbol.
            let outer_pattern_id = children.first().copied();
            let is_dict_pattern = outer_pattern_id
                .and_then(|pid| hir.symbol(pid))
                .is_some_and(|s| matches!(s.kind, SymbolKind::Pattern { is_dict: true }));

            if is_dict_pattern {
                // Dict pattern: constrain binding to the initializer.
                // When the initializer is a literal Dict, wire each PatternVariable to
                // the corresponding field's value type so `a + true` is caught when `a: number`.
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));

                if let Some(&init_id) = children.last() {
                    let init_ty = ctx.get_or_create_symbol_type(init_id);
                    ctx.add_constraint(Constraint::Equal(
                        Type::Var(ty_var),
                        init_ty,
                        range,
                        ConstraintOrigin::General,
                    ));

                    // For literal Dict initializers, look up per-field types
                    if let Some(init_sym) = hir.symbol(init_id)
                        && matches!(init_sym.kind, SymbolKind::Dict)
                    {
                        // Build key → value_type map from the Dict's key symbols
                        let key_to_val_ty: std::collections::HashMap<String, Type> =
                            get_children(children_index, init_id)
                                .iter()
                                .filter_map(|&key_id| {
                                    let key_name = hir.symbol(key_id)?.value.as_deref()?.to_string();
                                    let val_children = get_children(children_index, key_id);
                                    let val_ty = val_children
                                        .last()
                                        .map(|&vid| ctx.get_or_create_symbol_type(vid))
                                        .unwrap_or_else(|| Type::Var(ctx.fresh_var()));
                                    Some((key_name, val_ty))
                                })
                                .collect();

                        if let Some(pid) = outer_pattern_id {
                            for &child_id in get_children(children_index, pid) {
                                if let Some(child_sym) = hir.symbol(child_id) {
                                    match &child_sym.kind {
                                        // Shorthand `{a}`: PatternVariable is a direct child;
                                        // its value is both the key and the binding name.
                                        SymbolKind::PatternVariable { .. } => {
                                            if let Some(key) = child_sym.value.as_deref()
                                                && let Some(val_ty) = key_to_val_ty.get(key)
                                            {
                                                let pv_ty = ctx.get_or_create_symbol_type(child_id);
                                                ctx.add_constraint(Constraint::Equal(
                                                    pv_ty,
                                                    val_ty.clone(),
                                                    range,
                                                    ConstraintOrigin::General,
                                                ));
                                            }
                                        }
                                        // Explicit `{key: pattern}`: the inner Pattern's `value`
                                        // holds the dict key name (set during HIR lowering).
                                        // Constrain all PatternVariables under it.
                                        SymbolKind::Pattern { .. } => {
                                            if let Some(key) = child_sym.value.as_deref()
                                                && let Some(val_ty) = key_to_val_ty.get(key)
                                            {
                                                for &pv_id in get_children(children_index, child_id) {
                                                    if hir.symbol(pv_id).is_some_and(|s| {
                                                        matches!(s.kind, SymbolKind::PatternVariable { .. })
                                                    }) {
                                                        let pv_ty = ctx.get_or_create_symbol_type(pv_id);
                                                        ctx.add_constraint(Constraint::Equal(
                                                            pv_ty,
                                                            val_ty.clone(),
                                                            range,
                                                            ConstraintOrigin::General,
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // Array pattern: binding type is Array(elem_ty); each PatternVariable
                // is constrained to elem_ty so that `a + true` is flagged when `a: Number`.
                let elem_ty_var = ctx.fresh_var();
                let array_ty = Type::array(Type::Var(elem_ty_var));
                ctx.set_symbol_type(symbol_id, array_ty.clone());

                if let Some(&last_child) = children.last() {
                    let init_ty = ctx.get_or_create_symbol_type(last_child);
                    ctx.add_constraint(Constraint::Equal(array_ty, init_ty, range, ConstraintOrigin::General));
                }

                let range = get_symbol_range(hir, symbol_id);
                let pv_ids = collect_pattern_variable_descendants(hir, symbol_id, children_index);
                for pv_id in pv_ids {
                    let pv_ty = ctx.get_or_create_symbol_type(pv_id);
                    // Rest bindings (`..rest`) capture the remaining elements as an array,
                    // so their type is `Array(elem_ty)` rather than `elem_ty`.
                    let target_ty = if hir
                        .symbol(pv_id)
                        .is_some_and(|s| matches!(s.kind, SymbolKind::PatternVariable { is_rest: true }))
                    {
                        Type::array(Type::Var(elem_ty_var))
                    } else {
                        Type::Var(elem_ty_var)
                    };
                    ctx.add_constraint(Constraint::Equal(pv_ty, target_ty, range, ConstraintOrigin::General));
                }
            }
        }

        // Function definitions
        SymbolKind::Function(params) => {
            // Create type variables for each parameter
            let param_tys: Vec<Type> = params.iter().map(|_| Type::Var(ctx.fresh_var())).collect();

            // Create type variable for return type
            let ret_ty = Type::Var(ctx.fresh_var());

            // Function type is (param_tys) -> ret_ty
            let func_ty = Type::function(param_tys.clone(), ret_ty.clone());
            ctx.set_symbol_type(symbol_id, func_ty);

            // Bind parameter types to their parameter symbols
            let children = get_children(children_index, symbol_id);
            let param_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            for (i, (param_sym, param_ty)) in param_children.iter().zip(param_tys.iter()).enumerate() {
                let sym_ty = ctx.get_or_create_symbol_type(*param_sym);
                let range = get_symbol_range(hir, *param_sym);
                ctx.add_constraint(Constraint::Equal(
                    sym_ty,
                    param_ty.clone(),
                    range,
                    ConstraintOrigin::General,
                ));

                // Connect default parameter values to their parameter types.
                // In HIR, parameters with defaults have `has_default: true` and the
                // default value expression appears as the next non-parameter, non-keyword
                // child after the parameter in the children list.
                if i < params.len() && params[i].has_default {
                    // Find the default value expression: it follows the parameter in the children
                    // list and is not a Parameter or Keyword
                    let param_pos = children.iter().position(|&c| c == *param_sym);
                    if let Some(pos) = param_pos {
                        // Look at the next child after this parameter
                        if let Some(&default_id) = children.get(pos + 1)
                            && let Some(default_sym) = hir.symbol(default_id)
                            && !matches!(default_sym.kind, SymbolKind::Parameter | SymbolKind::Keyword)
                        {
                            let default_ty = ctx.get_or_create_symbol_type(default_id);
                            ctx.add_constraint(Constraint::Equal(
                                param_ty.clone(),
                                default_ty,
                                range,
                                ConstraintOrigin::General,
                            ));
                        }
                    }
                }
            }

            // Connect function body's type to the return type
            // The body is the last non-parameter child
            let body_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| !matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            if let Some(&last_body) = body_children.last() {
                let body_ty = ctx.get_or_create_symbol_type(last_body);
                let range = get_symbol_range(hir, symbol_id);
                let fn_name = hir.symbol(symbol_id).and_then(|s| s.value.clone()).unwrap_or_default();
                ctx.add_constraint(Constraint::Equal(
                    ret_ty,
                    body_ty,
                    range,
                    ConstraintOrigin::ReturnType {
                        fn_name: SmolStr::new(fn_name.as_str()),
                    },
                ));
            }
        }

        // References should unify with their definition
        SymbolKind::Ref => {
            if let Some(def_id) = hir.resolve_reference_symbol(symbol_id) {
                // Check if the reference is to a builtin function
                if let Some(symbol) = hir.symbol(def_id)
                    && let Some(name) = &symbol.value
                {
                    // Only treat as builtin if the resolved definition is actually a builtin symbol,
                    // not a user-defined parameter/variable that happens to share a builtin name
                    let has_builtin =
                        hir.is_builtin_symbol(symbol) && ctx.get_builtin_overloads(name.as_str()).is_some();
                    if has_builtin {
                        // Skip piped input handling for foreach iterable Refs.
                        // In HIR, `foreach (var, iterable)` stores the iterable as a Ref
                        // sibling of the Foreach symbol. These Refs should not be called
                        // with piped input since they reference the iterable function.
                        if is_foreach_iterable_ref(hir, symbol_id) {
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            return;
                        }

                        // If there's a piped input, treat this as a call with the piped value
                        if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                            // Resolve the piped type through substitutions before overload resolution,
                            // so that bound type variables are replaced with their concrete types.
                            let resolved_piped = ctx.resolve_type(&piped_ty);

                            // If the piped input is still a type variable, defer overload resolution
                            // to avoid committing to a wrong overload when multiple are available.
                            if resolved_piped.is_var() {
                                let overload_count =
                                    ctx.get_builtin_overloads(name.as_str()).map(|o| o.len()).unwrap_or(0);
                                if overload_count > 1 {
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                    ctx.add_deferred_overload(DeferredOverload {
                                        symbol_id,
                                        op_name: SmolStr::new(name.as_str()),
                                        operand_tys: vec![piped_ty],
                                        range: get_symbol_range(hir, symbol_id),
                                    });
                                    return;
                                }
                            }

                            let arg_tys = vec![resolved_piped];
                            if let Some(resolved_ty) = ctx.resolve_overload(name.as_str(), &arg_tys) {
                                if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                    let range = get_symbol_range(hir, symbol_id);
                                    for (arg_ty, param_ty) in [piped_ty].iter().zip(param_tys.iter()) {
                                        ctx.add_constraint(Constraint::Equal(
                                            arg_ty.clone(),
                                            param_ty.clone(),
                                            range,
                                            ConstraintOrigin::PipedInput {
                                                fn_name: SmolStr::new(name.as_str()),
                                            },
                                        ));
                                    }
                                    ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());
                                    return;
                                }
                            } else {
                                let range = get_symbol_range(hir, symbol_id);
                                ctx.report_no_matching_overload(name.as_str(), &arg_tys, range);
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                return;
                            }
                        }

                        let overload_count = ctx.get_builtin_overloads(name.as_str()).map(|o| o.len()).unwrap_or(0);

                        // For builtin functions with overloads, we need to handle them specially
                        // For now, we'll create a fresh type variable that will be resolved
                        // during call resolution
                        let ref_ty = ctx.get_or_create_symbol_type(symbol_id);

                        // If there's only one overload, use it directly
                        if overload_count == 1 {
                            let builtin_ty = ctx.get_builtin_overloads(name.as_str()).unwrap()[0].clone();
                            let range = get_symbol_range(hir, symbol_id);
                            ctx.add_constraint(Constraint::Equal(ref_ty, builtin_ty, range, ConstraintOrigin::General));
                        }
                        // For multiple overloads, the type will be resolved at the call site
                        return;
                    }
                }

                // Collect any child Selector symbols for variable attribute access,
                // e.g. `md.depth` where `md` is an Ident with a Selector child.
                let child_selectors: Vec<SymbolId> = get_children(children_index, symbol_id)
                    .iter()
                    .copied()
                    .filter(|&child_id| {
                        hir.symbol(child_id)
                            .map(|s| matches!(s.kind, SymbolKind::Selector(_)))
                            .unwrap_or(false)
                    })
                    .collect();

                let def_ty = ctx.get_or_create_symbol_type(def_id);
                // Instantiate fresh type variables for function references to enable
                // polymorphic use at different call/reference sites
                let def_ty = if matches!(def_ty, Type::Function(_, _)) {
                    ctx.instantiate_fresh(&def_ty)
                } else {
                    def_ty
                };
                let range = get_symbol_range(hir, symbol_id);

                if child_selectors.is_empty() {
                    // Normal reference resolution: the Ref's stored type = def type
                    let ref_ty = ctx.get_or_create_symbol_type(symbol_id);
                    ctx.add_constraint(Constraint::Equal(ref_ty, def_ty, range, ConstraintOrigin::General));
                } else {
                    // Variable attribute access (e.g. `md.depth`):
                    // Use a fresh lookup_var for the def-type equality so it doesn't
                    // conflict with the symbol's stored type, which will be overwritten
                    // with the child selector's output type (e.g. Number for `.depth`).
                    // This prevents a spurious "Number = Markdown" unification error on
                    // the second pass when `get_or_create_symbol_type(symbol_id)` would
                    // return the already-overwritten Number type.
                    let lookup_var = Type::Var(ctx.fresh_var());
                    ctx.add_constraint(Constraint::Equal(
                        lookup_var.clone(),
                        def_ty,
                        range,
                        ConstraintOrigin::General,
                    ));

                    ctx.set_piped_input(child_selectors[0], lookup_var);
                    for i in 0..child_selectors.len() {
                        if let Some(child_sym) = hir.symbol(child_selectors[i]) {
                            generate_symbol_constraints(
                                hir,
                                child_selectors[i],
                                child_sym.kind.clone(),
                                ctx,
                                children_index,
                            );
                        }
                        if i + 1 < child_selectors.len() {
                            let next_ty = ctx.get_or_create_symbol_type(child_selectors[i]);
                            ctx.set_piped_input(child_selectors[i + 1], next_ty);
                        }
                    }
                    // The Ref's final type is the last child selector's output type.
                    let last_ty = ctx.get_or_create_symbol_type(*child_selectors.last().unwrap());
                    ctx.set_symbol_type(symbol_id, last_ty);
                }
            } else {
                // No HIR resolution — try builtin registry by name as fallback
                if let Some(symbol) = hir.symbol(symbol_id)
                    && let Some(name) = &symbol.value
                    && ctx.get_builtin_overloads(name.as_str()).is_some()
                {
                    if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                        let arg_tys = vec![piped_ty];
                        if let Some(Type::Function(param_tys, ret_ty)) = ctx.resolve_overload(name.as_str(), &arg_tys) {
                            let range = get_symbol_range(hir, symbol_id);
                            for (arg_ty, param_ty) in arg_tys.iter().zip(param_tys.iter()) {
                                ctx.add_constraint(Constraint::Equal(
                                    arg_ty.clone(),
                                    param_ty.clone(),
                                    range,
                                    ConstraintOrigin::General,
                                ));
                            }
                            ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());
                            return;
                        }
                    }

                    let overload_count = ctx.get_builtin_overloads(name.as_str()).map(|o| o.len()).unwrap_or(0);
                    let ref_ty = ctx.get_or_create_symbol_type(symbol_id);
                    if overload_count == 1 {
                        let builtin_ty = ctx.get_builtin_overloads(name.as_str()).unwrap()[0].clone();
                        let range = get_symbol_range(hir, symbol_id);
                        ctx.add_constraint(Constraint::Equal(ref_ty, builtin_ty, range, ConstraintOrigin::General));
                    }
                }
            }
        }

        // Binary operators
        SymbolKind::BinaryOp => {
            if let Some(symbol) = hir.symbol(symbol_id) {
                if let Some(op_name) = &symbol.value {
                    // Get left and right operands
                    let children = get_children(children_index, symbol_id);
                    if children.len() >= 2 {
                        let left_ty = ctx.get_or_create_symbol_type(children[0]);
                        let right_ty = ctx.get_or_create_symbol_type(children[1]);
                        let range = get_symbol_range(hir, symbol_id);

                        // Resolve types to get their concrete values if already determined
                        let resolved_left = ctx.resolve_type(&left_ty);
                        let resolved_right = ctx.resolve_type(&right_ty);

                        // Check if any operand is a union type
                        let has_union = resolved_left.is_union() || resolved_right.is_union();

                        // If any operand is still a type variable, defer overload resolution
                        // until after the first round of unification when types may be known
                        if resolved_left.is_var() || resolved_right.is_var() {
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            ctx.add_deferred_overload(DeferredOverload {
                                symbol_id,
                                op_name: SmolStr::new(op_name.as_str()),
                                operand_tys: vec![left_ty, right_ty],
                                range,
                            });
                        } else if has_union {
                            // Defer — Union operands are resolved in `resolve_deferred_overloads`
                            // where `union_members_consistent_return` can check all members.
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            ctx.add_deferred_overload(DeferredOverload {
                                symbol_id,
                                op_name: SmolStr::new(op_name.as_str()),
                                operand_tys: vec![left_ty, right_ty],
                                range,
                            });
                        } else {
                            // Try to resolve the best matching overload
                            let arg_types = vec![resolved_left.clone(), resolved_right.clone()];
                            if let Some(resolved_ty) = ctx.resolve_overload(op_name.as_str(), &arg_types) {
                                // resolved_ty is the matched function type: (T1, T2) -> T3
                                if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                    if param_tys.len() == 2 {
                                        ctx.add_constraint(Constraint::Equal(
                                            left_ty,
                                            param_tys[0].clone(),
                                            range,
                                            ConstraintOrigin::Operator {
                                                op: SmolStr::new(op_name.as_str()),
                                            },
                                        ));
                                        ctx.add_constraint(Constraint::Equal(
                                            right_ty,
                                            param_tys[1].clone(),
                                            range,
                                            ConstraintOrigin::Operator {
                                                op: SmolStr::new(op_name.as_str()),
                                            },
                                        ));
                                        ctx.set_symbol_type(symbol_id, *ret_ty);
                                    } else {
                                        let ty_var = ctx.fresh_var();
                                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                    }
                                } else {
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                }
                            } else {
                                // No matching overload found - collect error
                                ctx.report_no_matching_overload(op_name, &[resolved_left, resolved_right], range);
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            }
                        }
                    } else {
                        // Not enough operands
                        let ty_var = ctx.fresh_var();
                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                    }
                } else {
                    // No operator name
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            }
        }

        // Assignments (e.g., `x = 10`, `x += 1`)
        SymbolKind::Assign => {
            if let Some(symbol) = hir.symbol(symbol_id) {
                let children = get_children(children_index, symbol_id);
                if children.len() >= 2 {
                    let lhs_id = children[0]; // Ref to the variable
                    let rhs_id = children[1]; // Value expression
                    let rhs_ty = ctx.get_or_create_symbol_type(rhs_id);
                    let range = get_symbol_range(hir, symbol_id);

                    // Resolve the LHS Ref to find the original Variable
                    if let Some(var_id) = hir.resolve_reference_symbol(lhs_id) {
                        let op_name = symbol.value.as_deref().unwrap_or("=");

                        if op_name == "=" {
                            // Plain assignment: variable takes the RHS type.
                            // Use set_symbol_type_no_bind to avoid cascading the old type
                            // variable binding (which would conflict with the initializer
                            // constraint, e.g., `var x = 10 | x = "hello"` would fail
                            // because tv1→Number and tv1→String would conflict).
                            let new_var_ty = ctx.fresh_var();
                            ctx.set_symbol_type_no_bind(var_id, Type::Var(new_var_ty));
                            let var_name = hir.symbol(lhs_id).and_then(|s| s.value.clone()).unwrap_or_default();
                            ctx.add_constraint(Constraint::Equal(
                                Type::Var(new_var_ty),
                                rhs_ty.clone(),
                                range,
                                ConstraintOrigin::Assignment {
                                    var_name: SmolStr::new(var_name.as_str()),
                                },
                            ));

                            // No need to re-bind subsequent Refs — Assigns are processed
                            // in Pass 2.5 (before Refs/Calls in Pass 3), so Refs will
                            // pick up the updated variable type when they are processed.
                        } else {
                            // Compound assignment (+=, -=, etc.)
                            let base_op = match op_name {
                                "+=" => "+",
                                "-=" => "-",
                                "*=" => "*",
                                "/=" => "/",
                                "%=" => "%",
                                "//=" => "//",
                                _ => op_name,
                            };
                            let current_var_ty = ctx.get_or_create_symbol_type(var_id);
                            let resolved_left = ctx.resolve_type(&current_var_ty);
                            let resolved_right = ctx.resolve_type(&rhs_ty);

                            if resolved_left.is_var() || resolved_right.is_var() {
                                // Defer if types not yet known
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                ctx.add_deferred_overload(DeferredOverload {
                                    symbol_id,
                                    op_name: SmolStr::new(base_op),
                                    operand_tys: vec![current_var_ty, rhs_ty],
                                    range,
                                });
                            } else {
                                let arg_types = vec![resolved_left.clone(), resolved_right.clone()];
                                if let Some(resolved_ty) = ctx.resolve_overload(base_op, &arg_types)
                                    && let Type::Function(param_tys, ret_ty) = resolved_ty
                                    && param_tys.len() == 2
                                {
                                    ctx.add_constraint(Constraint::Equal(
                                        current_var_ty,
                                        param_tys[0].clone(),
                                        range,
                                        ConstraintOrigin::General,
                                    ));
                                    ctx.add_constraint(Constraint::Equal(
                                        rhs_ty,
                                        param_tys[1].clone(),
                                        range,
                                        ConstraintOrigin::General,
                                    ));
                                    // Update variable type to result
                                    ctx.set_symbol_type(var_id, *ret_ty);
                                } else if ctx.resolve_overload(base_op, &arg_types).is_none() {
                                    ctx.report_no_matching_overload(base_op, &[resolved_left, resolved_right], range);
                                }
                            }
                        }
                    }

                    // The Assign expression itself evaluates to the piped input
                    // (consistent with runtime which returns the input value)
                    if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                        ctx.set_symbol_type(symbol_id, piped_ty);
                    } else {
                        let ty_var = ctx.fresh_var();
                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            }
        }

        // Unary operators
        SymbolKind::UnaryOp => {
            if let Some(symbol) = hir.symbol(symbol_id) {
                if let Some(op_name) = &symbol.value {
                    let children = get_children(children_index, symbol_id);
                    if !children.is_empty() {
                        let operand_ty = ctx.get_or_create_symbol_type(children[0]);
                        let range = get_symbol_range(hir, symbol_id);

                        // Resolve type to get its concrete value if already determined
                        let resolved_operand = ctx.resolve_type(&operand_ty);

                        // If the operand is still a type variable, defer overload resolution
                        if resolved_operand.is_var() {
                            let ty_var = ctx.fresh_var();
                            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            ctx.add_deferred_overload(DeferredOverload {
                                symbol_id,
                                op_name: SmolStr::new(op_name.as_str()),
                                operand_tys: vec![operand_ty],
                                range,
                            });
                        } else {
                            // Try to resolve the best matching overload
                            let arg_types = vec![resolved_operand.clone()];
                            if let Some(resolved_ty) = ctx.resolve_overload(op_name.as_str(), &arg_types) {
                                if let Type::Function(param_tys, ret_ty) = resolved_ty {
                                    if param_tys.len() == 1 {
                                        ctx.add_constraint(Constraint::Equal(
                                            operand_ty,
                                            param_tys[0].clone(),
                                            range,
                                            ConstraintOrigin::Operator {
                                                op: SmolStr::new(op_name.as_str()),
                                            },
                                        ));
                                        ctx.set_symbol_type(symbol_id, *ret_ty);
                                    } else {
                                        let ty_var = ctx.fresh_var();
                                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                    }
                                } else {
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                }
                            } else {
                                // No matching overload found - collect error
                                ctx.report_no_matching_overload(op_name, &[resolved_operand], range);
                                let ty_var = ctx.fresh_var();
                                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                            }
                        }
                    } else {
                        let ty_var = ctx.fresh_var();
                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            }
        }

        // Function calls
        SymbolKind::Call => {
            // Get the function name from the Call symbol itself
            if let Some(call_symbol) = hir.symbol(symbol_id) {
                if let Some(func_name) = &call_symbol.value {
                    // All children are explicit arguments (filter out Keyword symbols
                    // which are syntax elements like `fn` in lambda expressions)
                    let children = get_non_keyword_children(hir, symbol_id, children_index);
                    let explicit_arg_tys: Vec<Type> = children
                        .iter()
                        .map(|&arg_id| ctx.get_or_create_symbol_type(arg_id))
                        .collect();

                    let range = get_symbol_range(hir, symbol_id);

                    // Try user-defined function first (via HIR reference resolution)
                    if let Some(def_id) = hir.resolve_reference_symbol(symbol_id) {
                        let def_symbol = hir.symbol(def_id);
                        let is_user_defined = def_symbol.map(|s| !hir.is_builtin_symbol(s)).unwrap_or(false);

                        if is_user_defined {
                            // When `def_id` is a Variable holding a lambda (e.g. `let f = fn(x): x - 1;`),
                            // the Variable's type is still an unresolved type variable at this point in
                            // constraint generation (before unification). Instead, look up the lambda
                            // Function child directly and use its already-established Function type.
                            // This ensures a proper `DeferredUserCall` is created with the lambda's
                            // SymbolId as `def_id`, enabling call-site argument type checking for lambdas.
                            let (effective_def_id, original_func_ty) =
                                if def_symbol.map(|s| s.is_variable()).unwrap_or(false) {
                                    if let Some(lambda_id) = find_lambda_function_child(hir, def_id, children_index) {
                                        let lambda_ty = ctx.get_or_create_symbol_type(lambda_id);
                                        (lambda_id, lambda_ty)
                                    } else {
                                        (def_id, ctx.get_or_create_symbol_type(def_id))
                                    }
                                } else {
                                    (def_id, ctx.get_or_create_symbol_type(def_id))
                                };
                            // Instantiate fresh type variables so each call site is independent
                            let func_ty = ctx.instantiate_fresh(&original_func_ty);
                            let def_id = effective_def_id;

                            if let Type::Function(param_tys, ret_ty) = &func_ty {
                                // Try piped input if explicit args don't match arity
                                let arg_tys = if param_tys.len() != explicit_arg_tys.len() {
                                    if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                                        let mut piped_args = vec![piped_ty];
                                        piped_args.extend(explicit_arg_tys.iter().cloned());
                                        piped_args
                                    } else {
                                        explicit_arg_tys.clone()
                                    }
                                } else {
                                    explicit_arg_tys.clone()
                                };

                                // Check arity
                                if param_tys.len() != arg_tys.len() {
                                    // If arity doesn't match and the call might receive piped input later
                                    // (inside a Block), defer the error until Pass 4 re-processing
                                    if !might_receive_piped_input(hir, symbol_id) {
                                        ctx.add_error(TypeError::WrongArity {
                                            expected: param_tys.len(),
                                            found: arg_tys.len(),
                                            span: range.as_ref().map(range_to_span),
                                            location: range,
                                            context: None,
                                        });
                                    }
                                } else {
                                    // Unify argument types with parameter types
                                    for (arg_ty, param_ty) in arg_tys.iter().zip(param_tys.iter()) {
                                        ctx.add_constraint(Constraint::Equal(
                                            arg_ty.clone(),
                                            param_ty.clone(),
                                            range,
                                            ConstraintOrigin::General,
                                        ));
                                    }
                                }
                                ctx.set_symbol_type(symbol_id, ret_ty.as_ref().clone());

                                // Track this call for post-unification resolution.
                                // After unification, the original function's return type
                                // will be concrete, allowing propagation to this call site.
                                // arg_symbol_ids records the HIR symbol IDs of the arguments
                                // so that lambda body operators can be checked later.
                                let arg_symbol_ids = if param_tys.len() == children.len() {
                                    children.clone()
                                } else {
                                    // piped input was prepended — include a placeholder
                                    let mut ids = vec![symbol_id]; // placeholder for piped arg
                                    ids.extend_from_slice(&children);
                                    ids
                                };
                                ctx.add_deferred_user_call(DeferredUserCall {
                                    call_symbol_id: symbol_id,
                                    def_id,
                                    fresh_param_tys: param_tys.clone(),
                                    fresh_ret_ty: ret_ty.as_ref().clone(),
                                    arg_tys,
                                    arg_symbol_ids,
                                    range,
                                });
                            } else {
                                // Check for potential Record field access via bracket notation
                                // (e.g., v[:key]). Only trigger when the argument is a
                                // Symbol/Selector (`:key` pattern), not a regular variable ref.
                                let field_name = children.first().and_then(|&arg_id| {
                                    let arg_symbol = hir.symbol(arg_id)?;
                                    if matches!(
                                        arg_symbol.kind,
                                        SymbolKind::Symbol | SymbolKind::Selector(_) | SymbolKind::String
                                    ) {
                                        arg_symbol.value.as_ref().map(|v| v.to_string())
                                    } else {
                                        None
                                    }
                                });

                                if let Some(name) = field_name {
                                    // Defer field access resolution to post-unification
                                    ctx.add_deferred_record_access(infer::DeferredRecordAccess {
                                        call_symbol_id: symbol_id,
                                        def_id,
                                        field_name: name,
                                        range: get_symbol_range(hir, symbol_id),
                                    });
                                    let ty_var = ctx.fresh_var();
                                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                                } else {
                                    // The definition exists but isn't a function type yet.
                                    // If the definition is a function parameter (higher-order call),
                                    // record the inner call so that `check_user_call_body_operators`
                                    // can propagate the concrete element type to the lambda's body.
                                    if def_symbol.map(|s| s.is_parameter()).unwrap_or(false) {
                                        let ret_ty = Type::Var(ctx.fresh_var());
                                        let expected_func_ty = Type::function(explicit_arg_tys.clone(), ret_ty.clone());
                                        ctx.add_constraint(Constraint::Equal(
                                            func_ty,
                                            expected_func_ty,
                                            range,
                                            ConstraintOrigin::General,
                                        ));
                                        ctx.set_symbol_type(symbol_id, ret_ty);
                                        if let Some(outer_def_id) = find_enclosing_function(hir, symbol_id) {
                                            ctx.add_deferred_parameter_call(DeferredParameterCall {
                                                outer_def_id,
                                                param_sym_id: def_id,
                                                arg_tys: explicit_arg_tys.clone(),
                                            });
                                        }
                                    } else {
                                        // Non-function variable with bracket access (e.g., v[0]).
                                        // Always defer index access resolution to post-unification.
                                        // This handles both Array and Tuple types correctly:
                                        // - For Tuple (heterogeneous arrays), per-element types are preserved
                                        // - For Array, element type is used as before
                                        // - For unresolved vars, the fallback adds an Array structural constraint
                                        // This avoids false positives where different elements of a heterogeneous
                                        // array share the same type variable and get incorrectly unified.
                                        let literal_index = children.first().and_then(|&arg_id| {
                                            let arg_sym = hir.symbol(arg_id)?;
                                            if matches!(arg_sym.kind, SymbolKind::Number) {
                                                arg_sym.value.as_ref()?.parse::<usize>().ok()
                                            } else {
                                                None
                                            }
                                        });

                                        // Only constrain the index to Number when it is a literal
                                        // numeric index (e.g. v[0]). For variable indices
                                        // (e.g. a String key used for Dict access), skip this
                                        // constraint — the correct element type is resolved via
                                        // resolve_deferred_tuple_accesses once the container's
                                        // type is known.
                                        if literal_index.is_some()
                                            && let Some(index_ty) = explicit_arg_tys.first()
                                        {
                                            ctx.add_constraint(Constraint::Equal(
                                                index_ty.clone(),
                                                Type::Number,
                                                range,
                                                ConstraintOrigin::General,
                                            ));
                                        }

                                        let ty_var = ctx.fresh_var();
                                        ctx.set_symbol_type(symbol_id, Type::Var(ty_var));

                                        ctx.add_deferred_tuple_access(infer::DeferredTupleAccess {
                                            call_symbol_id: symbol_id,
                                            def_id,
                                            index: literal_index,
                                            range: get_symbol_range(hir, symbol_id),
                                        });
                                    }
                                }
                            }
                        } else {
                            // Resolved to a builtin - handle via overload resolution
                            // If there's piped input, prepend it as the implicit first argument
                            let arg_tys = build_piped_call_args(ctx, symbol_id, &explicit_arg_tys, func_name);
                            // Defer error if call might receive piped input later (inside a Block)
                            // or if it is inside a quote block (template code not directly executed).
                            let defer =
                                might_receive_piped_input(hir, symbol_id) || is_inside_quote_block(hir, symbol_id);
                            resolve_builtin_call(ctx, symbol_id, func_name, &arg_tys, range, defer);
                        }
                    } else {
                        // No HIR resolution - try builtin overload resolution
                        // If there's piped input, prepend it as the implicit first argument
                        let arg_tys = build_piped_call_args(ctx, symbol_id, &explicit_arg_tys, func_name);
                        let defer = might_receive_piped_input(hir, symbol_id) || is_inside_quote_block(hir, symbol_id);
                        resolve_builtin_call(ctx, symbol_id, func_name, &arg_tys, range, defer);
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Collections
        SymbolKind::Array => {
            // Array elements should have consistent types when possible.
            // mq is dynamically typed and allows heterogeneous arrays (e.g., [string, number]
            // used as tuples). When elements have different concrete types, we skip
            // unification to avoid cascading false-positive type errors.
            let children = get_children(children_index, symbol_id);
            if children.is_empty() {
                // Empty array - element type is a fresh type variable
                let elem_ty_var = ctx.fresh_var();
                let array_ty = Type::array(Type::Var(elem_ty_var));
                ctx.set_symbol_type(symbol_id, array_ty);
            } else {
                // Get types of all elements
                let elem_tys: Vec<Type> = children
                    .iter()
                    .map(|&child_id| ctx.get_or_create_symbol_type(child_id))
                    .collect();

                // Resolve element types to check for heterogeneous concrete types
                let resolved_tys: Vec<Type> = elem_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
                let concrete_tys: Vec<&Type> = resolved_tys.iter().filter(|ty| !ty.is_var()).collect();

                // Check if concrete types are all the same (homogeneous)
                let is_heterogeneous = concrete_tys.len() >= 2
                    && concrete_tys
                        .windows(2)
                        .any(|w| std::mem::discriminant(w[0]) != std::mem::discriminant(w[1]));

                // Use Tuple only when there are multiple elements with mixed resolved/unresolved
                // types, or when the elements are heterogeneous. A single-element array [x] where
                // x is still a type variable should be Array(Var), not Tuple(Var), so that array
                // operations like `[x] + [y]` and `arr + [x]` resolve correctly via Array overloads.
                let needs_tuple = is_heterogeneous || (elem_tys.len() >= 2 && concrete_tys.len() != elem_tys.len());

                if needs_tuple {
                    // In strict array mode, report heterogeneous arrays as errors
                    if is_heterogeneous && ctx.strict_array() {
                        let range = get_symbol_range(hir, symbol_id);
                        let types_str = concrete_tys
                            .iter()
                            .map(|ty| ty.display_renumbered())
                            .collect::<Vec<_>>()
                            .join(", ");
                        ctx.add_error(TypeError::HeterogeneousArray {
                            types: types_str,
                            span: range.as_ref().map(range_to_span),
                            location: range,
                        });
                    }

                    // Always use Tuple to preserve per-element type information.
                    // When elements have different concrete types (heterogeneous) or some
                    // elements are still type variables (partially-resolved), a single
                    // shared element type variable would be incorrectly unified with
                    // incompatible types (e.g., both String from result[0] and Bool from
                    // result[1] if the same Var is used for all elements).
                    let tuple_ty = Type::tuple(elem_tys);
                    ctx.set_symbol_type(symbol_id, tuple_ty);
                } else {
                    // Homogeneous or unresolved — unify all element types
                    let elem_ty = elem_tys[0].clone();
                    let range = get_symbol_range(hir, symbol_id);
                    for ty in &elem_tys[1..] {
                        ctx.add_constraint(Constraint::Equal(
                            elem_ty.clone(),
                            ty.clone(),
                            range,
                            ConstraintOrigin::General,
                        ));
                    }

                    let array_ty = Type::array(elem_ty);
                    ctx.set_symbol_type(symbol_id, array_ty);
                }
            }
        }

        SymbolKind::Dict => {
            // Dict structure in HIR: Dict -> key_symbol -> value_expr
            // Direct children of Dict are the key symbols.
            // When all keys are string literals, use Record type (row polymorphism)
            // to track per-field value types.
            let key_symbols = get_children(children_index, symbol_id);
            if key_symbols.is_empty() {
                // Empty dict → open record with fresh row variable
                let row_var = ctx.fresh_var();
                let record_ty = Type::record(std::collections::BTreeMap::new(), Type::Var(row_var));
                ctx.set_symbol_type(symbol_id, record_ty);
            } else {
                // Try to build a Record type from known keys (string literals or symbols)
                let mut fields = std::collections::BTreeMap::new();
                let mut all_string_keys = true;

                for &key_id in key_symbols {
                    let symbol = hir.symbol(key_id);
                    let key_name = symbol.and_then(|s| s.value.as_ref().map(|v| v.to_string()));
                    let key_kind = symbol.map(|s| s.kind.clone());

                    if let Some(name) = key_name {
                        // Get value type from the key's children
                        let value_children = get_children(children_index, key_id);
                        let val_ty = if let Some(&val_id) = value_children.last() {
                            ctx.get_or_create_symbol_type(val_id)
                        } else {
                            Type::Var(ctx.fresh_var())
                        };
                        // Assign appropriate type to the key symbol
                        let key_ty = if key_kind == Some(SymbolKind::String) {
                            Type::String
                        } else {
                            Type::Symbol
                        };
                        ctx.set_symbol_type(key_id, key_ty);
                        fields.insert(name, val_ty);
                    } else {
                        all_string_keys = false;
                        break;
                    }
                }

                if all_string_keys {
                    // Closed record: all field keys are statically known
                    let record_ty = Type::record(fields, Type::RowEmpty);
                    ctx.set_symbol_type(symbol_id, record_ty);
                } else {
                    // Fallback: dynamic keys → use Dict type
                    let key_ty_var = ctx.fresh_var();
                    let val_ty_var = ctx.fresh_var();
                    let key_ty = Type::Var(key_ty_var);
                    let range = get_symbol_range(hir, symbol_id);

                    for &key_id in key_symbols {
                        let k_ty = ctx.get_or_create_symbol_type(key_id);
                        ctx.add_constraint(Constraint::Equal(
                            key_ty.clone(),
                            k_ty,
                            range,
                            ConstraintOrigin::General,
                        ));

                        let value_children = get_children(children_index, key_id);
                        for &val_id in value_children {
                            ctx.get_or_create_symbol_type(val_id);
                        }
                    }

                    let dict_ty = Type::dict(key_ty, Type::Var(val_ty_var));
                    ctx.set_symbol_type(symbol_id, dict_ty);
                }
            }
        }

        // Control flow constructs
        SymbolKind::If => {
            let children = get_children(children_index, symbol_id);
            if !children.is_empty() {
                let range = get_symbol_range(hir, symbol_id);

                // First child is the condition — mq is dynamically typed, so any
                // value can be used as a condition (truthy/falsy), not just Bool.
                let _cond_ty = ctx.get_or_create_symbol_type(children[0]);

                // Analyze condition for type narrowing (e.g., is_string(x))
                let cond_narrowings = analyze_condition(hir, children[0], children_index, ctx);
                if !cond_narrowings.is_empty() && children.len() > 1 {
                    let else_branch_ids: Vec<SymbolId> = children[2..].to_vec();
                    ctx.add_type_narrowing(TypeNarrowing {
                        then_narrowings: cond_narrowings.then_narrowings,
                        else_narrowings: cond_narrowings.else_narrowings,
                        then_branch_id: children[1],
                        else_branch_ids,
                    });
                }

                // Subsequent children are then-branch and elif/else branches.
                // mq is dynamically typed: branches may return different types
                // (e.g., `if (...): true elif (...): false else: None`).
                // Use Union types when branches have different concrete types.
                if children.len() > 1 {
                    let then_ty = ctx.get_or_create_symbol_type(children[1]);
                    let mut branch_tys = vec![then_ty.clone()];
                    for &child_id in &children[2..] {
                        let child_ty = resolve_branch_body_type(hir, child_id, ctx, children_index);
                        branch_tys.push(child_ty);
                    }

                    // Check if branches have different concrete types
                    let resolved: Vec<Type> = branch_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
                    let concrete: Vec<&Type> = resolved.iter().filter(|ty| !ty.is_var()).collect();

                    // Check if all concrete types are structurally compatible.
                    // Use `can_branch_unify_with` for strict structural comparison:
                    // Var vs concrete → incompatible, preventing false unification of
                    // branches like Tuple([None,Var]) vs Tuple([Var,Var]) where the Var
                    // might later resolve to an incompatible concrete type (e.g. Number).
                    let all_same = if concrete.len() >= 2 {
                        concrete.windows(2).all(|w| w[0].can_branch_unify_with(w[1]))
                    } else {
                        true
                    };

                    // Check if any branch is None — in mq, `if (...): value else: None`
                    // should not unify `value` with None; instead treat as different types.
                    let has_none_branch = resolved.iter().any(|t| matches!(t, Type::None));

                    // Detect patterns like `if (is_array(x)): x else: [x]` where one branch
                    // is Var(a) and another contains Var(a).  Trying to unify these directly
                    // would trigger an "infinite type" occurs-check error.  Instead, form a
                    // Union so both branches coexist without being forcibly equated.
                    let would_cause_infinite_type = resolved.iter().any(|ty| {
                        if let Type::Var(v) = ty {
                            resolved
                                .iter()
                                .any(|other| !matches!(other, Type::Var(_)) && other.free_vars().contains(v))
                        } else {
                            false
                        }
                    });

                    if would_cause_infinite_type
                        || (!all_same && concrete.len() >= 2)
                        || (has_none_branch && resolved.len() >= 2)
                    {
                        // Different concrete types across branches — use Union type.
                        // Include ALL resolved branch types (vars and concrete) so that
                        // branches whose types are not yet fully resolved can still be
                        // tracked and resolved later (e.g., `if (...): items else: None`
                        // where `items` is still a Var at constraint-generation time).
                        let union_ty = Type::union(resolved.clone());
                        ctx.set_symbol_type(symbol_id, union_ty);
                    } else {
                        // Homogeneous or unresolved — unify all branch types
                        ctx.set_symbol_type(symbol_id, then_ty.clone());
                        for ty in &branch_tys[1..] {
                            ctx.add_constraint(Constraint::Equal(
                                then_ty.clone(),
                                ty.clone(),
                                range,
                                ConstraintOrigin::General,
                            ));
                        }
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Elif | SymbolKind::Else => {
            // Handled inline by the parent If handler via resolve_branch_body_type
            if ctx.get_symbol_type(symbol_id).is_none() {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::While => {
            // While loop: condition must be Bool, result type from body.
            // `break: value` exits with the value's type; multiple break paths may
            // create a union with the normal-exit (body) type.
            let children = get_children(children_index, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            if !children.is_empty() {
                // First child is the condition
                let cond_ty = ctx.get_or_create_symbol_type(children[0]);
                ctx.add_constraint(Constraint::Equal(cond_ty, Type::Bool, range, ConstraintOrigin::General));

                // Analyze condition for type narrowing: narrowings apply inside the loop body
                // (then-branch) and to code after the loop (else-branch, when condition is false).
                if children.len() > 1 {
                    let cond_narrowings = analyze_condition(hir, children[0], children_index, ctx);
                    if !cond_narrowings.is_empty() {
                        // Loop body is the last child; all body children receive then-narrowings.
                        let body_children: Vec<SymbolId> = children[1..].to_vec();
                        for &body_id in &body_children {
                            ctx.add_type_narrowing(TypeNarrowing {
                                then_narrowings: cond_narrowings.then_narrowings.clone(),
                                else_narrowings: Vec::new(),
                                then_branch_id: body_id,
                                else_branch_ids: Vec::new(),
                            });
                        }

                        // Post-loop narrowing: when the while condition becomes false,
                        // apply else_narrowings to siblings that follow the While node
                        // in its parent scope.
                        if !cond_narrowings.else_narrowings.is_empty() {
                            let post_loop_siblings = get_post_loop_siblings(hir, symbol_id, children_index);
                            if !post_loop_siblings.is_empty() {
                                ctx.add_type_narrowing(TypeNarrowing {
                                    then_narrowings: Vec::new(),
                                    else_narrowings: cond_narrowings.else_narrowings,
                                    then_branch_id: symbol_id, // unused (then_narrowings empty)
                                    else_branch_ids: post_loop_siblings,
                                });
                            }
                        }
                    }
                }

                // Result type comes from the body (last child after condition)
                if children.len() > 1 {
                    let body_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
                    let break_tys = collect_break_value_types(hir, symbol_id, ctx, children_index);
                    let loop_ty = merge_loop_types(body_ty, break_tys, ctx);
                    ctx.set_symbol_type(symbol_id, loop_ty);
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Loop => {
            // Infinite loop: result type is determined by `break: value` expressions
            // and/or the last body expression if the loop falls through.
            // Multiple break paths with different types create a union type.
            let children = get_children(children_index, symbol_id);

            if !children.is_empty() {
                let body_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
                let break_tys = collect_break_value_types(hir, symbol_id, ctx, children_index);
                let loop_ty = merge_loop_types(body_ty, break_tys, ctx);
                ctx.set_symbol_type(symbol_id, loop_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Foreach => {
            // Foreach: iterates over an array/string and collects body results into an array.
            // Children: [Variable(item), Ref(iterable), body_expr...]
            // The body expression is the last child (highest SymbolId).
            //
            // When `break: value` is used, foreach exits early returning the value directly
            // (not wrapped in an array).  This creates a union with the normal Array<body>
            // exit type.
            let children = get_children(children_index, symbol_id);

            if !children.is_empty() {
                // Constrain the loop variable type to the element type of the iterable.
                // This is needed for type checking operations on the loop variable,
                // especially when lambdas are passed as higher-order function arguments.
                if children.len() >= 2 {
                    let item_id = children[0]; // Variable (loop item)
                    let iterable_id = children[1]; // Ref or Call (iterable)
                    let item_ty = ctx.get_or_create_symbol_type(item_id);
                    let iterable_ty = ctx.get_or_create_symbol_type(iterable_id);
                    let resolved_iterable = ctx.resolve_type(&iterable_ty);
                    let range = get_symbol_range(hir, symbol_id);

                    match &resolved_iterable {
                        Type::Array(elem) => {
                            // Iterable is a concrete array - directly constrain loop variable
                            ctx.add_constraint(Constraint::Equal(
                                item_ty,
                                *elem.clone(),
                                range,
                                ConstraintOrigin::General,
                            ));
                        }
                        Type::String => {
                            // String iteration yields string characters
                            ctx.add_constraint(Constraint::Equal(
                                item_ty,
                                Type::String,
                                range,
                                ConstraintOrigin::General,
                            ));
                        }
                        Type::Var(_) => {
                            // Unknown iterable type: create fresh element variable,
                            // constrain iterable = Array(elem), and item = elem.
                            // This propagates element types through polymorphic iteration
                            // (e.g. when the iterable is a function parameter).
                            let elem_var = ctx.fresh_var();
                            let elem_ty = Type::Var(elem_var);
                            ctx.add_constraint(Constraint::Equal(
                                iterable_ty,
                                Type::array(elem_ty.clone()),
                                range,
                                ConstraintOrigin::General,
                            ));
                            ctx.add_constraint(Constraint::Equal(item_ty, elem_ty, range, ConstraintOrigin::General));
                        }
                        _ => {
                            // Other types (number, bool, etc.): skip constraint to avoid
                            // false positives for runtime-dynamic iteration.
                        }
                    }
                }

                let body_ty = ctx.get_or_create_symbol_type(*children.last().unwrap());
                let resolved_body = ctx.resolve_type(&body_ty);

                // Normal exit type: Array<body_type>
                let array_ty = if !resolved_body.is_var() {
                    Type::array(resolved_body)
                } else {
                    Type::array(body_ty)
                };

                // Merge with break value types (each break: value returns that value directly)
                let break_tys = collect_break_value_types(hir, symbol_id, ctx, children_index);
                let loop_ty = merge_loop_types(array_ty, break_tys, ctx);
                ctx.set_symbol_type(symbol_id, loop_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::MatchArm | SymbolKind::Pattern { .. } => {
            // These are handled by the Match handler below.
            // Assign a fresh type variable as default.
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        SymbolKind::Match => {
            // All match arms should have the same type
            // Pattern types should be compatible with the match expression type
            let children = get_children(children_index, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            if children.len() > 1 {
                // First child is the match expression, rest are arms.
                let match_expr_ty = ctx.get_or_create_symbol_type(children[0]);
                let arm_children: Vec<_> = children[1..].to_vec();

                // If the match expression is a direct variable reference, we can narrow
                // its type inside each arm body to match the arm's pattern type.
                let match_var_def_id = hir
                    .symbol(children[0])
                    .filter(|s| matches!(s.kind, SymbolKind::Ref))
                    .and_then(|_| hir.resolve_reference_symbol(children[0]));

                if !arm_children.is_empty() {
                    let mut arm_body_tys = Vec::new();

                    for &arm_id in &arm_children {
                        let arm_children_inner = get_children(children_index, arm_id);

                        // Separate pattern and body children, tracking both IDs and types.
                        let mut body_ty = None;
                        let mut body_id = None;
                        let mut arm_pattern_ty = None;
                        for &arm_child_id in arm_children_inner {
                            if let Some(arm_child_symbol) = hir.symbol(arm_child_id) {
                                if matches!(arm_child_symbol.kind, SymbolKind::Pattern { .. }) {
                                    // Determine pattern type from its literal children
                                    let pattern_ty = resolve_pattern_type(hir, arm_child_id, ctx, children_index);
                                    ctx.add_constraint(Constraint::Equal(
                                        match_expr_ty.clone(),
                                        pattern_ty.clone(),
                                        range,
                                        ConstraintOrigin::General,
                                    ));
                                    // Keep the pattern type if it is concrete (not a wildcard Var)
                                    if !pattern_ty.is_var() {
                                        arm_pattern_ty = Some(pattern_ty);
                                    }
                                } else {
                                    // Track the last non-Pattern child as the body
                                    body_ty = Some(ctx.get_or_create_symbol_type(arm_child_id));
                                    body_id = Some(arm_child_id);
                                }
                            }
                        }

                        // Narrow the matched variable to the concrete pattern type inside the arm body.
                        if let (Some(def_id), Some(pat_ty), Some(bid)) = (match_var_def_id, arm_pattern_ty, body_id) {
                            ctx.add_type_narrowing(TypeNarrowing {
                                then_narrowings: vec![NarrowingEntry {
                                    def_id,
                                    narrowed_type: pat_ty,
                                    is_complement: false,
                                }],
                                else_narrowings: Vec::new(),
                                then_branch_id: bid,
                                else_branch_ids: Vec::new(),
                            });
                        }

                        if let Some(body_ty) = body_ty {
                            ctx.set_symbol_type(arm_id, body_ty.clone());
                            arm_body_tys.push(body_ty);
                        } else {
                            let arm_ty = ctx.get_or_create_symbol_type(arm_id);
                            arm_body_tys.push(arm_ty);
                        }
                    }

                    // Check if arms have different concrete types (heterogeneous match)
                    let resolved: Vec<Type> = arm_body_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();
                    let concrete: Vec<&Type> = resolved.iter().filter(|ty| !ty.is_var()).collect();

                    // Check if all concrete types are the same
                    let all_same = if concrete.len() >= 2 {
                        concrete
                            .windows(2)
                            .all(|w| std::mem::discriminant(w[0]) == std::mem::discriminant(w[1]))
                    } else {
                        true
                    };

                    if !all_same && concrete.len() >= 2 {
                        // Different concrete types in arms — use Union type
                        let unique_types: Vec<Type> = concrete.into_iter().cloned().collect();
                        let union_ty = Type::union(unique_types);
                        ctx.set_symbol_type(symbol_id, union_ty);
                    } else {
                        let result_ty_var = ctx.fresh_var();
                        let result_ty = Type::Var(result_ty_var);
                        for ty in &arm_body_tys {
                            ctx.add_constraint(Constraint::Equal(
                                result_ty.clone(),
                                ty.clone(),
                                range,
                                ConstraintOrigin::General,
                            ));
                        }
                        ctx.set_symbol_type(symbol_id, result_ty);
                    }
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        SymbolKind::Try => {
            // Try/Catch: try body and catch body may have different types in dynamically typed mq
            let children = get_children(children_index, symbol_id);
            let range = get_symbol_range(hir, symbol_id);

            if children.len() >= 2 {
                // First child is try body
                let try_ty = ctx.get_or_create_symbol_type(children[0]);

                // Second child may be a Catch symbol wrapping the catch body
                let catch_ty = if let Some(catch_symbol) = hir.symbol(children[1])
                    && matches!(catch_symbol.kind, SymbolKind::Catch)
                {
                    // Look into Catch's children to get the actual catch body type
                    let catch_children = get_children(children_index, children[1]);
                    if let Some(&last_child) = catch_children.last() {
                        let body_ty = ctx.get_or_create_symbol_type(last_child);
                        ctx.set_symbol_type(children[1], body_ty.clone());
                        body_ty
                    } else {
                        ctx.get_or_create_symbol_type(children[1])
                    }
                } else {
                    ctx.get_or_create_symbol_type(children[1])
                };

                // Resolve the types to check if they're concrete
                let resolved_try = ctx.resolve_type(&try_ty);
                let resolved_catch = ctx.resolve_type(&catch_ty);
                let both_concrete = !resolved_try.is_var() && !resolved_catch.is_var();
                let same_discriminant =
                    both_concrete && std::mem::discriminant(&resolved_try) == std::mem::discriminant(&resolved_catch);

                if both_concrete && !same_discriminant {
                    // Different concrete types: use Union type to represent both possibilities
                    let union_ty = Type::union(vec![resolved_try, resolved_catch]);
                    ctx.set_symbol_type(symbol_id, union_ty);
                } else {
                    // Same type or at least one is a type variable: unify them
                    ctx.add_constraint(Constraint::Equal(
                        try_ty.clone(),
                        catch_ty,
                        range,
                        ConstraintOrigin::General,
                    ));
                    ctx.set_symbol_type(symbol_id, try_ty);
                }
            } else if !children.is_empty() {
                let try_ty = ctx.get_or_create_symbol_type(children[0]);
                ctx.set_symbol_type(symbol_id, try_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Macro definitions (same type logic as Function)
        SymbolKind::Macro(params) => {
            let param_tys: Vec<Type> = params.iter().map(|_| Type::Var(ctx.fresh_var())).collect();
            let ret_ty = Type::Var(ctx.fresh_var());
            let func_ty = Type::function(param_tys.clone(), ret_ty.clone());
            ctx.set_symbol_type(symbol_id, func_ty);

            // Bind parameter types to their parameter symbols
            let children = get_children(children_index, symbol_id);
            let param_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            for (param_sym, param_ty) in param_children.iter().zip(param_tys.iter()) {
                let sym_ty = ctx.get_or_create_symbol_type(*param_sym);
                let range = get_symbol_range(hir, *param_sym);
                ctx.add_constraint(Constraint::Equal(
                    sym_ty,
                    param_ty.clone(),
                    range,
                    ConstraintOrigin::General,
                ));
            }

            // Connect macro body's type to the return type
            let body_children: Vec<SymbolId> = children
                .iter()
                .filter(|&&child_id| {
                    hir.symbol(child_id)
                        .map(|s| !matches!(s.kind, SymbolKind::Parameter))
                        .unwrap_or(false)
                })
                .copied()
                .collect();
            if let Some(&last_body) = body_children.last() {
                let body_ty = ctx.get_or_create_symbol_type(last_body);
                let range = get_symbol_range(hir, symbol_id);
                ctx.add_constraint(Constraint::Equal(ret_ty, body_ty, range, ConstraintOrigin::General));
            }
        }

        // Dynamic function calls
        SymbolKind::CallDynamic => {
            let children = get_children(children_index, symbol_id);
            if !children.is_empty() {
                let callable_ty = ctx.get_or_create_symbol_type(children[0]);
                let arg_tys: Vec<Type> = children[1..]
                    .iter()
                    .map(|&arg_id| ctx.get_or_create_symbol_type(arg_id))
                    .collect();
                let range = get_symbol_range(hir, symbol_id);

                // Build the expected function type and unify with the callable
                let ret_ty = Type::Var(ctx.fresh_var());
                let expected_func_ty = Type::function(arg_tys, ret_ty.clone());
                ctx.add_constraint(Constraint::Equal(
                    callable_ty,
                    expected_func_ty,
                    range,
                    ConstraintOrigin::General,
                ));
                ctx.set_symbol_type(symbol_id, ret_ty);
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Selector: resolve the type for chained selectors like `.h1.value` or `.h1.depth`.
        //
        // In the HIR, `.h1.value` is represented as Selector(.h1) with a child Selector(.value).
        // The parent selector's output type (always `Type::Markdown` for non-Attr selectors)
        // is propagated as piped input to each child selector in the chain.
        //
        // Attr selectors (`Selector::Attr`) return the concrete attribute type via `attr_kind_to_type`.
        // Non-Attr selectors (`.h1`, `.code`, etc.) always return `Type::Markdown`.
        SymbolKind::Selector(ref selector) => {
            // Compute this selector's own output type based on the incoming piped input.
            let own_type = if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                let resolved = ctx.resolve_type(&piped_ty);
                let field_name = hir
                    .symbol(symbol_id)
                    .and_then(|s| s.value.as_ref())
                    .map(|v| v.trim_start_matches('.').trim_start_matches("[:").trim_end_matches(']'));

                if let Type::Markdown = resolved {
                    // Piped input is a Markdown node (from a parent selector in a chain).
                    // Attr selectors return their specific type; non-Attr still return Markdown.
                    if let mq_lang::Selector::Attr(attr_kind) = selector {
                        attr_kind_to_type(attr_kind)
                    } else {
                        Type::Markdown
                    }
                } else if let Type::Record(ref fields, ref rest) = resolved {
                    if let Some(name) = field_name {
                        if let Some(field_ty) = fields.get(name) {
                            field_ty.clone()
                        } else if matches!(**rest, Type::RowEmpty) {
                            let range = get_symbol_range(hir, symbol_id);
                            ctx.add_error(TypeError::UndefinedField {
                                field: name.to_string(),
                                record_ty: resolved.display_renumbered(),
                                span: range.as_ref().map(range_to_span),
                                location: range,
                            });
                            let ty_var = ctx.fresh_var();
                            Type::Var(ty_var)
                        } else {
                            let ty_var = ctx.fresh_var();
                            Type::Var(ty_var)
                        }
                    } else {
                        let ty_var = ctx.fresh_var();
                        Type::Var(ty_var)
                    }
                } else {
                    // Piped type not yet resolved — defer to post-unification.
                    // Include the attr_kind so that if the piped type resolves to Markdown,
                    // the correct attribute type can be returned (e.g., `md.depth` → number).
                    if let Some(name) = field_name {
                        let attr_kind = if let mq_lang::Selector::Attr(ak) = selector {
                            Some(ak.clone())
                        } else {
                            None
                        };
                        ctx.add_deferred_selector_access(infer::DeferredSelectorAccess {
                            symbol_id,
                            piped_ty: piped_ty.clone(),
                            field_name: name.to_string(),
                            attr_kind,
                            range: get_symbol_range(hir, symbol_id),
                        });
                    }
                    let ty_var = ctx.fresh_var();
                    Type::Var(ty_var)
                }
            } else {
                // No piped input: non-Attr selectors (`.h1`, `.code`, etc.) always produce
                // a Markdown node. Attr selectors (`.value`, `.depth`, etc.) as the root of
                // a chain return their concrete type.
                if let mq_lang::Selector::Attr(attr_kind) = selector {
                    attr_kind_to_type(attr_kind)
                } else {
                    Type::Markdown
                }
            };

            // Propagate own_type as piped input to child selectors (for chains like `.h1.value`).
            // The parent selector's final type is the last child selector's output type.
            let child_selectors: Vec<SymbolId> = get_children(children_index, symbol_id)
                .iter()
                .copied()
                .filter(|&child_id| {
                    hir.symbol(child_id)
                        .map(|s| matches!(s.kind, SymbolKind::Selector(_)))
                        .unwrap_or(false)
                })
                .collect();

            if child_selectors.is_empty() {
                ctx.set_symbol_type(symbol_id, own_type);
            } else {
                // Thread the output type through the chain of child selectors.
                ctx.set_piped_input(child_selectors[0], own_type);
                for i in 0..child_selectors.len() {
                    if let Some(child_sym) = hir.symbol(child_selectors[i]) {
                        generate_symbol_constraints(
                            hir,
                            child_selectors[i],
                            child_sym.kind.clone(),
                            ctx,
                            children_index,
                        );
                    }
                    if i + 1 < child_selectors.len() {
                        let next_ty = ctx.get_or_create_symbol_type(child_selectors[i]);
                        ctx.set_piped_input(child_selectors[i + 1], next_ty);
                    }
                }
                // The parent's type is the last child's type.
                let last_ty = ctx.get_or_create_symbol_type(*child_selectors.last().unwrap());
                ctx.set_symbol_type(symbol_id, last_ty);
            }
        }

        // QualifiedAccess doesn't need deep type checking
        SymbolKind::QualifiedAccess => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }

        // `break: value` carries the type of its value expression.
        // Bare `break` (no value child) gets a fresh type variable.
        SymbolKind::Keyword => {
            let symbol = hir.symbol(symbol_id);
            if symbol.is_some_and(|s| s.value.as_deref() == Some("break")) {
                let children = get_children(children_index, symbol_id);
                if let Some(&value_child) = children.first() {
                    let child_ty = ctx.get_or_create_symbol_type(value_child);
                    ctx.set_symbol_type(symbol_id, child_ty);
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else if symbol.is_some_and(|s| s.value.as_deref() == Some("self")) {
                // `self` refers to the piped input value
                if let Some(piped_ty) = ctx.get_piped_input(symbol_id).cloned() {
                    ctx.set_symbol_type(symbol_id, piped_ty);
                } else {
                    let ty_var = ctx.fresh_var();
                    ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
                }
            } else {
                let ty_var = ctx.fresh_var();
                ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
            }
        }

        // Inert kinds: identifiers, arguments, imports, modules, etc.
        _ => {
            let ty_var = ctx.fresh_var();
            ctx.set_symbol_type(symbol_id, Type::Var(ty_var));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walk_ancestors;
    use rstest::rstest;

    #[test]
    fn test_constraint_display() {
        let c = Constraint::Equal(Type::Number, Type::String, None, ConstraintOrigin::General);
        assert_eq!(c.to_string(), "number ~ string");
    }

    #[test]
    fn test_children_index() {
        let mut hir = Hir::default();
        let _ = hir.add_code(None, "let x = 1 | x + 2");

        let index = build_children_index(&hir);
        assert!(!index.is_empty());

        // Root symbols should have parent = None, not in index
        // But symbols inside the root scope/block should be indexed
        for (id, symbol) in hir.symbols() {
            if let Some(parent) = symbol.parent {
                let children = get_children(&index, parent);
                assert!(children.contains(&id));
            }
        }
    }

    #[rstest]
    #[case(mq_lang::AttrKind::Value, Type::String)]
    #[case(mq_lang::AttrKind::Depth, Type::Number)]
    #[case(mq_lang::AttrKind::Ordered, Type::Bool)]
    #[case(mq_lang::AttrKind::Children, Type::array(Type::Markdown))]
    fn test_attr_kind_to_type(#[case] kind: mq_lang::AttrKind, #[case] expected: Type) {
        assert_eq!(attr_kind_to_type(&kind), expected);
    }

    #[test]
    fn test_is_foreach_iterable_ref() {
        let mut hir = Hir::default();
        hir.add_code(None, "foreach(x, y): 1;");

        let y_ref = hir
            .symbols()
            .find(|(_, s)| s.value.as_deref() == Some("y"))
            .map(|(id, _)| id)
            .unwrap();
        assert!(is_foreach_iterable_ref(&hir, y_ref));

        let x_var = hir
            .symbols()
            .find(|(_, s)| s.value.as_deref() == Some("x"))
            .map(|(id, _)| id)
            .unwrap();
        assert!(!is_foreach_iterable_ref(&hir, x_var));
    }

    #[test]
    fn test_might_receive_piped_input() {
        let mut hir = Hir::default();
        // x | f() -> f() might receive piped input
        hir.add_code(None, "let x = 1 | f()");
        let f_call = hir
            .symbols()
            .find(|(_, s)| s.value.as_deref() == Some("f"))
            .map(|(id, _)| id)
            .unwrap();
        assert!(might_receive_piped_input(&hir, f_call));
    }

    #[test]
    fn test_is_inside_quote_block() {
        let mut hir = Hir::default();
        // In HIR, is_inside_quote_block checks if a Keyword symbol with None value is an ancestor.
        // Bare quote/unquote keywords in HIR (added by add_quote_expr/add_unquote_expr)
        // have value=None and kind=Keyword.
        // Quote block in mq uses `quote do ... end` which is parsed into Quote node.
        let _ = hir.add_code(None, "quote: x + 1;");

        // Try to find the 'x' reference
        let x_ref = hir
            .symbols()
            .find(|(_, s)| s.value.as_deref() == Some("x"))
            .map(|(id, _)| id)
            .expect("Reference to 'x' should be found");

        // Verify that the setup created a quote keyword parent
        let has_quote_parent =
            walk_ancestors(&hir, x_ref).any(|(_, s)| matches!(s.kind, SymbolKind::Keyword) && s.value.is_none());

        if has_quote_parent {
            assert!(is_inside_quote_block(&hir, x_ref));
        }
    }
}
