//! Type inference engine for mq language using Hindley-Milner type inference.
//!
//! This crate provides static type checking and type inference capabilities for mq.
//! It implements a Hindley-Milner style type inference algorithm with support for:
//! - Automatic type inference (no type annotations required)
//! - Polymorphic functions (generics)
//! - Type constraints and unification
//! - Integration with mq-hir for symbol and scope information
//! - Error location reporting with source ranges
//!
//! ## Error Location Reporting
//!
//! Type errors carry the [`mq_lang::Range`] of the offending symbol, taken from the HIR.
//! Use [`TypeError::location`] to read it, or [`TypeError::span`] for a byte span in the
//! source to use as a miette label.

// Suppress false-positive warnings for fields used in thiserror/miette macros
#![allow(unused_assignments)]

pub(crate) mod builtin;
pub mod certainty;
pub(crate) mod constraint;
pub(crate) mod deferred;
pub(crate) mod exhaustiveness;
pub(crate) mod field_guard;
pub(crate) mod infer;
pub mod kind_set;
pub(crate) mod narrowing;
pub(crate) mod node_attr;
pub mod type_expr;
pub mod types;
pub(crate) mod unify;

use miette::Diagnostic;
use mq_hir::{Hir, SymbolId};
use rustc_hash::FxHashMap;
use thiserror::Error;
use types::TypeScheme;

pub use certainty::{Certainty, Classified, Severity};

/// Names `mq-test` defines at runtime, which no source declares. Pass them to
/// [`Hir::declare_global`] when checking test files.
pub const TEST_RUNNER_GLOBALS: &[&str] = &["TEST_FILE", "assert_snapshot"];

/// Result type for type checking operations
pub type Result<T> = std::result::Result<T, TypeError>;

/// Type environment mapping symbol IDs to their inferred type schemes
#[derive(Debug, Clone, Default)]
pub struct TypeEnv(FxHashMap<SymbolId, TypeScheme>);

