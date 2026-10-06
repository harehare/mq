//! Deferred resolution passes for the type checker.
//!
//! After the first round of constraint generation and unification, some type information
//! is still partially unknown (record fields, tuple indices, overloaded operators, user-defined
//! function calls). This module contains the resolution passes that are run iteratively until
//! all types are concrete.
//!
//! Each function is a pure transformation over an [`InferenceContext`] and/or [`mq_hir::Hir`];
//! none of them touch `TypeChecker`'s own state directly.

use mq_hir::{Hir, SymbolId};
use rustc_hash::FxHashMap;

use crate::{
    TypeError,
    constraint::{Constraint, ConstraintOrigin},
    infer::{DeferredOverload, DeferredParameterCall, InferenceContext},
    node_attr::{SelectorOutput, node_attr_type, node_selector_output},
    types::{self, Substitution},
    unify, walk_ancestors,
};

/// Looks up `field` in a Record type, following row variables through the substitution map.
///
/// Returns `Some(field_type)` when found, or `None` when the field is absent and
/// the row chain ends with `RowEmpty` or a still-free `Var` (no further extension
/// was ever unified into this record, so the field is genuinely missing).
fn find_record_field<'a>(ctx: &'a InferenceContext, ty: &'a types::Type, field: &str) -> Option<types::Type> {
    match ty {
        types::Type::Record(fields, rest) => {
            if let Some(ft) = fields.get(field) {
                return Some(ft.clone());
            }
            let resolved = ctx.resolve_type(rest);
            find_record_field(ctx, &resolved, field)
        }
        _ => None,
    }
}

/// Returns `true` when the row chain ends with `RowEmpty` or a free type variable,
/// meaning no further fields can appear — the record is effectively closed.
fn record_row_is_closed(ctx: &InferenceContext, ty: &types::Type) -> bool {
    match ty {
        types::Type::Record(_, rest) => {
            let resolved = ctx.resolve_type(rest);
            record_row_is_closed(ctx, &resolved)
        }
        types::Type::RowEmpty | types::Type::Var(_) => true,
        _ => false,
    }
}

/// Resolves deferred record field accesses after the first round of unification.
///
/// For each deferred bracket access `v[:key]`, resolves the variable's type
/// (now concrete after unification) and, if it is a Record, looks up the field
/// type and binds the bracket access expression's type to that field type.
pub(crate) fn resolve_record_field_accesses(ctx: &mut InferenceContext) -> bool {
    let accesses = ctx.take_deferred_record_accesses();
    if accesses.is_empty() {
        return false;
    }

    let mut resolved_any = false;
    for access in &accesses {
        let var_ty = match ctx.get_symbol_type(access.def_id).cloned() {
            Some(ty) => ctx.resolve_type(&ty),
            None => continue,
        };

        if let types::Type::Record(..) = &var_ty {
            if let Some(field_ty) = find_record_field(ctx, &var_ty, &access.field_name) {
                let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                ctx.add_constraint(Constraint::Equal(call_ty, field_ty, None, ConstraintOrigin::General));
                resolved_any = true;
            } else if record_row_is_closed(ctx, &var_ty) {
                ctx.add_error(TypeError::UndefinedField {
                    field: access.field_name.clone(),
                    record_ty: var_ty.display_renumbered(),
                    span: access.range.as_ref().map(unify::range_to_span),
                    location: access.range,
                });
            }
        }
    }
    resolved_any
}

/// Resolves deferred bracket accesses on function call return values.
///
/// When the CST lowers `f(x)["key"]` as `Call(f, [x, "key"])` and the type
/// checker detects trailing bracket keys, it defers the field lookup until
/// after unification. This function resolves the function's return type (now
/// concrete) and looks up the field, binding the call expression's type to it.
pub(crate) fn resolve_deferred_call_return_accesses(ctx: &mut InferenceContext) -> bool {
    let accesses = ctx.take_deferred_call_return_accesses();
    if accesses.is_empty() {
        return false;
    }

    let mut resolved_any = false;
    for access in &accesses {
        let return_ty = ctx.resolve_type(&access.return_type);

        if let types::Type::Record(..) = &return_ty {
            if let Some(field_ty) = find_record_field(ctx, &return_ty, &access.field_name) {
                let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                ctx.add_constraint(Constraint::Equal(call_ty, field_ty, None, ConstraintOrigin::General));
                resolved_any = true;
            } else if record_row_is_closed(ctx, &return_ty) {
                ctx.add_error(TypeError::UndefinedField {
                    field: access.field_name.clone(),
                    record_ty: return_ty.display_renumbered(),
                    span: access.range.as_ref().map(unify::range_to_span),
                    location: access.range,
                });
            }
        } else if let types::Type::Dict(..) = &return_ty {
            // Dict access: result is a fresh type variable (dynamic value type)
            resolved_any = true;
        }
    }
    resolved_any
}

