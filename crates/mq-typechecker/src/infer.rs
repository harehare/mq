//! Type inference context and engine.

use crate::TypeError;
use crate::constraint::Constraint;
use crate::types::{Substitution, Type, TypeScheme, TypeVarContext, TypeVarId};
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
    /// Deferred overload resolutions for operators with unresolved type variable operands
    deferred_overloads: Vec<DeferredOverload>,
    /// Deferred user-defined function calls for post-unification type checking
    deferred_user_calls: Vec<DeferredUserCall>,
    /// Deferred inner calls to function parameters for higher-order function checking
    deferred_parameter_calls: Vec<DeferredParameterCall>,
    /// Deferred record field accesses for post-unification resolution
    deferred_record_accesses: Vec<DeferredRecordAccess>,
}

impl InferenceContext {
    /// Creates a new inference context
    pub fn new() -> Self {
        Self {
            var_ctx: TypeVarContext::new(),
            symbol_types: FxHashMap::default(),
            constraints: Vec::new(),
            substitutions: FxHashMap::default(),
            builtins: FxHashMap::default(),
            errors: Vec::new(),
            piped_inputs: FxHashMap::default(),
            deferred_overloads: Vec::new(),
            deferred_user_calls: Vec::new(),
            deferred_parameter_calls: Vec::new(),
            deferred_record_accesses: Vec::new(),
        }
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

    /// Adds a deferred overload resolution
    pub fn add_deferred_overload(&mut self, deferred: DeferredOverload) {
        self.deferred_overloads.push(deferred);
    }

    /// Takes all deferred overloads (consumes them)
    pub fn take_deferred_overloads(&mut self) -> Vec<DeferredOverload> {
        std::mem::take(&mut self.deferred_overloads)
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

    /// Reports a "no matching overload" error with formatted argument types.
    pub fn report_no_matching_overload(&mut self, op_name: &str, arg_tys: &[Type], range: Option<mq_lang::Range>) {
        let args_str = arg_tys
            .iter()
            .map(|t| t.display_renumbered())
            .collect::<Vec<_>>()
            .join(", ");
        self.add_error(crate::TypeError::UnificationError {
            left: format!("{} with arguments ({})", op_name, args_str),
            right: "no matching overload".to_string(),
            span: range.as_ref().map(crate::unify::range_to_span),
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
                    // Update best match if this is better
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
    /// Type variable chains (var1 → var2 → ... → concrete) are followed iteratively
    /// to avoid stack overflow on long chains.
    pub fn resolve_type(&self, ty: &Type) -> Type {
        // Follow type variable chains iteratively
        let ty = self.resolve_var_chain(ty);
        match &ty {
            Type::Var(_) => ty,
            Type::Array(elem) => Type::Array(Box::new(self.resolve_type(elem))),
            Type::Dict(key, value) => Type::Dict(Box::new(self.resolve_type(key)), Box::new(self.resolve_type(value))),
            Type::Function(params, ret) => {
                let new_params = params.iter().map(|p| self.resolve_type(p)).collect();
                Type::Function(new_params, Box::new(self.resolve_type(ret)))
            }
            Type::Record(fields, rest) => {
                let new_fields = fields.iter().map(|(k, v)| (k.clone(), self.resolve_type(v))).collect();
                Type::Record(new_fields, Box::new(self.resolve_type(rest)))
            }
            _ => ty,
        }
    }

    /// Follows a type variable substitution chain iteratively until reaching
    /// a non-variable type or an unbound variable.
    fn resolve_var_chain(&self, ty: &Type) -> Type {
        let mut current = ty.clone();
        loop {
            match &current {
                Type::Var(var) => {
                    if let Some(bound) = self.substitutions.get(var) {
                        current = bound.clone();
                    } else {
                        return current;
                    }
                }
                _ => return current,
            }
        }
    }

    /// Finalizes inference and returns symbol type schemes
    pub fn finalize(self) -> FxHashMap<SymbolId, TypeScheme> {
        let mut result = FxHashMap::default();

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
