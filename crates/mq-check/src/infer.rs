//! Type inference context and engine.

use crate::constraint::Constraint;
use crate::types::{Substitution, Type, TypeScheme, TypeVarContext, TypeVarId, format_type_list};
use crate::{TypeEnv, TypeError, unify};
use mq_hir::SymbolId;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

/// A deferred overload resolution for operators with unresolved type variable operands.
///
/// When a binary or unary operator's operands are still type variables during constraint
/// generation, we defer the overload resolution until after the first round of unification
/// when the operand types may have been resolved.
#[derive(Debug, Clone)]
pub struct DeferredOverload {
    /// The symbol ID of the operator
    pub symbol_id: SymbolId,
    /// The operator name (e.g., "+", "-", "*")
    pub op_name: SmolStr,
    /// The original type variables for the operands
    pub operand_tys: Vec<Type>,
    /// The source range for error reporting
    pub range: Option<mq_lang::Range>,
}

/// A deferred inner call to a function parameter (higher-order function pattern).
///
/// When a parameter `f` (with type `Var(tv_f)`) is called inside a function body,
/// e.g. `f(x)` inside `apply_to_all(v, f)`, we track the call so that
/// `check_user_call_body_operators` can determine the concrete element type that
/// the lambda argument receives at the outer call site.
#[derive(Debug, Clone)]
pub struct DeferredParameterCall {
    /// The enclosing function definition symbol that contains this inner call
    pub outer_def_id: SymbolId,
    /// The parameter symbol being called (e.g., the `f` parameter symbol)
    pub param_sym_id: SymbolId,
    /// The argument types at this inner call site (e.g., `[type_of_x]`)
    pub arg_tys: Vec<Type>,
}

/// A deferred user-defined function call for post-unification type checking.
///
/// After unification, the original function's return type will be resolved from
/// its body, and we can propagate it to the call site and verify argument types.
#[derive(Debug, Clone)]
pub struct DeferredUserCall {
    /// The call site symbol ID
    pub call_symbol_id: SymbolId,
    /// The function definition symbol ID
    pub def_id: SymbolId,
    /// The fresh (instantiated) parameter types at this call site
    pub fresh_param_tys: Vec<Type>,
    /// The fresh (instantiated) return type at this call site
    pub fresh_ret_ty: Type,
    /// The actual argument types at the call site
    pub arg_tys: Vec<Type>,
    /// The HIR symbol IDs of each argument expression (used to identify lambda arguments
    /// for checking their body operators against call-site concrete types).
    pub arg_symbol_ids: Vec<SymbolId>,
    /// Source range for error reporting
    pub range: Option<mq_lang::Range>,
}

/// A deferred record field access for post-unification resolution.
///
/// When bracket access `v[:key]` is detected and the variable's type is not yet
/// resolved to a Record during constraint generation, we defer the field type
/// resolution until after unification.
#[derive(Debug, Clone)]
pub struct DeferredRecordAccess {
    /// The call symbol ID (the bracket access expression)
    pub call_symbol_id: SymbolId,
    /// The variable definition symbol ID
    pub def_id: SymbolId,
    /// The field name being accessed
    pub field_name: String,
    /// Source range for error reporting
    pub range: Option<mq_lang::Range>,
}

/// A deferred selector field access for post-unification resolution.
///
/// When `.field` selector is encountered and the piped input type is not yet
/// resolved to a Record, we defer the field existence check until after
/// unification.
#[derive(Debug, Clone)]
pub struct DeferredSelectorAccess {
    /// The selector symbol ID
    pub symbol_id: SymbolId,
    /// The piped input type (may be a type variable before unification)
    pub piped_ty: Type,
    /// The field name being accessed
    pub field_name: String,
    /// Source range for error reporting
    pub range: Option<mq_lang::Range>,
}

/// A deferred tuple index access for post-unification resolution.
///
/// When `v[0]` is encountered on a variable that may be a Tuple type,
/// we defer the index type resolution until after unification, when
/// the variable's Tuple type is fully resolved.
#[derive(Debug, Clone)]
pub struct DeferredTupleAccess {
    /// The call symbol ID (the bracket access expression)
    pub call_symbol_id: SymbolId,
    /// The variable definition symbol ID
    pub def_id: SymbolId,
    /// The literal index value (None if dynamic index)
    pub index: Option<usize>,
    /// Source range for error reporting
    pub range: Option<mq_lang::Range>,
}