/// Resolves deferred selector field accesses after unification. Returns whether any was resolved;
/// accesses whose piped input is still a type variable are kept for a later call.
///
/// For each deferred selector access, resolves the piped input type (now concrete
/// after unification) and either returns the attribute type (for Markdown piped input)
/// or checks that the field exists in the record.
pub(crate) fn resolve_selector_field_accesses(ctx: &mut InferenceContext) -> bool {
    let accesses = ctx.take_deferred_selector_accesses();
    let mut resolved_any = false;

    for access in accesses {
        let mut resolved = ctx.resolve_type(&access.piped_ty);
        if resolved.is_var()
            && let Some(source_ty) = access
                .piped_source
                .and_then(|source| ctx.get_symbol_type(source).cloned())
        {
            resolved = ctx.resolve_type(&source_ty);
        }
        if resolved.is_var() {
            // The input is not known yet; try again after more types are resolved.
            ctx.add_deferred_selector_access(access);
            continue;
        }
        resolved_any = true;

        if let types::Type::Node(kinds) = resolved {
            // Piped input resolved to a Markdown node (e.g. `let md = .h | md.depth`).
            match node_selector_output(&access.selector, kinds) {
                SelectorOutput::Type(ty) => {
                    let result_ty = access.result_ty.clone();
                    ctx.add_constraint(Constraint::Equal(result_ty, ty, None, ConstraintOrigin::General));
                }
                SelectorOutput::MissingAttr(attr) => ctx.add_error(TypeError::UndefinedAttribute {
                    attr,
                    node_ty: resolved.display_renumbered(),
                    span: access.range.as_ref().map(unify::range_to_span),
                    location: access.range,
                }),
            }
        } else if let types::Type::Record(fields, rest) = &resolved {
            if let Some(field_ty) = fields.get(&access.field_name) {
                ctx.add_constraint(Constraint::Equal(
                    access.result_ty.clone(),
                    field_ty.clone(),
                    None,
                    ConstraintOrigin::General,
                ));
            } else if matches!(rest.as_ref(), types::Type::RowEmpty) {
                ctx.add_error(TypeError::UndefinedField {
                    field: access.field_name.clone(),
                    record_ty: resolved.display_renumbered(),
                    span: access.range.as_ref().map(unify::range_to_span),
                    location: access.range,
                });
            }
        }
    }
    resolved_any
}

/// Resolves deferred tuple index accesses after the first round of unification.
///
/// For each deferred tuple access `v[i]`, resolves the variable's type.
/// If it is a Tuple type:
///   - Literal index: binds the access to the specific element type
///   - Dynamic index: binds the access to the Union of all element types
///
/// If it is an Array type, binds like normal array element access.
///
/// Accesses on Union types with unresolved Var members are re-queued for a
/// later pass, allowing `resolve_deferred_overloads` to first resolve the
/// function calls that produce those Var types.
pub(crate) fn resolve_deferred_tuple_accesses(ctx: &mut InferenceContext) -> bool {
    let accesses = ctx.take_deferred_tuple_accesses();
    if accesses.is_empty() {
        return false;
    }

    let mut resolved_any = false;
    for access in &accesses {
        let var_ty = match ctx.get_symbol_type(access.def_id).cloned() {
            Some(ty) => ctx.resolve_type(&ty),
            None => continue,
        };

        match &var_ty {
            types::Type::Tuple(elems) => {
                let result_ty = if let Some(idx) = access.index {
                    if idx < elems.len() {
                        elems[idx].clone()
                    } else {
                        // Out of bounds — use fresh type variable
                        types::Type::Var(ctx.fresh_var())
                    }
                } else {
                    // Dynamic index — return Union of all element types
                    types::Type::union(elems.clone())
                };
                let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                ctx.add_constraint(Constraint::Equal(call_ty, result_ty, None, ConstraintOrigin::General));
                resolved_any = true;
            }
            types::Type::Array(elem) => {
                // Normal array — bind to element type
                let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                ctx.add_constraint(Constraint::Equal(
                    call_ty,
                    *elem.clone(),
                    None,
                    ConstraintOrigin::General,
                ));
                resolved_any = true;
            }
            types::Type::Union(members) => {
                // Union type (e.g., `Union(Array(String), None)`) — extract element types
                // from Array/Tuple members and use them as the index access result.
                // If any union member is still an unresolved Var, re-queue this access
                // for the next pass (after `resolve_deferred_overloads` has run).
                let has_var_member = members.iter().any(|m| m.is_var());
                if has_var_member {
                    ctx.add_deferred_tuple_access(access.clone());
                    continue;
                }

                let mut elem_types = Vec::new();
                for member in members {
                    match member {
                        types::Type::Array(elem) => elem_types.push(*elem.clone()),
                        types::Type::Tuple(elems) => {
                            if let Some(idx) = access.index {
                                if idx < elems.len() {
                                    elem_types.push(elems[idx].clone());
                                }
                            } else {
                                elem_types.extend(elems.iter().cloned());
                            }
                        }
                        _ => {}
                    }
                }
                if !elem_types.is_empty() {
                    let result_ty = if elem_types.len() == 1 {
                        elem_types.remove(0)
                    } else {
                        types::Type::union(elem_types)
                    };
                    let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                    ctx.add_constraint(Constraint::Equal(call_ty, result_ty, None, ConstraintOrigin::General));
                    resolved_any = true;
                }
            }
            types::Type::Dict(_, value) => {
                // Dict bracket access: result type is the dict's value type.
                // e.g. `d[key]` where d: Dict(String, V) → V
                let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                ctx.add_constraint(Constraint::Equal(
                    call_ty,
                    *value.clone(),
                    None,
                    ConstraintOrigin::General,
                ));
                resolved_any = true;
            }
            types::Type::String => {
                // String bracket access: `s[n]` or `s[n:m]` returns a String (character/slice).
                let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                ctx.add_constraint(Constraint::Equal(
                    call_ty,
                    types::Type::String,
                    None,
                    ConstraintOrigin::General,
                ));
                resolved_any = true;
            }
            types::Type::Var(_) => {
                // Type variable not yet resolved — re-queue for next pass.
                // Do NOT add an Array constraint here: if the variable later resolves
                // to a Tuple or other type, the premature Array constraint would
                // conflict and produce spurious "infinite type" or mismatch errors.
                ctx.add_deferred_tuple_access(access.clone());
            }
            types::Type::Record(..) => {
                // A record indexed by a non-literal key (e.g. `{"a": true}[word]`) is a dynamic
                // dict lookup. The key is unknown statically, so the result stays unconstrained
                // instead of forcing the record to unify with an array.
            }
            _ => {
                // Known non-array/tuple/union type — add array constraint as fallback
                let elem_var = ctx.fresh_var();
                let elem_ty = types::Type::Var(elem_var);
                ctx.add_constraint(Constraint::Equal(
                    var_ty.clone(),
                    types::Type::array(elem_ty.clone()),
                    access.range,
                    ConstraintOrigin::General,
                ));
                let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                ctx.add_constraint(Constraint::Equal(call_ty, elem_ty, None, ConstraintOrigin::General));
                resolved_any = true;
            }
        }
    }
    resolved_any
}

