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

        // Generate builtin type signatures
        builtin::register_all(&mut ctx);

        // Generate constraints from HIR (collects errors internally)
        constraint::generate_constraints(hir, &mut ctx);

        // Solve constraints through unification (collects errors internally)
        unify::solve_constraints(&mut ctx);

        // Process deferred overload resolutions (operators with type variable operands)
        // After the first round of unification, operand types may now be resolved
        Self::resolve_deferred_overloads(&mut ctx);

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
    fn resolve_deferred_overloads(ctx: &mut infer::InferenceContext) {
        let deferred = ctx.take_deferred_overloads();
        if deferred.is_empty() {
            return;
        }

        for d in &deferred {
            let resolved_operands: Vec<types::Type> = d.operand_tys.iter().map(|ty| ctx.resolve_type(ty)).collect();

            eprintln!(
                "[DEBUG deferred] op={}, operand_tys={:?}, resolved={:?}",
                d.op_name, d.operand_tys, resolved_operands
            );

            if let Some(resolved_ty) = ctx.resolve_overload(&d.op_name, &resolved_operands) {
                if let types::Type::Function(param_tys, ret_ty) = resolved_ty
                    && param_tys.len() == d.operand_tys.len()
                {
                    eprintln!(
                        "[DEBUG deferred]   matched: param_tys={:?}, ret={:?}",
                        param_tys, ret_ty
                    );
                    // Add constraints for operand types
                    for (operand_ty, param_ty) in d.operand_tys.iter().zip(param_tys.iter()) {
                        ctx.add_constraint(constraint::Constraint::Equal(
                            operand_ty.clone(),
                            param_ty.clone(),
                            d.range,
                        ));
                    }
                    // Set the result type
                    ctx.set_symbol_type(d.symbol_id, *ret_ty);
                }
            } else {
                // Check if all operands are now concrete (not type variables)
                let all_concrete = resolved_operands.iter().all(|ty| !ty.is_var());
                if all_concrete {
                    let args_str = resolved_operands
                        .iter()
                        .map(|t| t.to_string())
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

        // Run unification again with the new constraints
        unify::solve_constraints(ctx);
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
