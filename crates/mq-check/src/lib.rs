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
pub(crate) mod deferred;
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
        location: Option<mq_lang::Range>,
        #[help]
        context: Option<String>,
    },
    #[error("Cannot unify types: {left} and {right}")]
    #[diagnostic(code(typechecker::unification_error))]
    #[allow(dead_code)]
    UnificationError {
        left: String,
        right: String,
        #[label("cannot unify these types")]
        span: Option<miette::SourceSpan>,
        location: Option<mq_lang::Range>,
        #[help]
        context: Option<String>,
    },
    #[error("Occurs check failed: type variable {var} occurs in {ty}")]
    #[diagnostic(code(typechecker::occurs_check))]
    #[allow(dead_code)]
    OccursCheck {
        var: String,
        ty: String,
        #[label("infinite type")]
        span: Option<miette::SourceSpan>,
        location: Option<mq_lang::Range>,
    },
    #[error("Undefined symbol: {name}")]
    #[diagnostic(code(typechecker::undefined_symbol))]
    #[allow(dead_code)]
    UndefinedSymbol {
        name: String,
        #[label("undefined symbol")]
        span: Option<miette::SourceSpan>,
        location: Option<mq_lang::Range>,
    },
    #[error("Wrong number of arguments: expected {expected}, found {found}")]
    #[diagnostic(code(typechecker::wrong_arity))]
    #[allow(dead_code)]
    WrongArity {
        expected: usize,
        found: usize,
        #[label("wrong number of arguments")]
        span: Option<miette::SourceSpan>,
        location: Option<mq_lang::Range>,
        #[help]
        context: Option<String>,
    },
    #[error("Undefined field `{field}` in record type {record_ty}")]
    #[diagnostic(code(typechecker::undefined_field))]
    #[allow(dead_code)]
    UndefinedField {
        field: String,
        record_ty: String,
        #[label("field not found")]
        span: Option<miette::SourceSpan>,
        location: Option<mq_lang::Range>,
    },
    #[error("Heterogeneous array: elements have mixed types [{types}]")]
    #[diagnostic(code(typechecker::heterogeneous_array))]
    #[allow(dead_code)]
    HeterogeneousArray {
        types: String,
        #[label("mixed types in array")]
        span: Option<miette::SourceSpan>,
        location: Option<mq_lang::Range>,
    },
    #[error("Type variable not found: {0}")]
    #[diagnostic(code(typechecker::type_var_not_found))]
    TypeVarNotFound(String),
    #[error("Internal error: {0}")]
    #[diagnostic(code(typechecker::internal_error))]
    Internal(String),
}