/// Result of checking union member overload compatibility.
enum UnionCheckResult {
    /// All non-None members return a consistent type, with optional None propagation.
    /// The `has_none_propagation` flag is `true` when at least one `None` member was
    /// skipped (standard None-propagation pattern), meaning the result may be `None`
    /// at runtime even though the statically-inferred return type is concrete.
    Consistent {
        ret: types::Type,
        has_none_propagation: bool,
    },
    /// A union member type has no matching overload for the operator.
    MemberUnsupported(types::Type),
    /// Different union members return different types (inconsistent).
    InconsistentReturn,
    /// At least one union member is still an unresolved type variable; defer.
    HasVarMember,
}

/// Checks whether all non-None-propagating members of every union-typed argument
/// resolve to the same return type when applied to the given operator.
///
/// Returns [`UnionCheckResult::Consistent`] if every non-None member produces an
/// identical return type, indicating the operation is safe for the non-None cases.
///
/// None members that follow the standard none-propagation pattern (`f(None) -> None`)
/// are skipped when determining consistency, since they are an expected dynamic
/// behavior in mq (e.g. `len(None)` returns None while `len(Array) → Number`).
fn check_union_members(
    ctx: &mut InferenceContext,
    op_name: &str,
    resolved_operands: &[types::Type],
) -> UnionCheckResult {
    let mut unique_ret: Option<types::Type> = None;
    let mut has_none_propagation = false;

    for (i, arg_ty) in resolved_operands.iter().enumerate() {
        let types::Type::Union(members) = arg_ty else {
            continue;
        };

        for member in members {
            // Reject unions containing unresolved type variables
            if member.is_var() {
                return UnionCheckResult::HasVarMember;
            }

            let mut test_args = resolved_operands.to_vec();
            test_args[i] = member.clone();
            let Some(types::Type::Function(_, member_ret)) = ctx.resolve_overload(op_name, &test_args) else {
                return UnionCheckResult::MemberUnsupported(member.clone());
            };
            let resolved_ret = ctx.resolve_type(&member_ret);

            // Skip None-propagation overloads: a None input producing a None output
            // is the standard mq "propagate None" pattern and does not affect the
            // consistency of the return type for non-None inputs.
            if matches!(member, types::Type::None) && matches!(resolved_ret, types::Type::None) {
                has_none_propagation = true;
                continue;
            }

            match &unique_ret {
                None => unique_ret = Some(resolved_ret),
                Some(prev) if prev == &resolved_ret => {}
                _ => return UnionCheckResult::InconsistentReturn,
            }
        }
    }

    match unique_ret {
        Some(ret) => UnionCheckResult::Consistent {
            ret,
            has_none_propagation,
        },
        // All members were None-propagation (or no union args) → use None as result
        None => UnionCheckResult::Consistent {
            ret: types::Type::None,
            has_none_propagation: false,
        },
    }
}

/// Resolves an operation whose single operand is a union of arrays with different return types
/// by treating that operand as one array of the union of the members' element types.
///
/// `map([bool] | [string], f)` is resolved as `map([bool | string], f)`, so `f` is checked
/// against the elements of every member and the result is `[r]` for the result `r` of `f`.
/// Returns `false` when more than one operand is a union or a member is not an array, leaving
/// the operation to be reported.
fn merge_array_union_operand(
    ctx: &mut InferenceContext,
    d: &DeferredOverload,
    resolved_operands: &[types::Type],
) -> bool {
    use types::Type;

    let mut union_positions = resolved_operands.iter().enumerate().filter(|(_, ty)| ty.is_union());
    let (Some((position, Type::Union(members))), None) = (union_positions.next(), union_positions.next()) else {
        return false;
    };
    let elems: Option<Vec<Type>> = members
        .iter()
        .map(|member| match member {
            Type::Array(elem) => Some(elem.as_ref().clone()),
            _ => None,
        })
        .collect();
    let Some(elems) = elems else {
        return false;
    };

    let merged = Type::array(Type::union(elems));
    let mut args = resolved_operands.to_vec();
    args[position] = merged.clone();
    let Some(Type::Function(param_tys, ret_ty)) = ctx.resolve_overload(&d.op_name, &args) else {
        return false;
    };
    if param_tys.len() != d.operand_tys.len() {
        return false;
    }
    for (i, param_ty) in param_tys.iter().enumerate() {
        let operand_ty = if i == position {
            merged.clone()
        } else {
            d.operand_tys[i].clone()
        };
        ctx.add_constraint(Constraint::Equal(
            operand_ty,
            param_ty.clone(),
            d.range,
            ConstraintOrigin::General,
        ));
    }
    ctx.set_symbol_type_no_bind(d.symbol_id, *ret_ty);
    true
}