/// A single variable type narrowing entry.
///
/// Represents a narrowing of a variable's type within a specific branch,
/// e.g., `is_string(x)` narrows `x` to `String` in the then-branch.
#[derive(Debug, Clone)]
pub struct NarrowingEntry {
    /// The variable definition symbol ID being narrowed
    pub def_id: SymbolId,
    /// The predicate type (e.g., Number for is_number)
    pub narrowed_type: Type,
    /// If true, narrow AWAY from the type (subtract from union).
    /// If false, narrow TO the type directly.
    pub is_complement: bool,
}

/// Collected type narrowing information for an if/elif expression.
///
/// When a type predicate like `is_string(x)` is used as a condition in an if
/// expression, the then-branch can narrow `x` to `String` and the else-branch
/// can narrow `x` to the complement (e.g., `Number` if `x: String | Number`).
/// Supports compound conditions with `&&`, `||`, and `!`.
#[derive(Debug, Clone)]
pub struct TypeNarrowing {
    /// Narrowings to apply in the then-branch (condition is true)
    pub then_narrowings: Vec<NarrowingEntry>,
    /// Narrowings to apply in the else/elif branches (condition is false)
    pub else_narrowings: Vec<NarrowingEntry>,
    /// The then-branch symbol ID
    pub then_branch_id: SymbolId,
    /// The else/elif branch symbol IDs where complement narrowings apply
    pub else_branch_ids: Vec<SymbolId>,
}

/// Inference context maintains state during type inference
pub struct InferenceContext {
    /// Type variable context for generating fresh variables
    var_ctx: TypeVarContext,
    /// Mapping from symbols to their types
    symbol_types: FxHashMap<SymbolId, Type>,
    /// Type constraints to be solved
    constraints: Vec<Constraint>,
    /// Type variable substitutions (unified types)
    substitutions: FxHashMap<TypeVarId, Type>,
    /// Builtin function/operator type signatures (can have multiple overloads)
    builtins: FxHashMap<SmolStr, Vec<Type>>,
    /// Collected type errors (for non-fatal error reporting)
    errors: Vec<TypeError>,
    /// Piped input types for symbols in a pipe chain
    piped_inputs: FxHashMap<SymbolId, Type>,
    /// Deferred overload resolutions for operators with unresolved type variable operands,
    /// keyed by `SymbolId` so that insert/replace is O(1).
    deferred_overloads: FxHashMap<SymbolId, DeferredOverload>,
    /// Deferred user-defined function calls for post-unification type checking
    deferred_user_calls: Vec<DeferredUserCall>,
    /// Deferred inner calls to function parameters for higher-order function checking
    deferred_parameter_calls: Vec<DeferredParameterCall>,
    /// Deferred record field accesses for post-unification resolution
    deferred_record_accesses: Vec<DeferredRecordAccess>,
    /// Deferred selector field accesses for post-unification resolution
    deferred_selector_accesses: Vec<DeferredSelectorAccess>,
    /// Deferred tuple index accesses for post-unification resolution
    deferred_tuple_accesses: Vec<DeferredTupleAccess>,
    /// Type narrowings collected from type predicate conditions in if/elif expressions
    type_narrowings: Vec<TypeNarrowing>,
    /// When true, heterogeneous arrays produce a type error
    strict_array: bool,
}

impl InferenceContext {
    /// Creates a new inference context with default options
    pub fn new() -> Self {
        Self::with_options(false)
    }

    /// Creates a new inference context with the given options
    pub fn with_options(strict_array: bool) -> Self {
        Self {
            var_ctx: TypeVarContext::new(),
            symbol_types: FxHashMap::default(),
            constraints: Vec::new(),
            substitutions: FxHashMap::default(),
            builtins: FxHashMap::default(),
            errors: Vec::new(),
            piped_inputs: FxHashMap::default(),
            deferred_overloads: FxHashMap::default(),
            deferred_user_calls: Vec::new(),
            deferred_parameter_calls: Vec::new(),
            deferred_record_accesses: Vec::new(),
            deferred_selector_accesses: Vec::new(),
            deferred_tuple_accesses: Vec::new(),
            type_narrowings: Vec::new(),
            strict_array,
        }
    }