impl TypeError {
    /// Returns the location (range) of the error, if available.
    pub fn location(&self) -> Option<mq_lang::Range> {
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
        let mut ctx = infer::InferenceContext::with_options(self.options.strict_array);

        builtin::register_all(&mut ctx);

        // Generate constraints from HIR (collects errors internally)
        constraint::generate_constraints(hir, &mut ctx);

        // Solve constraints through unification (collects errors internally)
        unify::solve_constraints(&mut ctx);

        // Apply type narrowings from type predicate conditions (e.g., is_string(x))
        // in if/elif branches. This overrides Ref types within narrowed branches.
        narrowing::resolve_type_narrowings(hir, &mut ctx);

        // Resolve deferred tuple index accesses now that variable types are known.
        if deferred::resolve_deferred_tuple_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        // Resolve deferred record field accesses now that variable types are known.
        // This binds bracket access return types (e.g., v[:key]) to specific field types
        // from Record types, enabling type error detection for subsequent operations.
        if deferred::resolve_record_field_accesses(&mut ctx) {
            // Re-run unification to propagate newly resolved record field types
            unify::solve_constraints(&mut ctx);
        }

        // Resolve deferred selector field accesses (.field on records)
        deferred::resolve_selector_field_accesses(&mut ctx);

        // Propagate return types from user-defined function calls.
        // After unification, the original function's return type may be concrete,
        // allowing us to connect it to the fresh return type at each call site.
        // This must run BEFORE deferred overload resolution so that operators using
        // return types have concrete operands.
        deferred::propagate_user_call_returns(&mut ctx);

        // Process deferred overload resolutions (operators with type variable operands)
        // After return type propagation + unification, operand types may now be resolved.
        // Unresolved overloads are stored back for later processing.
        deferred::resolve_deferred_overloads(&mut ctx);

        // Re-run deferred tuple accesses after overload resolution, because some variable
        // types (e.g., the return type of `first(xs)`) may only be resolved after
        // `resolve_deferred_overloads` runs. This ensures that index accesses on variables
        // with union types containing unresolved vars (e.g., `Union(None, Var)`) are
        // retried with the now-concrete member types.
        if deferred::resolve_deferred_tuple_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        // Check operators inside user-defined function bodies against call-site types.
        // Uses local substitution (original params → call-site args) without modifying
        // global state, so multiple call sites don't interfere.
        deferred::check_user_call_body_operators(hir, &mut ctx);

        // Collect errors before finalizing
        let errors = ctx.take_errors();

        // Store inferred types
        self.symbol_types = ctx.finalize();

        errors
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
    use mq_hir::SymbolKind;
    use rstest::rstest;

    #[test]
    fn test_typechecker_creation() {
        let checker = TypeChecker::new();
        assert_eq!(checker.symbol_types.len(), 0);
    }

    #[test]
    fn test_type_env() {
        let mut env = TypeEnv::default();
        assert!(env.is_empty());
        assert_eq!(env.len(), 0);

        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, "42");
        let (symbol_id, _) = hir.symbols_for_source(source_id).next().unwrap();

        let scheme = TypeScheme::mono(types::Type::Number);
        env.insert(symbol_id, scheme.clone());

        assert!(!env.is_empty());
        assert_eq!(env.len(), 1);
        assert_eq!(env.get(&symbol_id), Some(&scheme));
        assert!(env.get_all().contains_key(&symbol_id));

        for (&id, s) in &env {
            assert_eq!(id, symbol_id);
            assert_eq!(s, &scheme);
        }
    }

    #[rstest]
    #[case(TypeError::Mismatch { expected: "n".into(), found: "s".into(), span: None, location: Some(mq_lang::Range { start: mq_lang::Position { line: 1, column: 5 }, end: mq_lang::Position { line: 1, column: 6 } }), context: None }, Some(mq_lang::Range { start: mq_lang::Position { line: 1, column: 5 }, end: mq_lang::Position { line: 1, column: 6 } }))]
    #[case(TypeError::UnificationError { left: "l".into(), right: "r".into(), span: None, location: Some(mq_lang::Range { start: mq_lang::Position { line: 2, column: 10 }, end: mq_lang::Position { line: 2, column: 11 } }), context: None }, Some(mq_lang::Range { start: mq_lang::Position { line: 2, column: 10 }, end: mq_lang::Position { line: 2, column: 11 } }))]
    #[case(TypeError::TypeVarNotFound("a".into()), None)]
    fn test_type_error_location(#[case] err: TypeError, #[case] expected: Option<mq_lang::Range>) {
        assert_eq!(err.location(), expected);
    }

    #[test]
    fn test_walk_ancestors() {
        let mut hir = Hir::default();
        // Use a nested structure: function -> body (block) -> expression
        let code = "def f(x): x + 1;";
        hir.add_code(None, code);

        // Find the '1' literal
        let (num_id, _) = hir
            .symbols()
            .find(|(_, s)| matches!(s.kind, SymbolKind::Number))
            .unwrap();

        let ancestors: Vec<_> = walk_ancestors(&hir, num_id).collect();
        assert!(!ancestors.is_empty());

        // The number '1' should be a child of the binary op '+', which is a child of the function
        let has_function = ancestors.iter().any(|(_, s)| matches!(s.kind, SymbolKind::Function(_)));
        assert!(has_function);
    }
}