impl TypeEnv {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self(FxHashMap::with_capacity_and_hasher(capacity, Default::default()))
    }

    /// Associates `scheme` with `symbol_id`, replacing any previous entry.
    pub fn insert(&mut self, symbol_id: SymbolId, scheme: TypeScheme) {
        self.0.insert(symbol_id, scheme);
    }

    /// The scheme for `symbol_id`, if any.
    pub fn get(&self, symbol_id: &SymbolId) -> Option<&TypeScheme> {
        self.0.get(symbol_id)
    }

    /// All symbol-to-scheme entries.
    pub fn get_all(&self) -> &FxHashMap<SymbolId, TypeScheme> {
        &self.0
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the environment has no entries.
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
    /// Expected and found types differ.
    Mismatch {
        /// The expected type.
        expected: String,
        /// The type that was found.
        found: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
        #[help]
        /// Extra hint shown as help.
        context: Option<String>,
    },
    #[error("Cannot unify types: {left} and {right}")]
    #[diagnostic(code(typechecker::unification_error))]
    #[allow(dead_code)]
    /// Two types cannot be unified.
    UnificationError {
        /// Left-hand type.
        left: String,
        /// Right-hand type.
        right: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
        #[help]
        /// Extra hint shown as help.
        context: Option<String>,
    },
    #[error("Occurs check failed: type variable {var} occurs in {ty}")]
    #[diagnostic(code(typechecker::occurs_check))]
    #[allow(dead_code)]
    /// A type variable occurs inside the type it is bound to.
    OccursCheck {
        /// The type variable.
        var: String,
        /// The type containing the variable.
        ty: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
    },
    #[error("Undefined symbol: {name}")]
    #[diagnostic(code(typechecker::undefined_symbol))]
    #[allow(dead_code)]
    /// A symbol is used but never defined.
    UndefinedSymbol {
        /// Name of the undefined symbol.
        name: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
    },
    #[error("Wrong number of arguments: expected {expected}, found {found}")]
    #[diagnostic(code(typechecker::wrong_arity))]
    #[allow(dead_code)]
    /// A call passes the wrong number of arguments.
    WrongArity {
        /// The expected number of arguments.
        expected: usize,
        /// The number of arguments passed.
        found: usize,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
        #[help]
        /// Extra hint shown as help.
        context: Option<String>,
    },
    #[error("Undefined field `{field}` in record type {record_ty}")]
    #[diagnostic(code(typechecker::undefined_field))]
    #[allow(dead_code)]
    /// A record type has no such field.
    UndefinedField {
        /// Name of the missing field.
        field: String,
        /// The record type that lacks the field.
        record_ty: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
    },
    #[error("Attribute `{attr}` does not exist on {node_ty}")]
    #[diagnostic(code(typechecker::undefined_attribute))]
    #[allow(dead_code)]
    /// A node type has no such attribute.
    UndefinedAttribute {
        /// Name of the missing attribute.
        attr: String,
        /// The node type that lacks the attribute.
        node_ty: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
    },
    #[error("Heterogeneous array: elements have mixed types [{types}]")]
    #[diagnostic(code(typechecker::heterogeneous_array))]
    #[allow(dead_code)]
    /// Array elements have mixed types.
    HeterogeneousArray {
        /// Comma-separated element types.
        types: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
    },
    #[error("Type variable not found: {0}")]
    #[diagnostic(code(typechecker::type_var_not_found))]
    /// A type variable id has no binding.
    TypeVarNotFound(String),
    #[error("Internal error: {0}")]
    #[diagnostic(code(typechecker::internal_error))]
    /// An unexpected internal failure.
    Internal(String),
    /// Emitted when an operation receives a nullable argument and may silently
    /// propagate `none` instead of producing the expected concrete type.
    #[error("Operation `{op}` may propagate `none`: argument `{nullable_arg}` may be `none`")]
    #[diagnostic(code(typechecker::nullable_propagation))]
    #[allow(dead_code)]
    NullablePropagation {
        /// The operation being checked.
        op: String,
        /// The argument that may be `none`.
        nullable_arg: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
        #[help]
        /// Extra hint shown as help.
        context: Option<String>,
    },
    /// Emitted when a branch is statically determined to be unreachable based on
    /// the known type of a variable and the type predicate in the condition.
    #[error("Unreachable code: this branch can never be executed")]
    #[diagnostic(code(typechecker::unreachable_code))]
    #[allow(dead_code)]
    UnreachableCode {
        /// Why the branch is unreachable.
        reason: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
    },
    /// Emitted when a match expression does not cover all possible values of the matched type.
    #[error("Non-exhaustive patterns: missing case for {missing}")]
    #[diagnostic(code(typechecker::non_exhaustive_patterns))]
    #[allow(dead_code)]
    NonExhaustiveMatch {
        /// Description of the uncovered case.
        missing: String,
        /// Source range of the error, if known.
        location: Option<mq_lang::Range>,
        #[help]
        /// Extra hint shown as help.
        context: Option<String>,
    },
}

impl TypeError {
    /// Whether the diagnostic is advice about the code rather than a type error: it is always shown
    /// as a warning.
    pub fn is_advisory(&self) -> bool {
        matches!(
            self,
            TypeError::NullablePropagation { .. }
                | TypeError::UnreachableCode { .. }
                | TypeError::NonExhaustiveMatch { .. }
                | TypeError::HeterogeneousArray { .. }
        )
    }

    /// Returns the location (range) of the error, if available.
    pub fn location(&self) -> Option<mq_lang::Range> {
        match self {
            TypeError::Mismatch { location, .. }
            | TypeError::UnificationError { location, .. }
            | TypeError::OccursCheck { location, .. }
            | TypeError::UndefinedSymbol { location, .. }
            | TypeError::WrongArity { location, .. }
            | TypeError::UndefinedField { location, .. }
            | TypeError::UndefinedAttribute { location, .. }
            | TypeError::HeterogeneousArray { location, .. }
            | TypeError::NullablePropagation { location, .. }
            | TypeError::UnreachableCode { location, .. }
            | TypeError::NonExhaustiveMatch { location, .. } => *location,
            _ => None,
        }
    }