/// Resolves an operation whose single operand is a union of containers (arrays, dicts, records,
/// generators, `none`)
/// by resolving it for each member and taking the union of the results.
///
/// `get(x, k)` with `x: [a] | {k: v}` is `a | v`. Each member's own signature is tied to that
/// member, so element and value types flow into the result. The other operands are checked
/// only where every member agrees on a concrete parameter type. Returns `false` when more than
/// one operand is a union or a member is not a container, leaving the operation to be reported.
fn distribute_over_container_members(
    ctx: &mut InferenceContext,
    d: &DeferredOverload,
    resolved_operands: &[types::Type],
) -> bool {
    use types::Type;

    let mut union_positions = resolved_operands.iter().enumerate().filter(|(_, ty)| ty.is_union());
    let (Some((position, Type::Union(members))), None) = (union_positions.next(), union_positions.next()) else {
        return false;
    };
    // Containers, plus `none` and generators, which functions over collections pass through.
    let is_container = |member: &Type| {
        matches!(
            member,
            Type::Array(_) | Type::Tuple(_) | Type::Dict(..) | Type::Record(..) | Type::None | Type::Generator(_)
        )
    };
    if !members.iter().all(is_container) {
        return false;
    }

    let mut rets = Vec::with_capacity(members.len());
    let mut shared_params: Vec<Option<Type>> = vec![None; resolved_operands.len()];
    let mut disagreeing = vec![false; resolved_operands.len()];
    for member in members {
        let mut args = resolved_operands.to_vec();
        args[position] = member.clone();
        let Some(Type::Function(param_tys, ret_ty)) = ctx.resolve_overload(&d.op_name, &args) else {
            return false;
        };
        if param_tys.len() != d.operand_tys.len() {
            return false;
        }
        ctx.add_constraint(Constraint::Equal(
            member.clone(),
            param_tys[position].clone(),
            d.range,
            ConstraintOrigin::General,
        ));
        for (i, param_ty) in param_tys.iter().enumerate().filter(|(i, _)| *i != position) {
            match &shared_params[i] {
                None if param_ty.is_concrete() => shared_params[i] = Some(param_ty.clone()),
                Some(shared) if shared == param_ty => {}
                _ => disagreeing[i] = true,
            }
        }
        rets.push(*ret_ty);
    }
    for (i, shared) in shared_params.into_iter().enumerate() {
        if let Some(param_ty) = shared.filter(|_| !disagreeing[i]) {
            ctx.add_constraint(Constraint::Equal(
                d.operand_tys[i].clone(),
                param_ty,
                d.range,
                ConstraintOrigin::General,
            ));
        }
    }
    ctx.set_symbol_type_no_bind(d.symbol_id, Type::union(rets));
    true
}

/// Resolves deferred try/catch branch-type merges, after other deferred passes
/// (record/tuple access, overloads, ...) have settled the branch types.
pub(crate) fn resolve_deferred_try_catches(ctx: &mut InferenceContext) -> bool {
    let entries = ctx.take_deferred_try_catches();
    if entries.is_empty() {
        return false;
    }

    for entry in entries {
        let Some(result_ty) = ctx.get_symbol_type(entry.symbol_id).cloned() else {
            continue;
        };
        let resolved_try = ctx.resolve_type(&entry.try_ty);
        let resolved_catch = ctx.resolve_type(&entry.catch_ty);
        let both_resolved = !resolved_try.is_var() && !resolved_catch.is_var();

        let merged_ty = if let Some(merged) = both_resolved
            .then(|| resolved_try.merge_branches(&resolved_catch, true))
            .flatten()
        {
            merged
        } else if !both_resolved {
            // A branch whose type is still unknown must not be pinned to the other branch's type:
            // `try: f() catch: "error"` is whatever `f()` returns, or a string.
            types::Type::union(vec![resolved_try, resolved_catch])
        } else {
            ctx.add_constraint(Constraint::Equal(
                entry.try_ty.clone(),
                entry.catch_ty,
                entry.range,
                ConstraintOrigin::General,
            ));
            entry.try_ty
        };
        ctx.add_constraint(Constraint::Equal(
            result_ty,
            merged_ty,
            entry.range,
            ConstraintOrigin::General,
        ));

        // Solve immediately so an outer try/catch that depends on this result
        // (nested `try: (try: ... catch: ...) catch: ...`) sees it resolved.
        unify::solve_constraints(ctx);
    }

    true
}