    /// Returns whether strict array mode is enabled
    pub fn strict_array(&self) -> bool {
        self.strict_array
    }

    /// Registers a builtin function or operator type
    pub fn register_builtin(&mut self, name: &str, ty: Type) {
        self.builtins.entry(SmolStr::new(name)).or_default().push(ty);
    }

    /// Gets all overloaded types for a builtin function or operator
    pub fn get_builtin_overloads(&self, name: &str) -> Option<&[Type]> {
        self.builtins.get(name).map(|v| v.as_slice())
    }

    /// Adds a type error to the error collection
    pub fn add_error(&mut self, error: TypeError) {
        self.errors.push(error);
    }

    /// Takes all collected errors (consumes them)
    pub fn take_errors(&mut self) -> Vec<TypeError> {
        std::mem::take(&mut self.errors)
    }

    /// Sets the piped input type for a symbol
    pub fn set_piped_input(&mut self, symbol: SymbolId, ty: Type) {
        self.piped_inputs.insert(symbol, ty);
    }

    /// Gets the piped input type for a symbol
    pub fn get_piped_input(&self, symbol: SymbolId) -> Option<&Type> {
        self.piped_inputs.get(&symbol)
    }

    /// Adds a deferred overload resolution.
    ///
    /// For a given `symbol_id` there should be at most one deferred overload entry.
    /// If an entry for the same symbol already exists, it is replaced with the new
    /// one. This keeps the deferred operand types in sync when a node is re-processed
    /// after its operand types or argument list have changed (for example, due to
    /// additional inference or piped input being attached).
    ///
    /// The map-backed storage makes both insert and replace O(1).
    pub fn add_deferred_overload(&mut self, deferred: DeferredOverload) {
        self.deferred_overloads.insert(deferred.symbol_id, deferred);
    }

    /// Takes all deferred overloads (consumes them)
    pub fn take_deferred_overloads(&mut self) -> Vec<DeferredOverload> {
        self.deferred_overloads.drain().map(|(_, v)| v).collect()
    }

    /// Adds a deferred user-defined function call
    pub fn add_deferred_user_call(&mut self, call: DeferredUserCall) {
        self.deferred_user_calls.push(call);
    }

    /// Takes all deferred user calls (consumes them)
    pub fn take_deferred_user_calls(&mut self) -> Vec<DeferredUserCall> {
        std::mem::take(&mut self.deferred_user_calls)
    }

    /// Adds a deferred parameter call (inner call to a function parameter)
    pub fn add_deferred_parameter_call(&mut self, call: DeferredParameterCall) {
        self.deferred_parameter_calls.push(call);
    }

    /// Returns a reference to all deferred parameter calls
    pub fn deferred_parameter_calls(&self) -> &[DeferredParameterCall] {
        &self.deferred_parameter_calls
    }

    /// Adds a deferred record field access for post-unification resolution
    pub fn add_deferred_record_access(&mut self, access: DeferredRecordAccess) {
        self.deferred_record_accesses.push(access);
    }

    /// Takes all deferred record accesses (consumes them)
    pub fn take_deferred_record_accesses(&mut self) -> Vec<DeferredRecordAccess> {
        std::mem::take(&mut self.deferred_record_accesses)
    }

    /// Adds a deferred selector field access for post-unification resolution
    pub fn add_deferred_selector_access(&mut self, access: DeferredSelectorAccess) {
        self.deferred_selector_accesses.push(access);
    }

    /// Takes all deferred selector accesses (consumes them)
    pub fn take_deferred_selector_accesses(&mut self) -> Vec<DeferredSelectorAccess> {
        std::mem::take(&mut self.deferred_selector_accesses)
    }

    /// Adds a deferred tuple index access for post-unification resolution
    pub fn add_deferred_tuple_access(&mut self, access: DeferredTupleAccess) {
        self.deferred_tuple_accesses.push(access);
    }

