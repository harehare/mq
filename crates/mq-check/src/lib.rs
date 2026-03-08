//! Type inference engine for mq language using Hindley-Milner type inference.
//!
//! This crate provides static type checking and type inference capabilities for mq.
//! It implements a Hindley-Milner style type inference algorithm with support for:
//! - Automatic type inference (no type annotations required)
//! - Polymorphic functions (generics)
//! - Type constraints and unification
//! - Integration with mq-hir for symbol and scope information
//! - Error location reporting with source spans
//!
//! ## Error Location Reporting
//!
//! Type errors include location information (line and column numbers) extracted from
//! the HIR symbols. This information is converted to `miette::SourceSpan` for diagnostic
//! display. The span information helps users identify exactly where type errors occur
//! in their source code.
//!
//! Example error output:
//! ```text
//! Error: Type mismatch: expected number, found string
//!   Span: SourceSpan { offset: 42, length: 6 }
//! ```

// Suppress false-positive warnings for fields used in thiserror/miette macros
#![allow(unused_assignments)]

pub mod builtin;
pub mod constraint;
pub mod infer;
pub mod narrowing;
pub mod types;
pub mod unify;

use miette::Diagnostic;
use mq_hir::{Hir, SymbolId};
use rustc_hash::FxHashMap;
use thiserror::Error;
use types::TypeScheme;

/// Result type for type checking operations
pub type Result<T> = std::result::Result<T, TypeError>;

/// Type environment mapping symbol IDs to their inferred type schemes
#[derive(Debug, Clone, Default)]
pub struct TypeEnv(FxHashMap<SymbolId, TypeScheme>);

impl TypeEnv {
    pub fn insert(&mut self, symbol_id: SymbolId, scheme: TypeScheme) {
        self.0.insert(symbol_id, scheme);
    }

    pub fn get(&self, symbol_id: &SymbolId) -> Option<&TypeScheme> {
        self.0.get(symbol_id)
    }