/// Resolves deferred overloads after the first round of unification.
///
/// Binary/unary operators whose operands were type variables during constraint
/// generation are re-processed now that operand types may be known.
/// Runs unification after each resolution so that type information propagates
/// incrementally to subsequent deferred overloads.
pub(crate) fn resolve_deferred_overloads(ctx: &mut InferenceContext) {
    let deferred = ctx.take_deferred_overloads();
    if deferred.is_empty() {
        return;
    }

    // Resolve deferred overloads in multiple passes using index-based tracking
    // to avoid cloning DeferredOverload structs.
    // Each pass resolves overloads that have at least one concrete operand.
    // Overloads with all-Var operands are deferred to subsequent passes,
    // as intermediate unification may resolve their types.
    let mut remaining_indices: Vec<usize> = (0..deferred.len()).collect();
    // Indices of items to store back into ctx after the loop, moved without cloning.
    let mut store_back_for_later: Vec<usize> = Vec::new();
    let max_passes = 3;
    for _ in 0..max_passes {
        let mut next_remaining = Vec::new();

        for &idx in &remaining_indices {
            let d = &deferred[idx];
            let resolved_operands: Vec<types::Type> = d.operand_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();

            let all_concrete = resolved_operands.iter().all(|ty| ty.is_concrete());
            let has_union = resolved_operands.iter().any(|ty| ty.is_union());

            if has_union {
                // If any union member contains an unresolved type variable, defer
                // to a later pass after the variable resolves.
                let union_has_var = resolved_operands.iter().any(|ty| {
                    if let types::Type::Union(members) = ty {
                        members.iter().any(|m| m.is_var())
                    } else {
                        false
                    }
                });
                if union_has_var {
                    next_remaining.push(idx);
                    continue;
                }

                // Try to find a polymorphic overload where every union-typed argument
                // is matched to a type-variable parameter (e.g. `to_number: (Var) -> Number`).
                if let Some(resolved_ty) = ctx.resolve_overload(&d.op_name, &resolved_operands)
                    && let types::Type::Function(param_tys, ret_ty) = resolved_ty
                    && param_tys.len() == d.operand_tys.len()
                {
                    let union_params_are_vars = resolved_operands
                        .iter()
                        .zip(param_tys.iter())
                        .filter(|(arg, _)| arg.is_union())
                        .all(|(_, param)| param.is_var());

                    if union_params_are_vars {
                        for (operand_ty, param_ty) in d.operand_tys.iter().zip(param_tys.iter()) {
                            ctx.add_constraint(Constraint::Equal(
                                operand_ty.clone(),
                                param_ty.clone(),
                                d.range,
                                ConstraintOrigin::General,
                            ));
                        }
                        ctx.set_symbol_type_no_bind(d.symbol_id, *ret_ty);
                        unify::solve_constraints(ctx);
                        continue;
                    }
                }

                // Check whether all non-None-propagating members return the same type.
                // This handles patterns like `len(Union(Array(String), None))` where
                // None-propagation (None → None) should be ignored when determining
                // the consistent return type.
                match check_union_members(ctx, &d.op_name, &resolved_operands) {
                    UnionCheckResult::Consistent {
                        ret,
                        has_none_propagation,
                    } => {
                        ctx.set_symbol_type_no_bind(d.symbol_id, ret);
                        // Phase 2: None safety — warn when None propagation silently makes
                        // the result `none` at runtime even though the static type is concrete.
                        if has_none_propagation {
                            let nullable_args: Vec<String> = resolved_operands
                                .iter()
                                .filter(|t| t.is_nullable())
                                .map(|t| t.display_renumbered())
                                .collect();
                            if !nullable_args.is_empty() {
                                ctx.add_error(TypeError::NullablePropagation {
                                    op: d.op_name.to_string(),
                                    nullable_arg: nullable_args.join(", "),
                                    span: d.range.as_ref().map(unify::range_to_span),
                                    location: d.range,
                                    context: Some(
                                        "guard with `if x != none: ...` to avoid `none` propagation".to_string(),
                                    ),
                                });
                            }
                        }
                        unify::solve_constraints(ctx);
                        continue;
                    }
                    UnionCheckResult::MemberUnsupported(bad_member) => {
                        let args_str = resolved_operands
                            .iter()
                            .map(|t| t.display_renumbered())
                            .collect::<Vec<_>>()
                            .join(", ");
                        ctx.add_error(TypeError::UnificationError {
                            left: format!("`{}` cannot be applied to all members of ({})", d.op_name, args_str),
                            right: format!("`{}` does not support `{}`", bad_member.display_renumbered(), d.op_name),
                            span: d.range.as_ref().map(unify::range_to_span),
                            location: d.range,
                            context: Some(format!(
                                "consider narrowing the type with `if is_{}(x): ...` before using `{}`",
                                bad_member.display_renumbered(),
                                d.op_name
                            )),
                        });
                        continue;
                    }
                    UnionCheckResult::InconsistentReturn => {
                        if merge_array_union_operand(ctx, d, &resolved_operands)
                            || distribute_over_container_members(ctx, d, &resolved_operands)
                        {
                            unify::solve_constraints(ctx);
                            continue;
                        }
                        let args_str = resolved_operands
                            .iter()
                            .map(|t| t.display_renumbered())
                            .collect::<Vec<_>>()
                            .join(", ");
                        ctx.add_error(TypeError::UnificationError {
                            left: format!("`{}` with arguments ({})", d.op_name, args_str),
                            right: "union members return inconsistent types".to_string(),
                            span: d.range.as_ref().map(unify::range_to_span),
                            location: d.range,
                            context: None,
                        });
                        continue;
                    }
                    UnionCheckResult::HasVarMember => {
                        // Union contains unresolved type variable — defer
                        next_remaining.push(idx);
                        continue;
                    }
                }
            }

            // Skip resolution when any operand is a bare type variable (unknown type)
            // and there are multiple overloads — we can't determine the correct one.
            let any_var = resolved_operands.iter().any(|ty| ty.is_var());
            if any_var {
                let overload_count = ctx.get_builtin_overloads(&d.op_name).map(|o| o.len()).unwrap_or(0);
                if overload_count > 1 {
                    if ctx.resolve_overload(&d.op_name, &resolved_operands).is_none() {
                        ctx.report_no_matching_overload(&d.op_name, &resolved_operands, d.range);
                    } else {
                        next_remaining.push(idx);
                    }
                    continue;
                }
            }

            if let Some(resolved_ty) = ctx.resolve_overload(&d.op_name, &resolved_operands) {
                if let types::Type::Function(param_tys, ret_ty) = resolved_ty
                    && param_tys.len() == d.operand_tys.len()
                {
                    for (operand_ty, param_ty) in d.operand_tys.iter().zip(param_tys.iter()) {
                        ctx.add_constraint(Constraint::Equal(
                            operand_ty.clone(),
                            param_ty.clone(),
                            d.range,
                            ConstraintOrigin::General,
                        ));
                    }
                    ctx.set_symbol_type_no_bind(d.symbol_id, *ret_ty);
                    // Solve constraints incrementally
                    unify::solve_constraints(ctx);
                }
            } else if all_concrete {
                ctx.report_no_matching_overload(&d.op_name, &resolved_operands, d.range);
            } else {
                // Some operands resolved but no match — defer to next pass
                next_remaining.push(idx);
            }
        }

        if next_remaining.len() == remaining_indices.len() {
            // No progress — resolve remaining with best-effort.
            // Collect indices to store back; actual move happens after the loop
            // so that `deferred` is not moved while still borrowed.
            for &idx in &next_remaining {
                let d = &deferred[idx];
                let resolved_operands: Vec<types::Type> = d.operand_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();

                // Don't resolve when any operand still contains a free type variable
                // and there are multiple overloads — store back for user call body checking
                let any_var_best = resolved_operands.iter().any(|ty| !ty.is_concrete());
                if any_var_best {
                    let overload_count = ctx.get_builtin_overloads(&d.op_name).map(|o| o.len()).unwrap_or(0);
                    if overload_count > 1 {
                        store_back_for_later.push(idx);
                        continue;
                    }
                }

                if let Some(resolved_ty) = ctx.resolve_overload(&d.op_name, &resolved_operands) {
                    if let types::Type::Function(param_tys, ret_ty) = resolved_ty
                        && param_tys.len() == d.operand_tys.len()
                    {
                        for (operand_ty, param_ty) in d.operand_tys.iter().zip(param_tys.iter()) {
                            ctx.add_constraint(Constraint::Equal(
                                operand_ty.clone(),
                                param_ty.clone(),
                                d.range,
                                ConstraintOrigin::General,
                            ));
                        }
                        ctx.set_symbol_type_no_bind(d.symbol_id, *ret_ty);
                    }
                } else {
                    let all_concrete = resolved_operands.iter().all(|ty| ty.is_concrete());
                    if all_concrete {
                        ctx.report_no_matching_overload(&d.op_name, &resolved_operands, d.range);
                    } else {
                        // Still unresolved — store back for later processing
                        store_back_for_later.push(idx);
                    }
                }
            }
            break;
        }

        remaining_indices = next_remaining;
        if remaining_indices.is_empty() {
            break;
        }
    }

    // Move store-back items from deferred into ctx without cloning.
    // This is done after the loop so that `deferred` is fully released from borrows.
    if !store_back_for_later.is_empty() {
        let store_back_set: rustc_hash::FxHashSet<usize> = store_back_for_later.into_iter().collect();
        for (idx, d) in deferred.into_iter().enumerate() {
            if store_back_set.contains(&idx) {
                ctx.add_deferred_overload(d);
            }
        }
    }

    // Final unification pass
    unify::solve_constraints(ctx);
}