    /// Takes all deferred tuple accesses (consumes them)
    pub fn take_deferred_tuple_accesses(&mut self) -> Vec<DeferredTupleAccess> {
        std::mem::take(&mut self.deferred_tuple_accesses)
    }

    /// Adds a type narrowing collected from a type predicate condition
    pub fn add_type_narrowing(&mut self, narrowing: TypeNarrowing) {
        self.type_narrowings.push(narrowing);
    }

    /// Takes all collected type narrowings (consumes them)
    pub fn take_type_narrowings(&mut self) -> Vec<TypeNarrowing> {
        std::mem::take(&mut self.type_narrowings)
    }

    /// Reports a type mismatch error between two types.
    ///
    /// Resolves both types before formatting them, and constructs the error with
    /// source location info from `range`. Use this instead of hand-rolling
    /// `resolve_type` + `display_renumbered` + `add_error(TypeError::Mismatch {...})`.
    pub fn report_mismatch(&mut self, t1: &Type, t2: &Type, range: Option<mq_lang::Range>) {
        let resolved_t1 = self.resolve_type(t1);
        let resolved_t2 = self.resolve_type(t2);
        self.add_error(TypeError::Mismatch {
            expected: resolved_t1.display_renumbered(),
            found: resolved_t2.display_renumbered(),
            span: range.as_ref().map(unify::range_to_span),
            location: range.as_ref().map(|r| (r.start.line, r.start.column)),
        });
    }

    /// Reports a "no matching overload" error with formatted argument types.
    pub fn report_no_matching_overload(&mut self, op_name: &str, arg_tys: &[Type], range: Option<mq_lang::Range>) {
        let args_str = format_type_list(arg_tys);
        self.add_error(TypeError::UnificationError {
            left: format!("{} with arguments ({})", op_name, args_str),
            right: "no matching overload".to_string(),
            span: range.as_ref().map(unify::range_to_span),
            location: range.as_ref().map(|r| (r.start.line, r.start.column)),
        });
    }

    /// Resolves the best matching overload for a function call.
    ///
    /// Given a function name and argument types, finds the best matching overload
    /// based on type compatibility and match scores.
    ///
    /// Returns the matched function type and the resolved argument types after instantiation.
    pub fn resolve_overload(&mut self, name: &str, arg_types: &[Type]) -> Option<Type> {
        let overloads = self.get_builtin_overloads(name)?;

        let mut best_match: Option<(Type, u32)> = None;

        for overload in overloads {
            // For function types, check if argument types match
            if let Type::Function(params, _ret) = overload {
                // Check arity first
                if params.len() != arg_types.len() {
                    continue;
                }

                // Compute match score for each parameter
                let mut total_score = 0u32;
                let mut all_match = true;

                for (param_ty, arg_ty) in params.iter().zip(arg_types.iter()) {
                    if let Some(score) = param_ty.match_score(arg_ty) {
                        total_score += score;
                    } else {
                        all_match = false;
                        break;
                    }
                }

                if all_match {
                    match &best_match {
                        None => {
                            best_match = Some((overload.clone(), total_score));
                        }
                        Some((_, best_score)) if total_score > *best_score => {
                            best_match = Some((overload.clone(), total_score));
                        }
                        _ => {}
                    }
                }
            }
        }

        best_match.map(|(ty, _score)| self.instantiate_fresh(&ty))
    }

    /// Instantiates fresh type variables in a type to avoid contamination
    /// when the same overload or user-defined function is used at multiple call sites.
    pub fn instantiate_fresh(&mut self, ty: &Type) -> Type {
        let free_vars = ty.free_vars();
        if free_vars.is_empty() {
            return ty.clone();
        }
        // Deduplicate: same var appearing multiple times should map to the same fresh var
        let mut seen = rustc_hash::FxHashSet::default();
        let mut subst = Substitution::empty();
        for var in free_vars {
            if seen.insert(var) {
                let fresh = self.fresh_var();
                subst.insert(var, Type::Var(fresh));
            }
        }
        ty.apply_subst(&subst)
    }

    /// Generates a fresh type variable
    pub fn fresh_var(&mut self) -> TypeVarId {
        self.var_ctx.fresh()
    }

