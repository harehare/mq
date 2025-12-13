//! Type inference context and engine.

use crate::TypeError;
use crate::constraint::Constraint;
use crate::types::{Type, TypeScheme, TypeVarContext, TypeVarId};
use mq_hir::SymbolId;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

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
    /// Collected type errors (for multi-error reporting)
    errors: Vec<TypeError>,
    /// Piped input types for symbols in a pipe chain
    piped_inputs: FxHashMap<SymbolId, Type>,
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

        best_match.map(|(ty, _score)| ty)
    }

    /// Generates a fresh type variable
    pub fn fresh_var(&mut self) -> TypeVarId {
        self.var_ctx.fresh()
    }

    /// Sets the type of a symbol
    pub fn set_symbol_type(&mut self, symbol: SymbolId, ty: Type) {
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

    /// Resolves a type by following type variable bindings
    pub fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(var) => {
                if let Some(bound) = self.substitutions.get(var) {
                    self.resolve_type(bound)
                } else {
                    ty.clone()
                }
            }
            Type::Array(elem) => Type::Array(Box::new(self.resolve_type(elem))),
            Type::Dict(key, value) => Type::Dict(Box::new(self.resolve_type(key)), Box::new(self.resolve_type(value))),
            Type::Function(params, ret) => {
                let new_params = params.iter().map(|p| self.resolve_type(p)).collect();
                Type::Function(new_params, Box::new(self.resolve_type(ret)))
            }
            _ => ty.clone(),
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