    pub fn get_all(&self) -> &FxHashMap<SymbolId, TypeScheme> {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a TypeEnv {
    type Item = (&'a SymbolId, &'a TypeScheme);
    type IntoIter = std::collections::hash_map::Iter<'a, SymbolId, TypeScheme>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Type checking errors
#[derive(Debug, Error, Clone, Diagnostic)]
#[allow(unused_assignments)]
pub enum TypeError {
    #[error("Type mismatch: expected {expected}, found {found}")]
    #[diagnostic(code(typechecker::type_mismatch))]
    #[allow(dead_code)]
    Mismatch {
        expected: String,
        found: String,
        #[label("type mismatch here")]
        span: Option<miette::SourceSpan>,
        location: Option<(u32, usize)>,
    },
    #[error("Cannot unify types: {left} and {right}")]
    #[diagnostic(code(typechecker::unification_error))]
    #[allow(dead_code)]
    UnificationError {
        left: String,
        right: String,
        #[label("cannot unify these types")]
        span: Option<miette::SourceSpan>,
        location: Option<(u32, usize)>,
    },
    #[error("Occurs check failed: type variable {var} occurs in {ty}")]
    #[diagnostic(code(typechecker::occurs_check))]
    #[allow(dead_code)]
    OccursCheck {
        var: String,
        ty: String,
        #[label("infinite type")]
        span: Option<miette::SourceSpan>,
        location: Option<(u32, usize)>,
    },
    #[error("Undefined symbol: {name}")]
    #[diagnostic(code(typechecker::undefined_symbol))]
    #[allow(dead_code)]
    UndefinedSymbol {
        name: String,
        #[label("undefined symbol")]
        span: Option<miette::SourceSpan>,
        location: Option<(u32, usize)>,
    },
    #[error("Wrong number of arguments: expected {expected}, found {found}")]
    #[diagnostic(code(typechecker::wrong_arity))]
    #[allow(dead_code)]
    WrongArity {
        expected: usize,
        found: usize,
        #[label("wrong number of arguments")]
        span: Option<miette::SourceSpan>,
        location: Option<(u32, usize)>,
    },
    #[error("Undefined field `{field}` in record type {record_ty}")]
    #[diagnostic(code(typechecker::undefined_field))]
    #[allow(dead_code)]
    UndefinedField {
        field: String,
        record_ty: String,
        #[label("field not found")]
        span: Option<miette::SourceSpan>,
        location: Option<(u32, usize)>,
    },
    #[error("Heterogeneous array: elements have mixed types [{types}]")]
    #[diagnostic(code(typechecker::heterogeneous_array))]
    #[allow(dead_code)]
    HeterogeneousArray {
        types: String,
        #[label("mixed types in array")]
        span: Option<miette::SourceSpan>,
        location: Option<(u32, usize)>,
    },
    #[error("Type variable not found: {0}")]
    #[diagnostic(code(typechecker::type_var_not_found))]
    TypeVarNotFound(String),
    #[error("Internal error: {0}")]
    #[diagnostic(code(typechecker::internal_error))]
    Internal(String),
}

impl TypeError {
    /// Returns the location (line, column) of the error, if available.
    pub fn location(&self) -> Option<(u32, usize)> {
        match self {
            TypeError::Mismatch { location, .. }
            | TypeError::UnificationError { location, .. }
            | TypeError::OccursCheck { location, .. }
            | TypeError::UndefinedSymbol { location, .. }
            | TypeError::WrongArity { location, .. }
            | TypeError::UndefinedField { location, .. }
            | TypeError::HeterogeneousArray { location, .. } => *location,
            _ => None,
        }
    }
}

/// Walks the HIR parent chain from `start`, yielding `(SymbolId, &Symbol)` pairs.
///
/// Begins with the parent of `start` and follows the chain upward.
/// Includes a depth limit to prevent infinite loops on cyclic structures.
pub(crate) fn walk_ancestors(
    hir: &Hir,
    start: mq_hir::SymbolId,
) -> impl Iterator<Item = (mq_hir::SymbolId, &mq_hir::Symbol)> {
    let mut current = hir.symbol(start).and_then(|s| s.parent);
    let mut depth = 0usize;
    std::iter::from_fn(move || {
        let id = current?;
        depth += 1;
        if depth > 200 {
            current = None;
            return None;
        }
        let sym = hir.symbol(id)?;
        current = sym.parent;
        Some((id, sym))
    })
}

/// Options for configuring the type checker behavior
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeCheckerOptions {
    /// When true, arrays must contain elements of a single type.
    /// Heterogeneous arrays like `[1, "hello"]` will produce a type error.
    pub strict_array: bool,
    /// When true, heterogeneous array literals are typed as tuples with per-element types.
    /// For example, `[1, "hello"]` gets type `(number, string)` and `v[0]` returns `number`.
    pub tuple: bool,
}

/// Type checker for mq programs
///
/// Provides type inference and checking capabilities based on HIR information.
pub struct TypeChecker {
    /// Symbol type mappings
    symbol_types: TypeEnv,
    /// Type checker options
    options: TypeCheckerOptions,
}

impl TypeChecker {
    /// Creates a new type checker with default options
    pub fn new() -> Self {
        Self {
            symbol_types: TypeEnv::default(),
            options: TypeCheckerOptions::default(),
        }
    }

    /// Creates a new type checker with the given options
    pub fn with_options(options: TypeCheckerOptions) -> Self {
        Self {
            symbol_types: TypeEnv::default(),
            options,
        }
    }

    /// Runs type inference on the given HIR
    ///
    /// Returns a list of type errors found. An empty list means no errors.
    pub fn check(&mut self, hir: &Hir) -> Vec<TypeError> {
        // Create inference context with options
        let mut ctx = infer::InferenceContext::with_options(self.options.strict_array, self.options.tuple);

        builtin::register_all(&mut ctx);

        // Generate constraints from HIR (collects errors internally)
        constraint::generate_constraints(hir, &mut ctx);

        // Solve constraints through unification (collects errors internally)
        unify::solve_constraints(&mut ctx);

        // Apply type narrowings from type predicate conditions (e.g., is_string(x))
        // in if/elif branches. This overrides Ref types within narrowed branches.
        narrowing::resolve_type_narrowings(hir, &mut ctx);

        // Resolve deferred tuple index accesses now that variable types are known.
        if Self::resolve_deferred_tuple_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        // Resolve deferred record field accesses now that variable types are known.
        // This binds bracket access return types (e.g., v[:key]) to specific field types
        // from Record types, enabling type error detection for subsequent operations.
        if Self::resolve_record_field_accesses(&mut ctx) {
            // Re-run unification to propagate newly resolved record field types
            unify::solve_constraints(&mut ctx);
        }

        // Resolve deferred selector field accesses (.field on records)
        Self::resolve_selector_field_accesses(&mut ctx);

        // Propagate return types from user-defined function calls.
        // After unification, the original function's return type may be concrete,
        // allowing us to connect it to the fresh return type at each call site.
        // This must run BEFORE deferred overload resolution so that operators using
        // return types have concrete operands.
        Self::propagate_user_call_returns(&mut ctx);

        // Process deferred overload resolutions (operators with type variable operands)
        // After return type propagation + unification, operand types may now be resolved.
        // Unresolved overloads are stored back for later processing.
        Self::resolve_deferred_overloads(&mut ctx);

        // Re-run deferred tuple accesses after overload resolution, because some variable
        // types (e.g., the return type of `first(xs)`) may only be resolved after
        // `resolve_deferred_overloads` runs. This ensures that index accesses on variables
        // with union types containing unresolved vars (e.g., `Union(None, Var)`) are
        // retried with the now-concrete member types.
        if Self::resolve_deferred_tuple_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        // Check operators inside user-defined function bodies against call-site types.
        // Uses local substitution (original params → call-site args) without modifying
        // global state, so multiple call sites don't interfere.
        Self::check_user_call_body_operators(hir, &mut ctx);

        // Collect errors before finalizing
        let errors = ctx.take_errors();

        // Store inferred types
        self.symbol_types = ctx.finalize();

        errors
    }

    /// Resolves deferred record field accesses after the first round of unification.
    ///
    /// For each deferred bracket access `v[:key]`, resolves the variable's type
    /// (now concrete after unification) and, if it is a Record, looks up the field
    /// type and binds the bracket access expression's type to that field type.
    fn resolve_record_field_accesses(ctx: &mut infer::InferenceContext) -> bool {
        let accesses = ctx.take_deferred_record_accesses();
        if accesses.is_empty() {
            return false;
        }

        let mut resolved_any = false;
        for access in &accesses {
            // Resolve the variable's type (should now be Record after unification)
            let var_ty = match ctx.get_symbol_type(access.def_id).cloned() {
                Some(ty) => ctx.resolve_type(&ty),
                None => continue,
            };

            if let types::Type::Record(fields, rest) = &var_ty {
                if let Some(field_ty) = fields.get(&access.field_name) {
                    // Bind the bracket access expression's type to the field type
                    let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                    ctx.add_constraint(constraint::Constraint::Equal(call_ty, field_ty.clone(), None));
                    resolved_any = true;
                } else if matches!(rest.as_ref(), types::Type::RowEmpty) {
                    // Field not found in closed record — report error
                    ctx.add_error(TypeError::UndefinedField {
                        field: access.field_name.clone(),
                        record_ty: var_ty.display_renumbered(),
                        span: access.range.as_ref().map(unify::range_to_span),
                        location: access.range.as_ref().map(|r| (r.start.line, r.start.column)),
                    });
                }
            }
        }
        resolved_any
    }

    /// Resolves deferred selector field accesses after unification.
    ///
    /// For each deferred selector access `.field`, resolves the piped input type
    /// (now concrete after unification) and checks if the field exists in the record.
    fn resolve_selector_field_accesses(ctx: &mut infer::InferenceContext) {
        let accesses = ctx.take_deferred_selector_accesses();
        if accesses.is_empty() {
            return;
        }

        for access in &accesses {
            let resolved = ctx.resolve_type(&access.piped_ty);

            if let types::Type::Record(fields, rest) = &resolved {
                if let Some(field_ty) = fields.get(&access.field_name) {
                    // Bind the selector's type to the field type
                    let sel_ty = ctx.get_or_create_symbol_type(access.symbol_id);
                    ctx.add_constraint(constraint::Constraint::Equal(sel_ty, field_ty.clone(), None));
                } else if matches!(rest.as_ref(), types::Type::RowEmpty) {
                    // Field not found in closed record — report error
                    ctx.add_error(TypeError::UndefinedField {
                        field: access.field_name.clone(),
                        record_ty: resolved.display_renumbered(),
                        span: access.range.as_ref().map(unify::range_to_span),
                        location: access.range.as_ref().map(|r| (r.start.line, r.start.column)),
                    });
                }
            }
        }
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
    fn resolve_deferred_tuple_accesses(ctx: &mut infer::InferenceContext) -> bool {
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
                    ctx.add_constraint(constraint::Constraint::Equal(call_ty, result_ty, None));
                    resolved_any = true;
                }
                types::Type::Array(elem) => {
                    // Normal array — bind to element type
                    let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                    ctx.add_constraint(constraint::Constraint::Equal(call_ty, *elem.clone(), None));
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
                        ctx.add_constraint(constraint::Constraint::Equal(call_ty, result_ty, None));
                        resolved_any = true;
                    }
                }
                types::Type::Var(_) => {
                    // Type variable not yet resolved — re-queue for next pass.
                    // Do NOT add an Array constraint here: if the variable later resolves
                    // to a Tuple or other type, the premature Array constraint would
                    // conflict and produce spurious "infinite type" or mismatch errors.
                    ctx.add_deferred_tuple_access(access.clone());
                }
                _ => {
                    // Known non-array/tuple/union type — add array constraint as fallback
                    let elem_var = ctx.fresh_var();
                    let elem_ty = types::Type::Var(elem_var);
                    ctx.add_constraint(constraint::Constraint::Equal(
                        var_ty.clone(),
                        types::Type::array(elem_ty.clone()),
                        access.range,
                    ));
                    let call_ty = ctx.get_or_create_symbol_type(access.call_symbol_id);
                    ctx.add_constraint(constraint::Constraint::Equal(call_ty, elem_ty, None));
                    resolved_any = true;
                }
            }
        }
        resolved_any
    }

