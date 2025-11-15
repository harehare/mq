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
pub enum TypeError {
    #[error("Type mismatch: expected {expected}, found {found}")]
    #[diagnostic(code(typechecker::type_mismatch))]
    Mismatch {
        expected: String,
        found: String,
        #[label("type mismatch here")]
        span: Option<miette::SourceSpan>,
    },

    #[error("Cannot unify types: {left} and {right}")]
    #[diagnostic(code(typechecker::unification_error))]
    UnificationError {
        left: String,
        right: String,
        #[label("cannot unify these types")]
        span: Option<miette::SourceSpan>,
    },

    #[error("Occurs check failed: type variable {var} occurs in {ty}")]
    #[diagnostic(code(typechecker::occurs_check))]
    OccursCheck {
        var: String,
        ty: String,
        #[label("infinite type")]
        span: Option<miette::SourceSpan>,
    },

    #[error("Undefined symbol: {name}")]
    #[diagnostic(code(typechecker::undefined_symbol))]
    UndefinedSymbol {
        name: String,
        #[label("undefined symbol")]
        span: Option<miette::SourceSpan>,
    },

    #[error("Wrong number of arguments: expected {expected}, found {found}")]
    #[diagnostic(code(typechecker::wrong_arity))]
    WrongArity {
        expected: usize,
        found: usize,
        #[label("wrong number of arguments")]
        span: Option<miette::SourceSpan>,
    },

    #[error("Type variable not found: {0}")]
    #[diagnostic(code(typechecker::type_var_not_found))]
    TypeVarNotFound(String),

    #[error("Internal error: {0}")]
    #[diagnostic(code(typechecker::internal_error))]
    Internal(String),
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
    /// # Errors
    ///
    /// Returns a `TypeError` if type checking fails.
    pub fn check(&mut self, hir: &Hir) -> Result<()> {
        // Create inference context
        let mut ctx = infer::InferenceContext::new();

        // Generate builtin type signatures
        self.add_builtin_types(&mut ctx);

        // Generate constraints from HIR
        constraint::generate_constraints(hir, &mut ctx)?;

        // Solve constraints through unification
        unify::solve_constraints(&mut ctx)?;

        // Store inferred types
        self.symbol_types = ctx.finalize();

        Ok(())
    }

    /// Gets the type of a symbol
    pub fn type_of(&self, symbol: SymbolId) -> Option<&TypeScheme> {
        self.symbol_types.get(&symbol)
    }

    /// Gets all symbol types
    pub fn symbol_types(&self) -> &FxHashMap<SymbolId, TypeScheme> {
        &self.symbol_types
    }

    /// Adds builtin function type signatures
    fn add_builtin_types(&self, ctx: &mut infer::InferenceContext) {
        use types::Type;

        // Addition operator: supports both numbers and strings
        // Overload 1: (number, number) -> number
        ctx.register_builtin("+", Type::function(vec![Type::Number, Type::Number], Type::Number));
        // Overload 2: (string, string) -> string
        ctx.register_builtin("+", Type::function(vec![Type::String, Type::String], Type::String));

        // Other arithmetic operators: (number, number) -> number
        for op in ["-", "*", "/", "%", "^"] {
            let params = vec![Type::Number, Type::Number];
            let ret = Type::Number;
            ctx.register_builtin(op, Type::function(params, ret));
        }

        // Comparison operators: (number, number) -> bool
        for op in ["<", ">", "<=", ">="] {
            let params = vec![Type::Number, Type::Number];
            let ret = Type::Bool;
            ctx.register_builtin(op, Type::function(params, ret));
        }

        // Equality operators: forall a. (a, a) -> bool
        // For now, we'll use type variables
        for op in ["==", "!="] {
            let a = ctx.fresh_var();
            let params = vec![Type::Var(a), Type::Var(a)];
            let ret = Type::Bool;
            ctx.register_builtin(op, Type::function(params, ret));
        }

        // Logical operators: (bool, bool) -> bool
        for op in ["and", "or"] {
            let params = vec![Type::Bool, Type::Bool];
            let ret = Type::Bool;
            ctx.register_builtin(op, Type::function(params, ret));
        }

        // Unary operators
        // not: bool -> bool
        ctx.register_builtin("!", Type::function(vec![Type::Bool], Type::Bool));
        ctx.register_builtin("not", Type::function(vec![Type::Bool], Type::Bool));

        // Unary minus: number -> number
        ctx.register_builtin("unary-", Type::function(vec![Type::Number], Type::Number));
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