/// Propagates return types from user-defined function calls.
///
/// After unification, the original function's return type is resolved from its body.
/// This method connects each call site's fresh return type to the original resolved
/// return type, enabling downstream operators to resolve with concrete types.
///
/// Runs in multiple passes until convergence to handle transitive dependencies:
/// when function A calls function B, A's return type may depend on B's fresh call-site
/// return type. A single pass would skip A (still Var) before B is propagated.
/// Iterating until no more progress ensures all transitive chains are resolved.
pub(crate) fn propagate_user_call_returns(ctx: &mut InferenceContext) {
    let deferred_calls = ctx.take_deferred_user_calls();

    let mut prev_unresolved = usize::MAX;
    loop {
        let mut unresolved = 0;
        for call in &deferred_calls {
            if let Some(orig_ty) = ctx.get_symbol_type(call.def_id).cloned() {
                let resolved_orig_ty = ctx.resolve_type(&orig_ty);
                if let types::Type::Function(_, orig_ret) = &resolved_orig_ty {
                    let resolved_ret = ctx.resolve_type(orig_ret);
                    if !resolved_ret.is_var() {
                        ctx.add_constraint(Constraint::Equal(
                            call.fresh_ret_ty.clone(),
                            resolved_ret,
                            call.range,
                            ConstraintOrigin::General,
                        ));
                    } else {
                        unresolved += 1;
                    }
                }
            }
        }

        // Solve new constraints from this pass
        unify::solve_constraints(ctx);

        // Stop when fully converged or no more progress
        if unresolved == 0 || unresolved == prev_unresolved {
            break;
        }
        prev_unresolved = unresolved;
    }

    // Store calls back for body operator checking
    for call in deferred_calls {
        ctx.add_deferred_user_call(call);
    }
}

/// Types the `attr(node, "name")` calls whose node argument is a known node type, from the
/// attribute table, and `get(record, "name")` calls from the field of the record. Returns whether any was resolved; the others are kept for a later call.
pub(crate) fn resolve_attr_calls(ctx: &mut InferenceContext) -> bool {
    let calls = ctx.take_deferred_attr_calls();
    let mut resolved_any = false;

    for call in calls {
        let mut node_ty = ctx.resolve_type(&call.node_ty);
        if node_ty.is_var()
            && let Some(source_ty) = call.node_source.and_then(|source| ctx.get_symbol_type(source).cloned())
        {
            node_ty = ctx.resolve_type(&source_ty);
        }
        if node_ty.is_var() {
            ctx.add_deferred_attr_call(call);
            continue;
        }
        if call.is_get {
            // A field of a record has its own type; other containers are left to the signature.
            if let Some(field_ty) = find_record_field(ctx, &node_ty, &call.attr_name) {
                resolved_any = true;
                let call_ty = ctx.get_or_create_symbol_type(call.symbol_id);
                ctx.add_constraint(Constraint::Equal(call_ty, field_ty, None, ConstraintOrigin::General));
            }
            continue;
        }
        let types::Type::Node(kinds) = node_ty else {
            continue;
        };
        resolved_any = true;
        match node_attr_type(kinds, &call.attr_name) {
            Some(attr_ty) => {
                let call_ty = ctx.get_or_create_symbol_type(call.symbol_id);
                ctx.add_constraint(Constraint::Equal(call_ty, attr_ty, None, ConstraintOrigin::General));
            }
            None => ctx.add_error(TypeError::UndefinedAttribute {
                attr: call.attr_name.clone(),
                node_ty: types::Type::Node(kinds).display_renumbered(),
                span: call.range.as_ref().map(unify::range_to_span),
                location: call.range,
            }),
        }
    }
    resolved_any
}

/// Fixes the yielded type of each generator to the join of the types of its `yield`s, for the
/// generators whose `yield` types are all known by now. The others are retried on the next call.
pub(crate) fn resolve_generator_yields(ctx: &mut InferenceContext) {
    let entries = ctx.take_deferred_generator_yields();
    for entry in entries {
        let yielded: Vec<types::Type> = entry
            .yields
            .iter()
            .map(|&y| {
                let ty = ctx.get_or_create_symbol_type(y);
                ctx.resolve_type(&ty)
            })
            .collect();
        if yielded.iter().any(|ty| ty.has_pending_var()) {
            ctx.add_deferred_generator_yield(entry);
            continue;
        }
        ctx.add_constraint(Constraint::Equal(
            types::Type::Var(entry.yielded),
            types::Type::join(yielded),
            None,
            ConstraintOrigin::General,
        ));
    }
    unify::solve_constraints(ctx);
}