    /// Checks whether all non-None-propagating members of every union-typed argument
    /// resolve to the same return type when applied to the given operator.
    ///
    /// Returns `Some(return_type)` if every non-None member produces an identical return
    /// type, indicating the operation is safe for the non-None cases. Returns `None` if
    /// any member fails to match an overload, any member returns a different type, or
    /// any union contains unresolved type variables.
    ///
    /// None members that follow the standard none-propagation pattern (`f(None) -> None`)
    /// are skipped when determining consistency, since they are an expected dynamic
    /// behavior in mq (e.g. `len(None)` returns None while `len(Array) → Number`).
    fn union_members_consistent_return(
        ctx: &mut infer::InferenceContext,
        op_name: &str,
        resolved_operands: &[types::Type],
    ) -> Option<types::Type> {
        let mut unique_ret: Option<types::Type> = None;
        for (i, arg_ty) in resolved_operands.iter().enumerate() {
            let types::Type::Union(members) = arg_ty else {
                continue;
            };

            for member in members {
                // Reject unions containing unresolved type variables
                if member.is_var() {
                    return None;
                }

                let mut test_args = resolved_operands.to_vec();
                test_args[i] = member.clone();
                let Some(types::Type::Function(_, member_ret)) = ctx.resolve_overload(op_name, &test_args) else {
                    return None;
                };
                let resolved_ret = ctx.resolve_type(&member_ret);

                // Skip None-propagation overloads: a None input producing a None output
                // is the standard mq "propagate None" pattern and does not affect the
                // consistency of the return type for non-None inputs.
                if matches!(member, types::Type::None) && matches!(resolved_ret, types::Type::None) {
                    continue;
                }

                match &unique_ret {
                    None => unique_ret = Some(resolved_ret),
                    Some(prev) if prev == &resolved_ret => {}
                    _ => return None,
                }
            }
        }
        unique_ret
    }