    /// Returns the byte span of the error in `source`, for use as a miette label.
    ///
    /// `source` must be the text the checked HIR was built from.
    pub fn span(&self, source: &str) -> Option<miette::SourceSpan> {
        let range = self.location()?;
        let offset = |position: &mq_lang::Position| {
            miette::SourceOffset::from_location(source, position.line as usize, position.column).offset()
        };
        let start = offset(&range.start);
        let end = offset(&range.end).max(start);

        Some(miette::SourceSpan::new(start.into(), end - start))
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

/// Line offset added to the ranges of symbols that come from included modules, so their
/// diagnostics can be told apart from the user's and dropped.
const MODULE_LINE_OFFSET: u32 = 1 << 24;

/// Range of `symbol`, shifted past `MODULE_LINE_OFFSET` when it belongs to a module source.
pub(crate) fn symbol_range(hir: &Hir, symbol: &mq_hir::Symbol) -> Option<mq_lang::Range> {
    let mut range = symbol.source.text_range?;
    if symbol.source.source_id.is_some_and(|sid| hir.is_module_source(sid)) {
        range.start.line += MODULE_LINE_OFFSET;
        range.end.line += MODULE_LINE_OFFSET;
    }
    Some(range)
}

pub(crate) fn is_module_range(range: &mq_lang::Range) -> bool {
    range.start.line >= MODULE_LINE_OFFSET
}

/// Options for configuring the type checker behavior
#[derive(Debug, Clone, Default)]
pub struct TypeCheckerOptions {
    /// When true, arrays must contain elements of a single type.
    /// Heterogeneous arrays like `[1, "hello"]` will produce a type error.
    pub strict_array: bool,
    /// When true, exhaustiveness checking for pattern match expressions is disabled.
    pub no_exhaustive_patterns: bool,
    /// When true, errors the checker is not sure about ([`Certainty::Uncertain`]) are errors.
    /// Consumers that show diagnostics read it through [`Classified::severity`].
    pub strict: bool,
    /// When true, errors the checker is not sure about are shown as warnings. They are left out
    /// otherwise.
    pub show_uncertain: bool,
    /// The type of the input document `.` (see [`type_expr::parse_type`]). When `None` the input
    /// is `dynamic`.
    pub input_type: Option<types::Type>,
}

/// Wall-clock time of each phase of [`TypeChecker::check_profiled`], in execution order.
///
/// A phase that runs more than once appears once per run.
#[derive(Debug, Clone, Default)]
pub struct PhaseTimings(pub Vec<(&'static str, std::time::Duration)>);

impl PhaseTimings {
    /// Total time per phase name, in first-seen order.
    pub fn summed(&self) -> Vec<(&'static str, std::time::Duration)> {
        let mut out: Vec<(&'static str, std::time::Duration)> = Vec::new();
        for (name, elapsed) in &self.0 {
            match out.iter_mut().find(|(n, _)| n == name) {
                Some((_, total)) => *total += *elapsed,
                None => out.push((name, *elapsed)),
            }
        }
        out
    }
}

/// Records the time between consecutive `lap` calls. Does nothing unless started.
#[derive(Default)]
struct Laps {
    last: Option<std::time::Instant>,
    timings: Vec<(&'static str, std::time::Duration)>,
}

impl Laps {
    fn started() -> Self {
        Self {
            last: Some(std::time::Instant::now()),
            timings: Vec::new(),
        }
    }

    fn lap(&mut self, name: &'static str) {
        if let Some(last) = &mut self.last {
            let now = std::time::Instant::now();
            self.timings.push((name, now - *last));
            *last = now;
        }
    }
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
        self.check_classified(hir).into_iter().map(|c| c.error).collect()
    }

    /// Like [`check`](Self::check), but says for each error how sure the checker is about it.
    pub fn check_classified(&mut self, hir: &Hir) -> Vec<Classified> {
        self.run(hir, &mut Laps::default())
    }

    /// Like [`check`](Self::check), but also returns the wall-clock time of each phase.
    pub fn check_profiled(&mut self, hir: &Hir) -> (Vec<TypeError>, PhaseTimings) {
        let mut laps = Laps::started();
        let errors = self.run(hir, &mut laps).into_iter().map(|c| c.error).collect();
        (errors, PhaseTimings(laps.timings))
    }

    fn run(&mut self, hir: &Hir, laps: &mut Laps) -> Vec<Classified> {
        // Create inference context with options
        let mut ctx = infer::InferenceContext::with_options(self.options.strict_array);
        if let Some(input_type) = &self.options.input_type {
            ctx.set_input_type(input_type.clone());
        }
        ctx.reserve(hir.symbols().count());

        builtin::register_all(&mut ctx);
        laps.lap("register_builtins");

        // Generate constraints from HIR (collects errors internally).
        // Returns the children index so it can be reused by later passes.
        let children_index = constraint::generate_constraints(hir, &mut ctx);
        laps.lap("generate_constraints");

        // Solve constraints through unification (collects errors internally)
        unify::solve_constraints(&mut ctx);
        laps.lap("solve");

        // Resolve user-defined call return types before narrowing so `y != None` can narrow `let y = f(x)`.
        deferred::resolve_generator_yields(&mut ctx);
        deferred::propagate_user_call_returns(&mut ctx);
        laps.lap("propagate_user_call_returns");

        // Apply type narrowings from type predicate conditions (e.g., is_string(x))
        // in if/elif branches. This overrides Ref types within narrowed branches.
        // Returns dead branch ranges for unreachable code detection.
        let dead_branches = narrowing::resolve_type_narrowings(hir, &mut ctx);
        for (reason, range) in dead_branches {
            ctx.add_error(TypeError::UnreachableCode {
                reason,
                location: range,
            });
        }

        // Apply cross-arm narrowings from match expressions.
        let dead_arms = narrowing::resolve_cross_arm_narrowings(hir, &mut ctx);
        for (reason, range) in dead_arms {
            ctx.add_error(TypeError::UnreachableCode {
                reason,
                location: range,
            });
        }

        laps.lap("narrowing");

        // Resolve deferred tuple index accesses now that variable types are known.
        if deferred::resolve_deferred_tuple_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        // Check the value assigned through `a[i][j] = v` against the element it replaces.
        if deferred::resolve_deferred_index_assigns(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        // Bind v[:key] bracket access return types to record field types.
        if deferred::resolve_record_field_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        // Resolve deferred selector field accesses (.field on records)
        deferred::resolve_selector_field_accesses(&mut ctx);
        laps.lap("deferred_accesses");

        // Propagate return types from user-defined function calls so that
        // f(x)["key"] bracket accesses below have concrete return types to inspect.
        deferred::resolve_generator_yields(&mut ctx);
        deferred::propagate_user_call_returns(&mut ctx);
        laps.lap("propagate_user_call_returns");

        // Resolve f(x)["key"] bracket accesses on function return values.
        // Must run after propagate_user_call_returns so ret_ty is concrete.
        if deferred::resolve_deferred_call_return_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        laps.lap("deferred_accesses");

        // Process deferred overload resolutions (operators with type variable operands).
        deferred::resolve_deferred_overloads(&mut ctx);
        laps.lap("resolve_deferred_overloads");

        // Selectors applied to a value that an overload call just resolved (`first() | .h1.depth`).
        while deferred::resolve_selector_field_accesses(&mut ctx) | deferred::resolve_attr_calls(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        // Re-run deferred tuple accesses after overload resolution, because some variable
        // types (e.g., the return type of `first(xs)`) may only be resolved after
        // `resolve_deferred_overloads` runs. This ensures that index accesses on variables
        // with union types containing unresolved vars (e.g., `Union(None, Var)`) are
        // retried with the now-concrete member types.
        if deferred::resolve_deferred_tuple_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }

        laps.lap("deferred_accesses");

        // Resolve deferred try/catch branch-type merges, then retry accesses/operators
        // that depend on a `let v = try: ... catch: ...` variable's now-final type.
        deferred::resolve_deferred_try_catches(&mut ctx);
        if deferred::resolve_deferred_tuple_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }
        laps.lap("deferred_try_catches");
        deferred::resolve_deferred_overloads(&mut ctx);
        laps.lap("resolve_deferred_overloads");

        // The types settled by the passes above may let an operation that was left waiting be
        // resolved.
        if deferred::resolve_record_field_accesses(&mut ctx) {
            unify::solve_constraints(&mut ctx);
        }
        deferred::resolve_deferred_overloads(&mut ctx);
        laps.lap("resolve_deferred_overloads");

        // Check operators inside user-defined function bodies against call-site types.
        // Uses local substitution (original params → call-site args) without modifying
        // global state, so multiple call sites don't interfere.
        deferred::check_user_call_body_operators(hir, &mut ctx);
        laps.lap("check_user_call_body_operators");

        // Check pattern match exhaustiveness (reuses the children index from constraint generation)
        if !self.options.no_exhaustive_patterns {
            for e in exhaustiveness::check_match_exhaustiveness(hir, &mut ctx, &children_index) {
                ctx.add_error(e);
            }
        }

        laps.lap("exhaustiveness");

        // Collect errors before finalizing
        let mut errors = ctx.take_errors();
        // Module diagnostics are never reported. A stage threaded twice reports its error twice.
        let mut seen = rustc_hash::FxHashSet::default();
        errors.retain(|e| !e.location().is_some_and(|r| is_module_range(&r)) && seen.insert(format!("{e:?}")));

        let errors = certainty::classify_all(hir, &ctx, errors, self.options.input_type.is_some());
        laps.lap("classify");

        // Store inferred types
        self.symbol_types = ctx.finalize();
        laps.lap("finalize");

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
    use mq_hir::{HirError, SymbolKind};
    use rstest::rstest;

    #[rstest]
    #[case::number_elements("[1, 2] | map(fn: self + 1)")]
    #[case::pipe_in_body("[\"a b\"] | map(fn: split(\" \") | len)")]
    #[case::nested("[[1, 2]] | map(fn: map(fn: self + 1))")]
    #[case::in_function("def f(xs): map(xs, fn: self * 2); | f([1])")]
    fn test_fn_without_params_type_checks(#[case] code: &str) {
        let mut hir = Hir::default();
        hir.add_code(None, code);
        let errors = TypeChecker::new().check(&hir);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[rstest]
    #[case::dict_value_of_other_type("var d = dict() | d[\"a\"] = true | get(d, \"a\")")]
    #[case::array_element_compound("var a = [1, 2] | a[0] += 1 | len(a)")]
    #[case::new_record_field("var d = {\"a\": 1} | d[\"b\"] = 2 | d")]
    #[case::read_added_record_field("var d = {\"a\": 1} | d[\"b\"] = 2 | d[\"b\"]")]
    #[case::read_retyped_record_field("var d = {\"a\": 1} | d[\"a\"] = \"x\" | d[\"a\"] | upcase()")]
    #[case::read_in_function_called_after_write("var d = {\"a\": 1} | def f(): d[\"b\"]; | d[\"b\"] = 2 | f()")]
    fn test_index_assign_keeps_the_variable_type(#[case] code: &str) {
        let mut hir = Hir::default();
        hir.add_code(None, code);
        let errors = TypeChecker::new().check(&hir);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[rstest]
    #[case::array_element("var a = [1, 2] | a[0] = \"x\" | a", true)]
    #[case::array_element_ok("var a = [1, 2] | a[0] = 3 | a", false)]
    #[case::chained_element("var a = [[1, 2], [3, 4]] | a[0][1] = 5 | a", false)]
    #[case::chained_element_mismatch("var a = [[1, 2], [3, 4]] | a[0][1] = \"x\" | a", true)]
    #[case::chained_dynamic_index_mismatch("var i = 0 | var a = [[1]] | a[i][i] = \"x\" | a", true)]
    #[case::chained_three_levels_mismatch("var a = [[[1]]] | a[0][0][0] = \"x\" | a", true)]
    #[case::chained_record_key("var a = [{\"k\": 1}] | a[0][\"k\"] = \"x\" | a", false)]
    #[case::slice_target("var a = [1, 2, 3] | a[0:1] = [9] | a", false)]
    fn test_index_assign_checks_the_element_type_in_strict_mode(#[case] code: &str, #[case] rejected: bool) {
        let check = |strict_array| {
            let mut hir = Hir::default();
            hir.add_code(None, code);
            TypeChecker::with_options(TypeCheckerOptions {
                strict_array,
                ..Default::default()
            })
            .check(&hir)
        };
        assert!(check(false).is_empty());
        assert_eq!(!check(true).is_empty(), rejected);
    }

    #[rstest]
    #[case::fold_passes_two_arguments("fold([1, 2], 0, fn: self)")]
    #[case::compound_assign_reads_missing_field("var d = {\"a\": 1} | d[\"b\"] += 1 | d")]
    #[case::read_before_write("var d = {\"a\": 1} | d[\"b\"] | d[\"b\"] = 2")]
    #[case::read_in_assigned_value("var d = {\"a\": 1} | d[\"b\"] = d[\"b\"] + 1")]
    #[case::retype_after_read("var d = {\"a\": 1} | let y = d[\"a\"] | upcase(y) | d[\"a\"] = \"x\"")]
    #[case::read_in_own_function_before_write("def f(): var d = {\"a\": 1} | d[\"b\"] | d[\"b\"] = 2;")]
    #[case::function_read_of_unwritten_field("var d = {\"a\": 1} | def f(): d[\"c\"]; | d[\"b\"] = 2 | f()")]
    #[case::unwritten_field_still_checked("var d = {\"a\": 1} | d[\"b\"] = 2 | d[\"c\"]")]
    #[case::elements_not_numbers("[\"a\"] | map(fn: abs())")]
    #[case::piped_call_in_body("[1] | map(fn: split(\" \"))")]
    fn test_fn_without_params_rejects_incompatible_calls(#[case] code: &str) {
        let mut hir = Hir::default();
        hir.add_code(None, code);
        let errors = TypeChecker::new().check(&hir);
        assert!(!errors.is_empty(), "{code} should not type check");
    }

    #[test]
    fn test_generator_code_does_not_produce_type_errors() {
        let mut hir = Hir::default();
        hir.add_code(None, "def g(): yield: 1; | let s = g() | next(s)");
        assert_no_unresolved_next(&hir);
        let errors = TypeChecker::new().check(&hir);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_coroutine_helpers_do_not_produce_type_errors() {
        let mut hir = Hir::default();
        hir.add_code(
            None,
            "def g(): yield: 1; | let s = g() | is_coroutine(s) | next(s) | send(s, 2) | status(s) | close(s)",
        );
        let errors = TypeChecker::new().check(&hir);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_local_next_definition_shadows_the_generator_builtin() {
        let mut hir = Hir::default();
        hir.add_code(None, "def next(value): value + 1; | next(41)");
        assert_no_unresolved_next(&hir);
        let errors = TypeChecker::new().check(&hir);
        assert!(errors.is_empty(), "{errors:?}");
    }

    fn assert_no_unresolved_next(hir: &Hir) {
        assert!(
            !hir.errors().iter().any(|error| {
                matches!(error, HirError::UnresolvedSymbol { symbol, .. } if symbol.value.as_deref() == Some("next"))
            }),
            "{:#?}",
            hir.errors()
        );
    }

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
    #[case(TypeError::Mismatch { expected: "n".into(), found: "s".into(), location: Some(mq_lang::Range { start: mq_lang::Position { line: 1, column: 5 }, end: mq_lang::Position { line: 1, column: 6 } }), context: None }, Some(mq_lang::Range { start: mq_lang::Position { line: 1, column: 5 }, end: mq_lang::Position { line: 1, column: 6 } }))]
    #[case(TypeError::UnificationError { left: "l".into(), right: "r".into(), location: Some(mq_lang::Range { start: mq_lang::Position { line: 2, column: 10 }, end: mq_lang::Position { line: 2, column: 11 } }), context: None }, Some(mq_lang::Range { start: mq_lang::Position { line: 2, column: 10 }, end: mq_lang::Position { line: 2, column: 11 } }))]
    #[case(TypeError::TypeVarNotFound("a".into()), None)]
    fn test_type_error_location(#[case] err: TypeError, #[case] expected: Option<mq_lang::Range>) {
        assert_eq!(err.location(), expected);
    }

    #[rstest]
    #[case::first_line("1 + \"a\"", 1, 5, 1, 8, 4, 3)]
    #[case::later_line("let x = 1\n| x + \"a\"", 2, 7, 2, 10, 16, 3)]
    #[case::multibyte("\"あい\" + 1", 1, 8, 1, 9, 11, 1)]
    fn test_type_error_span(
        #[case] source: &str,
        #[case] start_line: u32,
        #[case] start_column: usize,
        #[case] end_line: u32,
        #[case] end_column: usize,
        #[case] offset: usize,
        #[case] len: usize,
    ) {
        let err = TypeError::UndefinedSymbol {
            name: "x".into(),
            location: Some(mq_lang::Range {
                start: mq_lang::Position {
                    line: start_line,
                    column: start_column,
                },
                end: mq_lang::Position {
                    line: end_line,
                    column: end_column,
                },
            }),
        };
        let span = err.span(source).unwrap();
        assert_eq!((span.offset(), span.len()), (offset, len));
    }

    #[test]
    fn test_type_error_span_without_location() {
        assert!(TypeError::TypeVarNotFound("a".into()).span("1").is_none());
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