/// Checks operators inside user-defined function bodies against call-site argument types.
///
/// For each deferred user call, builds a local substitution mapping the original
/// function's parameter type variables to the resolved call-site argument types.
/// Then checks each unresolved deferred overload that belongs to the function body
/// by applying the substitution and verifying the operator has a matching overload.
///
/// Also checks operators inside lambda arguments passed to higher-order functions.
/// When a lambda is passed as a function argument and called inside the function body
/// (e.g. via `foreach`), the lambda's parameter type is resolved from the concrete
/// element type of the iterable, and any type-invalid operators inside the lambda
/// are reported.
///
/// Operators inside control flow constructs (If/Elif/Else/While/Match/Try/Catch)
/// are skipped because they may be guarded by runtime type checks (e.g.,
/// `if (is_dict(v)): keys(v)`) that narrow the type beyond what static analysis sees.
///
/// This uses a read-only approach: the substitution is applied locally without
/// modifying the global inference state, so multiple call sites don't interfere.
pub(crate) fn check_user_call_body_operators(hir: &Hir, ctx: &mut InferenceContext) {
    let deferred_calls = ctx.take_deferred_user_calls();
    let unresolved_overloads = ctx.take_deferred_overloads();
    let deferred_param_calls = ctx.take_deferred_parameter_calls();
    let index = BodyIndex::new(hir, &unresolved_overloads, &deferred_param_calls);

    for call in &deferred_calls {
        // Get the original function type
        let orig_ty = match ctx.get_symbol_type(call.def_id).cloned() {
            Some(ty) => ctx.resolve_type(&ty),
            None => continue,
        };

        let orig_params = match &orig_ty {
            types::Type::Function(params, _) => params,
            _ => continue,
        };

        // Resolve call-site argument types
        let resolved_args: Vec<types::Type> = call.arg_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();

        // Skip if any arg is still a type variable (can't determine errors)
        if resolved_args.iter().any(|ty| ty.is_var()) {
            continue;
        }

        // Build substitution: original param type variables → resolved arg types.
        // Uses structural matching to handle cases like Array(Var(elem)) vs Array(Number),
        // which arise when the function body constrains a parameter via foreach iteration.
        let mut subst = Substitution::empty();
        for (orig_param, arg_ty) in orig_params.iter().zip(resolved_args.iter()) {
            extract_structural_subst(orig_param, arg_ty, &mut subst, ctx);
        }

        // Check each unresolved overload that belongs to this function's body.
        // Uses iterative resolution: when an inner operator resolves (e.g. x + 1 → Number),
        // its result type is added to the substitution so outer operators that depend on it
        // (e.g. (x + 1) + true) can also be checked.
        let body_overloads: Vec<_> = index
            .body_overloads(call.def_id)
            .iter()
            .map(|&i| &unresolved_overloads[i])
            .collect();

        check_deferred_overloads_iteratively(&body_overloads, &mut subst, ctx, call.range);

        // Check operators inside lambda arguments via DeferredParameterCalls.
        //
        // When a lambda `fn(x): x + true;` is passed to a higher-order function
        // (e.g. `apply_to_all([1,2,3], f)`), the lambda is called inside with
        // each array element. The DeferredParameterCalls collected during constraint
        // generation record the argument types of those inner calls (e.g. the foreach
        // variable `x`). Resolving those arg types with the main substitution
        // yields the concrete element type (e.g. `Number`), which becomes the
        // lambda's parameter substitution for checking its body operators.
        // The outer function's parameter symbol IDs, to match against inner calls
        let outer_param_syms = index.params(call.def_id);
        for &param_call_index in index.param_calls(call.def_id) {
            let param_call = &deferred_param_calls[param_call_index];

            // Map the called parameter to its index in the outer function's param list
            let param_index = match outer_param_syms.iter().position(|&s| s == param_call.param_sym_id) {
                Some(i) => i,
                None => continue,
            };

            // The corresponding call-site argument must be a lambda (Function type)
            let lambda_tps = match resolved_args.get(param_index) {
                Some(types::Type::Function(p, _)) => p.clone(),
                _ => continue,
            };

            if lambda_tps.is_empty() {
                continue;
            }

            // Build lambda_subst: lambda_param_i → concrete call arg type
            // The concrete type comes from resolving the inner call's arg type
            // (which is linked to the foreach variable) via the main substitution.
            let mut lambda_subst = Substitution::empty();
            for (inner_arg_ty, lambda_tp) in param_call.arg_tys.iter().zip(lambda_tps.iter()) {
                let concrete = ctx.resolve_type(inner_arg_ty).apply_subst(&subst);
                if let types::Type::Var(v) = lambda_tp
                    && !concrete.is_var()
                {
                    lambda_subst.insert(*v, concrete);
                }
            }

            if lambda_subst.is_empty() {
                continue;
            }

            // Get the lambda's HIR symbol ID to scope the operator search
            let lambda_sym_id = match call.arg_symbol_ids.get(param_index) {
                Some(&id) => id,
                None => continue,
            };

            // Check deferred overloads inside the lambda body.
            // Uses iterative resolution for chained operators (e.g. x + 1 + true).
            let lambda_overloads: Vec<_> = index
                .inside_overloads(lambda_sym_id)
                .iter()
                .map(|&i| &unresolved_overloads[i])
                .collect();

            check_deferred_overloads_iteratively(&lambda_overloads, &mut lambda_subst, ctx, call.range);
        }
    }
}