    /// Resolves deferred overloads after the first round of unification.
    ///
    /// Binary/unary operators whose operands were type variables during constraint
    /// generation are re-processed now that operand types may be known.
    /// Runs unification after each resolution so that type information propagates
    /// incrementally to subsequent deferred overloads.
    fn resolve_deferred_overloads(ctx: &mut infer::InferenceContext) {
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
        let max_passes = 3;
        for _ in 0..max_passes {
            let mut next_remaining = Vec::new();

            for &idx in &remaining_indices {
                let d = &deferred[idx];
                let resolved_operands: Vec<types::Type> = d.operand_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();

                let all_concrete = resolved_operands.iter().all(|ty| !ty.is_var());
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
                                ctx.add_constraint(constraint::Constraint::Equal(
                                    operand_ty.clone(),
                                    param_ty.clone(),
                                    d.range,
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
                    if let Some(consistent_ret) =
                        Self::union_members_consistent_return(ctx, &d.op_name, &resolved_operands)
                    {
                        ctx.set_symbol_type_no_bind(d.symbol_id, consistent_ret);
                        unify::solve_constraints(ctx);
                        continue;
                    }
                    let args_str = resolved_operands
                        .iter()
                        .map(|t| t.display_renumbered())
                        .collect::<Vec<_>>()
                        .join(", ");
                    ctx.add_error(TypeError::UnificationError {
                        left: format!("{} with arguments ({})", d.op_name, args_str),
                        right: "union types cannot be used with binary operators".to_string(),
                        span: d.range.as_ref().map(unify::range_to_span),
                        location: d.range.as_ref().map(|r| (r.start.line, r.start.column)),
                    });
                    continue;
                }

                // Skip resolution when any operand is still a type variable
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
                            ctx.add_constraint(constraint::Constraint::Equal(
                                operand_ty.clone(),
                                param_ty.clone(),
                                d.range,
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
                // No progress — resolve remaining with best-effort
                for &idx in &next_remaining {
                    let d = &deferred[idx];
                    let resolved_operands: Vec<types::Type> =
                        d.operand_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();

                    // Don't resolve when any operand is still a type variable
                    // and there are multiple overloads — store back for user call body checking
                    let any_var_best = resolved_operands.iter().any(|ty| ty.is_var());
                    if any_var_best {
                        let overload_count = ctx.get_builtin_overloads(&d.op_name).map(|o| o.len()).unwrap_or(0);
                        if overload_count > 1 {
                            ctx.add_deferred_overload(d.clone());
                            continue;
                        }
                    }

                    if let Some(resolved_ty) = ctx.resolve_overload(&d.op_name, &resolved_operands) {
                        if let types::Type::Function(param_tys, ret_ty) = resolved_ty
                            && param_tys.len() == d.operand_tys.len()
                        {
                            for (operand_ty, param_ty) in d.operand_tys.iter().zip(param_tys.iter()) {
                                ctx.add_constraint(constraint::Constraint::Equal(
                                    operand_ty.clone(),
                                    param_ty.clone(),
                                    d.range,
                                ));
                            }
                            ctx.set_symbol_type_no_bind(d.symbol_id, *ret_ty);
                        }
                    } else {
                        let all_concrete = resolved_operands.iter().all(|ty| !ty.is_var());
                        if all_concrete {
                            ctx.report_no_matching_overload(&d.op_name, &resolved_operands, d.range);
                        } else {
                            // Still unresolved — store back for later processing
                            ctx.add_deferred_overload(d.clone());
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

        // Final unification pass
        unify::solve_constraints(ctx);
    }

    /// Propagates return types from user-defined function calls.
    ///
    /// After unification, the original function's return type is resolved from its body.
    /// This method connects each call site's fresh return type to the original resolved
    /// return type, enabling downstream operators to resolve with concrete types.
    fn propagate_user_call_returns(ctx: &mut infer::InferenceContext) {
        let deferred_calls = ctx.take_deferred_user_calls();

        for call in &deferred_calls {
            if let Some(orig_ty) = ctx.get_symbol_type(call.def_id).cloned() {
                let resolved_orig_ty = ctx.resolve_type(&orig_ty);
                if let types::Type::Function(_, orig_ret) = &resolved_orig_ty {
                    let resolved_ret = ctx.resolve_type(orig_ret);
                    if !resolved_ret.is_var() {
                        ctx.add_constraint(constraint::Constraint::Equal(
                            call.fresh_ret_ty.clone(),
                            resolved_ret,
                            call.range,
                        ));
                    }
                }
            }
        }

        // Solve new constraints from return type propagation
        unify::solve_constraints(ctx);

        // Store calls back for body operator checking
        for call in deferred_calls {
            ctx.add_deferred_user_call(call);
        }
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
    fn check_user_call_body_operators(hir: &Hir, ctx: &mut infer::InferenceContext) {
        let deferred_calls = ctx.take_deferred_user_calls();
        let unresolved_overloads = ctx.take_deferred_overloads();

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
            let mut subst = types::Substitution::empty();
            for (orig_param, arg_ty) in orig_params.iter().zip(resolved_args.iter()) {
                Self::extract_structural_subst(orig_param, arg_ty, &mut subst, ctx);
            }

            // Check each unresolved overload that belongs to this function's body.
            // Uses iterative resolution: when an inner operator resolves (e.g. x + 1 → Number),
            // its result type is added to the substitution so outer operators that depend on it
            // (e.g. (x + 1) + true) can also be checked.
            let body_overloads: Vec<_> = unresolved_overloads
                .iter()
                .filter(|d| {
                    Self::is_symbol_inside_function(hir, d.symbol_id, call.def_id)
                        && !Self::is_inside_control_flow(hir, d.symbol_id, call.def_id)
                })
                .collect();

            Self::check_deferred_overloads_iteratively(&body_overloads, &mut subst, ctx, call.range);

            // Check operators inside lambda arguments via DeferredParameterCalls.
            //
            // When a lambda `fn(x): x + true;` is passed to a higher-order function
            // (e.g. `apply_to_all([1,2,3], f)`), the lambda is called inside with
            // each array element. The DeferredParameterCalls collected during constraint
            // generation record the argument types of those inner calls (e.g. the foreach
            // variable `x`). Resolving those arg types with the main substitution
            // yields the concrete element type (e.g. `Number`), which becomes the
            // lambda's parameter substitution for checking its body operators.
            let deferred_param_calls = ctx.deferred_parameter_calls().to_vec();
            // Collect outer function's parameter symbol IDs to match against inner calls
            let outer_param_syms: Vec<SymbolId> = Self::get_function_params(hir, call.def_id);
            for param_call in &deferred_param_calls {
                if param_call.outer_def_id != call.def_id {
                    continue;
                }

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
                let mut lambda_subst = types::Substitution::empty();
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
                let lambda_overloads: Vec<_> = unresolved_overloads
                    .iter()
                    .filter(|d| Self::is_symbol_inside_function(hir, d.symbol_id, lambda_sym_id))
                    .collect();

                Self::check_deferred_overloads_iteratively(&lambda_overloads, &mut lambda_subst, ctx, call.range);
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
        overloads: &[&infer::DeferredOverload],
        subst: &mut types::Substitution,
        ctx: &mut infer::InferenceContext,
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

                // Skip if any operand is still a type variable after substitution
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
        subst: &mut types::Substitution,
        ctx: &mut infer::InferenceContext,
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
                    Self::extract_structural_subst(orig_elem, arg_elem, subst, ctx);
                }
            }
            // Function types are handled by the lambda-body checking phase — skip here.
            _ => {}
        }
    }

    /// Returns the HIR symbol IDs of all parameter symbols for a function definition.
    fn get_function_params(hir: &Hir, func_def_id: SymbolId) -> Vec<SymbolId> {
        hir.symbols()
            .filter_map(|(id, sym)| {
                if sym.parent == Some(func_def_id) && sym.is_parameter() {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Checks if a symbol is inside a function body by walking the HIR parent chain.
    /// Includes a depth limit to prevent stack overflow on deeply nested or cyclic structures.
    fn is_symbol_inside_function(hir: &Hir, symbol_id: SymbolId, func_id: SymbolId) -> bool {
        for (id, _) in walk_ancestors(hir, symbol_id) {
            if id == func_id {
                return true;
            }
        }
        false
    }

    /// Checks if a symbol is inside a control flow construct (If, Elif, Else, While, Loop,
    /// Match, MatchArm, Try, Catch, Foreach) between itself and the function definition.
    ///
    /// This is used to skip operator checking inside type-guarded branches, where runtime
    /// type checks narrow the type beyond what static analysis can determine.
    fn is_inside_control_flow(hir: &Hir, symbol_id: SymbolId, func_id: SymbolId) -> bool {
        use mq_hir::SymbolKind;
        for (id, symbol) in walk_ancestors(hir, symbol_id) {
            if id == func_id {
                return false;
            }
            if matches!(
                symbol.kind,
                SymbolKind::If
                    | SymbolKind::Elif
                    | SymbolKind::Else
                    | SymbolKind::While
                    | SymbolKind::Loop
                    | SymbolKind::Match
                    | SymbolKind::MatchArm
                    | SymbolKind::Try
                    | SymbolKind::Catch
                    | SymbolKind::Foreach
            ) {
                return true;
            }
        }
        false
    }

    /// Gets the type of a symbol
    pub fn type_of(&self, symbol: SymbolId) -> Option<&TypeScheme> {
        self.symbol_types.get(&symbol)
    }

    /// Gets all symbol types
    pub fn symbol_types(&self) -> &TypeEnv {
        &self.symbol_types
    }

    /// Gets the type scheme for a specific symbol.
    pub fn symbol_type(&self, symbol_id: SymbolId) -> Option<&TypeScheme> {
        self.symbol_types.get(&symbol_id)
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typechecker_creation() {
        let checker = TypeChecker::new();
        assert_eq!(checker.symbol_types.len(), 0);
    }
}
