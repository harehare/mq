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
pub mod types;
pub mod unify;

use miette::Diagnostic;
use mq_hir::{Hir, SymbolId};
use rustc_hash::FxHashMap;
use thiserror::Error;
use types::TypeScheme;

/// Result type for type checking operations
pub type Result<T> = std::result::Result<T, TypeError>;

/// Type checking errors
#[derive(Debug, Error, Diagnostic)]
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
            | TypeError::WrongArity { location, .. } => *location,
            _ => None,
        }
    }
}

/// Type checker for mq programs
///
/// Provides type inference and checking capabilities based on HIR information.
pub struct TypeChecker {
    /// Symbol type mappings
    symbol_types: FxHashMap<SymbolId, TypeScheme>,
}

impl TypeChecker {
    /// Creates a new type checker
    pub fn new() -> Self {
        Self {
            symbol_types: FxHashMap::default(),
        }
    }

    /// Runs type inference on the given HIR
    ///
    /// Returns a list of type errors found. An empty list means no errors.
    pub fn check(&mut self, hir: &Hir) -> Vec<TypeError> {
        // Create inference context
        let mut ctx = infer::InferenceContext::new();

        builtin::register_all(&mut ctx);

        // Generate constraints from HIR (collects errors internally)
        constraint::generate_constraints(hir, &mut ctx);

        // Solve constraints through unification (collects errors internally)
        unify::solve_constraints(&mut ctx);

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

        // Resolve deferred overloads in multiple passes.
        // Each pass resolves overloads that have at least one concrete operand.
        // Overloads with all-Var operands are deferred to subsequent passes,
        // as intermediate unification may resolve their types.
        let mut remaining = deferred;
        let max_passes = 3;
        for _ in 0..max_passes {
            let mut next_remaining = Vec::new();

            for d in &remaining {
                let resolved_operands: Vec<types::Type> = d.operand_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();

                let all_concrete = resolved_operands.iter().all(|ty| !ty.is_var());
                let has_union = resolved_operands.iter().any(|ty| ty.is_union());

                if has_union {
                    // Try to find a polymorphic overload where every union-typed argument
                    // is matched to a type-variable parameter (e.g. `to_number: (Var) -> Number`).
                    // Such overloads accept any input and the generated constraint
                    // (`Union == Var(fresh)`) unifies without error.
                    // Binary operators like `+` only have concrete parameter overloads
                    // (e.g. `(Number, Number) -> Number`), so they correctly fall through
                    // to the error below.
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
                // This is critical for function body operators where parameter type variables
                // must remain unresolved so check_user_call_body_operators can substitute
                // call-site argument types per call site.
                //
                // However, even with type variables we can detect definitive errors:
                // if NO overload could possibly match given the concrete operands, the
                // operation is invalid regardless of what the type variables resolve to.
                // For example, `x + true` where x is a Var has no valid `+` overload
                // (since no overload accepts Bool as an argument), so it is always wrong.
                let any_var = resolved_operands.iter().any(|ty| ty.is_var());
                if any_var {
                    let overload_count = ctx.get_builtin_overloads(&d.op_name).map(|o| o.len()).unwrap_or(0);
                    if overload_count > 1 {
                        if ctx.resolve_overload(&d.op_name, &resolved_operands).is_none() {
                            // No overload can possibly match — report error immediately.
                            let args_str = resolved_operands
                                .iter()
                                .map(|t| t.display_renumbered())
                                .collect::<Vec<_>>()
                                .join(", ");
                            ctx.add_error(TypeError::UnificationError {
                                left: format!("{} with arguments ({})", d.op_name, args_str),
                                right: "no matching overload".to_string(),
                                span: d.range.as_ref().map(unify::range_to_span),
                                location: d.range.as_ref().map(|r| (r.start.line, r.start.column)),
                            });
                        } else {
                            next_remaining.push(d.clone());
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
                    let args_str = resolved_operands
                        .iter()
                        .map(|t| t.display_renumbered())
                        .collect::<Vec<_>>()
                        .join(", ");
                    ctx.add_error(TypeError::UnificationError {
                        left: format!("{} with arguments ({})", d.op_name, args_str),
                        right: "no matching overload".to_string(),
                        span: d.range.as_ref().map(unify::range_to_span),
                        location: d.range.as_ref().map(|r| (r.start.line, r.start.column)),
                    });
                } else {
                    // Some operands resolved but no match — defer to next pass
                    next_remaining.push(d.clone());
                }
            }

            if next_remaining.len() == remaining.len() {
                // No progress — resolve remaining with best-effort
                for d in &next_remaining {
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
                            let args_str = resolved_operands
                                .iter()
                                .map(|t| t.display_renumbered())
                                .collect::<Vec<_>>()
                                .join(", ");
                            ctx.add_error(TypeError::UnificationError {
                                left: format!("{} with arguments ({})", d.op_name, args_str),
                                right: "no matching overload".to_string(),
                                span: d.range.as_ref().map(unify::range_to_span),
                                location: d.range.as_ref().map(|r| (r.start.line, r.start.column)),
                            });
                        } else {
                            // Still unresolved — store back for later processing
                            ctx.add_deferred_overload(d.clone());
                        }
                    }
                }
                break;
            }

            remaining = next_remaining;
            if remaining.is_empty() {
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

            // Check each unresolved overload that belongs to this function's body
            for d in &unresolved_overloads {
                if !Self::is_symbol_inside_function(hir, d.symbol_id, call.def_id) {
                    continue;
                }

                // Skip operators inside control flow constructs (If/Elif/Else/While/
                // Match/Try/Catch). These branches often have runtime type guards
                // (e.g., `if (is_dict(v)): keys(v)`) that narrow types beyond what
                // static analysis can determine, causing false positives.
                if Self::is_inside_control_flow(hir, d.symbol_id, call.def_id) {
                    continue;
                }

                // Apply substitution to get concrete operand types
                let substituted_operands: Vec<types::Type> = d
                    .operand_tys
                    .iter()
                    .map(|ty| {
                        let resolved = ctx.resolve_type(ty);
                        resolved.apply_subst(&subst)
                    })
                    .collect();

                // Skip if any operand is still a type variable after substitution
                if substituted_operands.iter().any(|ty| ty.is_var()) {
                    continue;
                }

                // Check if the operator has a matching overload with these types
                if ctx.resolve_overload(&d.op_name, &substituted_operands).is_none() {
                    let args_str = substituted_operands
                        .iter()
                        .map(|t| t.display_renumbered())
                        .collect::<Vec<_>>()
                        .join(", ");
                    ctx.add_error(TypeError::UnificationError {
                        left: format!("{} with arguments ({})", d.op_name, args_str),
                        right: "no matching overload".to_string(),
                        span: call.range.as_ref().map(unify::range_to_span),
                        location: call.range.as_ref().map(|r| (r.start.line, r.start.column)),
                    });
                }
            }

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

                // Check deferred overloads inside the lambda body
                for d in &unresolved_overloads {
                    if !Self::is_symbol_inside_function(hir, d.symbol_id, lambda_sym_id) {
                        continue;
                    }

                    let substituted_operands: Vec<types::Type> = d
                        .operand_tys
                        .iter()
                        .map(|ty| ctx.resolve_type(ty).apply_subst(&lambda_subst))
                        .collect();

                    if substituted_operands.iter().any(|ty| ty.is_var()) {
                        continue;
                    }

                    if ctx.resolve_overload(&d.op_name, &substituted_operands).is_none() {
                        let args_str = substituted_operands
                            .iter()
                            .map(|t| t.display_renumbered())
                            .collect::<Vec<_>>()
                            .join(", ");
                        ctx.add_error(TypeError::UnificationError {
                            left: format!("{} with arguments ({})", d.op_name, args_str),
                            right: "no matching overload".to_string(),
                            span: d.range.as_ref().map(unify::range_to_span),
                            location: d.range.as_ref().map(|r| (r.start.line, r.start.column)),
                        });
                    }
                }
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
        let mut current = symbol_id;
        let mut depth = 0;
        const MAX_DEPTH: usize = 200;
        loop {
            if current == func_id {
                return true;
            }
            depth += 1;
            if depth > MAX_DEPTH {
                return false;
            }
            match hir.symbol(current).and_then(|s| s.parent) {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }

    /// Checks if a symbol is inside a control flow construct (If, Elif, Else, While, Loop,
    /// Match, MatchArm, Try, Catch, Foreach) between itself and the function definition.
    ///
    /// This is used to skip operator checking inside type-guarded branches, where runtime
    /// type checks narrow the type beyond what static analysis can determine.
    fn is_inside_control_flow(hir: &Hir, symbol_id: SymbolId, func_id: SymbolId) -> bool {
        use mq_hir::SymbolKind;
        let mut current = symbol_id;
        let mut depth = 0;
        const MAX_DEPTH: usize = 200;
        loop {
            if current == func_id {
                return false;
            }
            depth += 1;
            if depth > MAX_DEPTH {
                return false;
            }
            match hir.symbol(current).and_then(|s| {
                let kind = &s.kind;
                s.parent.map(|p| (p, kind.clone()))
            }) {
                Some((parent, kind)) => {
                    if matches!(
                        kind,
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
                    current = parent;
                }
                None => return false,
            }
        }
    }

    /// Gets the type of a symbol
    pub fn type_of(&self, symbol: SymbolId) -> Option<&TypeScheme> {
        self.symbol_types.get(&symbol)
    }

    /// Gets all symbol types
    pub fn symbol_types(&self) -> &FxHashMap<SymbolId, TypeScheme> {
        &self.symbol_types
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