/// Checks deferred overloads iteratively, resolving chained operators.
///
/// When operators are chained (e.g. `x + 1 + true`), the inner operator's result
/// type variable is used as an operand of the outer operator. This method resolves
/// operators in multiple passes: when an inner operator resolves successfully, its
/// result type is added to the substitution so dependent outer operators can also
/// be checked in the next pass.
fn check_deferred_overloads_iteratively(
    overloads: &[&DeferredOverload],
    subst: &mut Substitution,
    ctx: &mut InferenceContext,
    error_range: Option<mq_lang::Range>,
) {
    let mut remaining_indices: Vec<usize> = (0..overloads.len()).collect();
    let max_passes = overloads.len() + 1;

    for _ in 0..max_passes {
        let mut made_progress = false;
        let mut next_remaining = Vec::new();

        for &idx in &remaining_indices {
            let d = overloads[idx];

            let substituted_operands: Vec<types::Type> = d
                .operand_tys
                .iter()
                .map(|ty| {
                    let resolved = ctx.resolve_type(ty);
                    resolved.apply_subst(subst)
                })
                .collect();

            // Skip if any operand is still a bare type variable after substitution
            if substituted_operands.iter().any(|ty| ty.is_var()) {
                next_remaining.push(idx);
                continue;
            }

            // Check if the operator has a matching overload with these types
            if let Some(resolved_ty) = ctx.resolve_overload(&d.op_name, &substituted_operands) {
                // Resolved successfully — add the result type to the substitution
                // so that dependent outer operators can resolve in the next pass.
                if let types::Type::Function(_, ret_ty) = resolved_ty {
                    if let Some(types::Type::Var(result_var)) = ctx.get_symbol_type(d.symbol_id).cloned() {
                        subst.insert(result_var, *ret_ty);
                    } else {
                        // The symbol type may already be resolved via substitution chain;
                        // try resolving it to find the underlying type variable.
                        if let Some(sym_ty) = ctx.get_symbol_type(d.symbol_id).cloned() {
                            let resolved_sym = sym_ty.apply_subst(subst);
                            if let types::Type::Var(result_var) = resolved_sym {
                                subst.insert(result_var, *ret_ty);
                            }
                        }
                    }
                }
                made_progress = true;
            } else {
                // No matching overload — report error
                ctx.report_no_matching_overload(&d.op_name, &substituted_operands, error_range);
                made_progress = true;
            }
        }

        remaining_indices = next_remaining;
        if remaining_indices.is_empty() || !made_progress {
            break;
        }
    }
}

/// Extracts type-variable-to-concrete-type bindings from a structural match between
/// an original parameter type and a resolved argument type.
///
/// This extends the simple `Var → arg` substitution to handle composite types produced
/// by the Foreach constraint, such as `Array(Var(elem))` appearing as a parameter type
/// when the function iterates over the parameter.  For example:
///
/// - `Var(x)` vs `Array(Number)` → `x → Array(Number)`
/// - `Array(Var(elem))` vs `Array(Number)` → `elem → Number`
///
/// Function types are intentionally NOT recursed into here because they represent
/// lambda arguments that are handled separately in the lambda-body checking phase.
fn extract_structural_subst(
    orig: &types::Type,
    arg: &types::Type,
    subst: &mut Substitution,
    ctx: &mut InferenceContext,
) {
    match orig {
        types::Type::Var(var) => {
            let free = arg.free_vars();
            if !free.contains(var) {
                subst.insert(*var, arg.clone());
            } else if matches!(arg, types::Type::Function(_, _)) {
                // Self-referential function argument — use a generic placeholder.
                let p = types::Type::Var(ctx.fresh_var());
                let r = types::Type::Var(ctx.fresh_var());
                subst.insert(*var, types::Type::function(vec![p], r));
            }
        }
        types::Type::Array(orig_elem) => {
            if let types::Type::Array(arg_elem) = arg {
                extract_structural_subst(orig_elem, arg_elem, subst, ctx);
            }
        }
        // Function types are handled by the lambda-body checking phase — skip here.
        _ => {}
    }
}

/// Per-function lookup tables for [`check_user_call_body_operators`], built in one pass so
/// the check does not rescan the HIR or walk ancestors once per call site.
struct BodyIndex {
    /// Function or lambda symbol -> overloads anywhere inside it.
    inside: FxHashMap<SymbolId, Vec<usize>>,
    /// Function symbol -> overloads inside it that are not nested in control flow.
    body: FxHashMap<SymbolId, Vec<usize>>,
    /// Function symbol -> its parameter symbols.
    params: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// Function symbol -> parameter calls made inside it.
    param_calls: FxHashMap<SymbolId, Vec<usize>>,
}

impl BodyIndex {
    fn new(hir: &Hir, overloads: &[DeferredOverload], param_calls: &[DeferredParameterCall]) -> Self {
        use mq_hir::SymbolKind;

        let mut inside: FxHashMap<SymbolId, Vec<usize>> = FxHashMap::default();
        let mut body: FxHashMap<SymbolId, Vec<usize>> = FxHashMap::default();
        for (i, overload) in overloads.iter().enumerate() {
            // Control flow guards may narrow types beyond what static analysis sees, so an
            // overload nested in one is not part of the checked body of the enclosing function.
            let mut in_control_flow = false;
            for (id, symbol) in walk_ancestors(hir, overload.symbol_id) {
                inside.entry(id).or_default().push(i);
                if !in_control_flow {
                    body.entry(id).or_default().push(i);
                }
                in_control_flow |= matches!(
                    symbol.kind,
                    SymbolKind::If
                        | SymbolKind::Unless
                        | SymbolKind::Elif
                        | SymbolKind::Else
                        | SymbolKind::While
                        | SymbolKind::Until
                        | SymbolKind::Loop
                        | SymbolKind::Match
                        | SymbolKind::MatchArm { .. }
                        | SymbolKind::Try
                        | SymbolKind::Catch
                        | SymbolKind::Foreach
                );
            }
        }

        let mut params: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        for (id, symbol) in hir.symbols() {
            if let Some(parent) = symbol.parent
                && symbol.is_parameter()
            {
                params.entry(parent).or_default().push(id);
            }
        }

        let mut by_outer: FxHashMap<SymbolId, Vec<usize>> = FxHashMap::default();
        for (i, call) in param_calls.iter().enumerate() {
            by_outer.entry(call.outer_def_id).or_default().push(i);
        }

        Self {
            inside,
            body,
            params,
            param_calls: by_outer,
        }
    }

    fn inside_overloads(&self, func: SymbolId) -> &[usize] {
        self.inside.get(&func).map_or(&[], Vec::as_slice)
    }

    fn body_overloads(&self, func: SymbolId) -> &[usize] {
        self.body.get(&func).map_or(&[], Vec::as_slice)
    }

    fn params(&self, func: SymbolId) -> &[SymbolId] {
        self.params.get(&func).map_or(&[], Vec::as_slice)
    }

    fn param_calls(&self, func: SymbolId) -> &[usize] {
        self.param_calls.get(&func).map_or(&[], Vec::as_slice)
    }
}