    /// Sets the type of a symbol
    pub fn set_symbol_type(&mut self, symbol: SymbolId, ty: Type) {
        // If the symbol already has a type variable, bind it to the new type
        // so that existing constraints referencing the old variable still resolve correctly
        if let Some(Type::Var(old_var)) = self.symbol_types.get(&symbol) {
            let old_var = *old_var;
            if !matches!(ty, Type::Var(v) if v == old_var) {
                self.bind_type_var(old_var, ty.clone());
            }
        }
        self.symbol_types.insert(symbol, ty);
    }

    /// Sets the type of a symbol without binding the old type variable.
    ///
    /// Use this during deferred resolution phases where binding the old Var
    /// would cascade through unification chains and corrupt types of unrelated
    /// symbols that share the same type variable.
    pub fn set_symbol_type_no_bind(&mut self, symbol: SymbolId, ty: Type) {
        self.symbol_types.insert(symbol, ty);
    }

    /// Gets the type of a symbol
    pub fn get_symbol_type(&self, symbol: SymbolId) -> Option<&Type> {
        self.symbol_types.get(&symbol)
    }

    /// Gets the type of a symbol or creates a fresh type variable
    pub fn get_or_create_symbol_type(&mut self, symbol: SymbolId) -> Type {
        if let Some(ty) = self.symbol_types.get(&symbol) {
            ty.clone()
        } else {
            let ty_var = self.fresh_var();
            let ty = Type::Var(ty_var);
            self.symbol_types.insert(symbol, ty.clone());
            ty
        }
    }

    /// Adds a constraint to be solved
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Takes all constraints (consumes them)
    pub fn take_constraints(&mut self) -> Vec<Constraint> {
        std::mem::take(&mut self.constraints)
    }

    /// Binds a type variable to a type
    pub fn bind_type_var(&mut self, var: TypeVarId, ty: Type) {
        self.substitutions.insert(var, ty);
    }

    /// Gets the bound type for a type variable
    pub fn get_type_var(&self, var: TypeVarId) -> Option<Type> {
        self.substitutions.get(&var).cloned()
    }

    /// Resolves a type by following type variable bindings.
    ///
    /// Uses a visited set to detect and break cycles in the substitution map.
    /// Cycles can form with mutually recursive functions (e.g. `Var(a) →
    /// Union(Var(b), String)` and `Var(b) → Union(Var(a), String)`), which
    /// would otherwise cause infinite recursion.
    pub fn resolve_type(&self, ty: &Type) -> Type {
        let mut visited = rustc_hash::FxHashSet::default();
        self.resolve_type_inner(ty, &mut visited)
    }

    fn resolve_type_inner(&self, ty: &Type, visited: &mut rustc_hash::FxHashSet<TypeVarId>) -> Type {
        match ty {
            Type::Var(var) => {
                if !visited.insert(*var) {
                    // Cycle detected — return the var unresolved to break infinite recursion
                    return ty.clone();
                }
                if let Some(bound) = self.substitutions.get(var) {
                    let result = self.resolve_type_inner(bound, visited);
                    visited.remove(var); // Allow the same var in sibling branches
                    result
                } else {
                    visited.remove(var);
                    ty.clone()
                }
            }
            Type::Array(elem) => Type::Array(Box::new(self.resolve_type_inner(elem, visited))),
            Type::Dict(key, value) => Type::Dict(
                Box::new(self.resolve_type_inner(key, visited)),
                Box::new(self.resolve_type_inner(value, visited)),
            ),
            Type::Function(params, ret) => {
                let new_params = params.iter().map(|p| self.resolve_type_inner(p, visited)).collect();
                Type::Function(new_params, Box::new(self.resolve_type_inner(ret, visited)))
            }
            Type::Record(fields, rest) => {
                let new_fields = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.resolve_type_inner(v, visited)))
                    .collect();
                Type::Record(new_fields, Box::new(self.resolve_type_inner(rest, visited)))
            }
            Type::Union(members) => Type::union(members.iter().map(|m| self.resolve_type_inner(m, visited)).collect()),
            Type::Tuple(elems) => Type::Tuple(elems.iter().map(|e| self.resolve_type_inner(e, visited)).collect()),
            _ => ty.clone(),
        }
    }

    /// Finalizes inference and returns symbol type schemes
    pub fn finalize(self) -> TypeEnv {
        let mut result = TypeEnv::default();

        for (symbol, ty) in &self.symbol_types {
            let resolved = self.resolve_type(ty);
            // Generalize function types: free type variables become quantified
            let scheme = if matches!(resolved, Type::Function(_, _)) {
                TypeScheme::generalize(resolved, &[])
            } else {
                TypeScheme::mono(resolved)
            };
            result.insert(*symbol, scheme);
        }

        result
    }

    /// Gets all symbol types (for testing)
    #[cfg(test)]
    pub fn symbol_types(&self) -> &FxHashMap<SymbolId, Type> {
        &self.symbol_types
    }
}

impl Default for InferenceContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_vars() {
        let mut ctx = InferenceContext::new();
        let var1 = ctx.fresh_var();
        let var2 = ctx.fresh_var();
        assert_ne!(var1, var2);
    }

    #[test]
    fn test_type_var_binding() {
        let mut ctx = InferenceContext::new();
        let var = ctx.fresh_var();

        ctx.bind_type_var(var, Type::Number);
        assert_eq!(ctx.get_type_var(var), Some(Type::Number));
    }

    #[test]
    fn test_resolve_type() {
        let mut ctx = InferenceContext::new();
        let var1 = ctx.fresh_var();
        let var2 = ctx.fresh_var();

        // var1 -> var2 -> Number
        ctx.bind_type_var(var2, Type::Number);
        ctx.bind_type_var(var1, Type::Var(var2));

        let resolved = ctx.resolve_type(&Type::Var(var1));
        assert_eq!(resolved, Type::Number);
    }

    #[test]
    fn test_overload_resolution_exact_match() {
        let mut ctx = InferenceContext::new();

        // Register two overloads for "add"
        ctx.register_builtin("add", Type::function(vec![Type::Number, Type::Number], Type::Number));
        ctx.register_builtin("add", Type::function(vec![Type::String, Type::String], Type::String));

        // Test with number arguments - should resolve to number overload
        let arg_types = vec![Type::Number, Type::Number];
        let resolved = ctx.resolve_overload("add", &arg_types);
        assert!(resolved.is_some());

        if let Some(Type::Function(_params, ret)) = resolved {
            assert_eq!(*ret, Type::Number);
        } else {
            panic!("Expected function type");
        }

        // Test with string arguments - should resolve to string overload
        let arg_types = vec![Type::String, Type::String];
        let resolved = ctx.resolve_overload("add", &arg_types);
        assert!(resolved.is_some());

        if let Some(Type::Function(_params, ret)) = resolved {
            assert_eq!(*ret, Type::String);
        } else {
            panic!("Expected function type");
        }
    }

    #[test]
    fn test_overload_resolution_type_variables() {
        let mut ctx = InferenceContext::new();

        // Register overloads
        ctx.register_builtin("op", Type::function(vec![Type::Number], Type::Bool));
        ctx.register_builtin("op", Type::function(vec![Type::String], Type::Symbol));

        // Test with type variable - should match but prefer concrete types
        let var = ctx.fresh_var();
        let arg_types = vec![Type::Var(var)];
        let resolved = ctx.resolve_overload("op", &arg_types);
        assert!(resolved.is_some());
    }

    #[test]
    fn test_overload_resolution_no_match() {
        let mut ctx = InferenceContext::new();

        // Register only number overload
        ctx.register_builtin("op", Type::function(vec![Type::Number], Type::Number));

        // Test with bool argument - should not match
        let arg_types = vec![Type::Bool];
        let resolved = ctx.resolve_overload("op", &arg_types);
        assert!(resolved.is_none());
    }

    #[test]
    fn test_overload_resolution_arity_mismatch() {
        let mut ctx = InferenceContext::new();

        // Register binary operator
        ctx.register_builtin("op", Type::function(vec![Type::Number, Type::Number], Type::Number));

        // Test with wrong arity - should not match
        let arg_types = vec![Type::Number];
        let resolved = ctx.resolve_overload("op", &arg_types);
        assert!(resolved.is_none());
    }
}
