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
    /// Returns a list of type errors found. An empty list means success.
    pub fn check(&mut self, hir: &Hir) -> Vec<TypeError> {
        // Create inference context
        let mut ctx = infer::InferenceContext::new();

        // Generate builtin type signatures
        builtin::register_all(&mut ctx);

        // Generate constraints from HIR
        constraint::generate_constraints(hir, &mut ctx);

        // Solve constraints through unification
        unify::solve_constraints(&mut ctx);

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
    pub fn symbol_types(&self) -> &FxHashMap<SymbolId, TypeScheme> {
        &self.symbol_types
    }

    /// Adds builtin function type signatures
    fn add_builtin_types(&self, ctx: &mut infer::InferenceContext) {
        use types::Type;

        // ===== Arithmetic Operators =====

        // Addition operator: supports both numbers and strings
        // Overload 1: (number, number) -> number
        ctx.register_builtin("+", Type::function(vec![Type::Number, Type::Number], Type::Number));
        // Overload 2: (string, string) -> string
        ctx.register_builtin("+", Type::function(vec![Type::String, Type::String], Type::String));
        ctx.register_builtin("add", Type::function(vec![Type::Number, Type::Number], Type::Number));

        // Other arithmetic operators: (number, number) -> number
        for op in ["-", "*", "/", "%", "^"] {
            let params = vec![Type::Number, Type::Number];
            let ret = Type::Number;
            ctx.register_builtin(op, Type::function(params, ret));
        }
        ctx.register_builtin("sub", Type::function(vec![Type::Number, Type::Number], Type::Number));
        ctx.register_builtin("mul", Type::function(vec![Type::Number, Type::Number], Type::Number));
        ctx.register_builtin("div", Type::function(vec![Type::Number, Type::Number], Type::Number));
        ctx.register_builtin("mod", Type::function(vec![Type::Number, Type::Number], Type::Number));
        ctx.register_builtin("pow", Type::function(vec![Type::Number, Type::Number], Type::Number));

        // ===== Comparison Operators =====

        // Comparison operators: (number, number) -> bool
        for op in ["<", ">", "<=", ">="] {
            let params = vec![Type::Number, Type::Number];
            let ret = Type::Bool;
            ctx.register_builtin(op, Type::function(params, ret));
        }
        ctx.register_builtin("lt", Type::function(vec![Type::Number, Type::Number], Type::Bool));
        ctx.register_builtin("gt", Type::function(vec![Type::Number, Type::Number], Type::Bool));
        ctx.register_builtin("lte", Type::function(vec![Type::Number, Type::Number], Type::Bool));
        ctx.register_builtin("gte", Type::function(vec![Type::Number, Type::Number], Type::Bool));

        // Equality operators: forall a. (a, a) -> bool
        // For now, we'll use type variables
        for op in ["==", "!="] {
            let a = ctx.fresh_var();
            let params = vec![Type::Var(a), Type::Var(a)];
            let ret = Type::Bool;
            ctx.register_builtin(op, Type::function(params, ret));
        }
        let eq_a = ctx.fresh_var();
        ctx.register_builtin("eq", Type::function(vec![Type::Var(eq_a), Type::Var(eq_a)], Type::Bool));
        let ne_a = ctx.fresh_var();
        ctx.register_builtin("ne", Type::function(vec![Type::Var(ne_a), Type::Var(ne_a)], Type::Bool));

        // ===== Logical Operators =====

        // Logical operators: (bool, bool) -> bool
        for op in ["and", "or", "&&", "||"] {
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
        ctx.register_builtin("negate", Type::function(vec![Type::Number], Type::Number));

        // ===== Mathematical Functions =====

        // Unary math functions: number -> number
        for func in ["abs", "ceil", "floor", "round", "trunc"] {
            ctx.register_builtin(func, Type::function(vec![Type::Number], Type::Number));
        }

        // Binary math functions with overloads
        // min/max: support numbers, strings, and symbols
        ctx.register_builtin("min", Type::function(vec![Type::Number, Type::Number], Type::Number));
        ctx.register_builtin("min", Type::function(vec![Type::String, Type::String], Type::String));
        ctx.register_builtin("min", Type::function(vec![Type::Symbol, Type::Symbol], Type::Symbol));

        ctx.register_builtin("max", Type::function(vec![Type::Number, Type::Number], Type::Number));
        ctx.register_builtin("max", Type::function(vec![Type::String, Type::String], Type::String));
        ctx.register_builtin("max", Type::function(vec![Type::Symbol, Type::Symbol], Type::Symbol));

        // Special number constants/functions
        ctx.register_builtin("nan", Type::function(vec![], Type::Number));
        ctx.register_builtin("infinite", Type::function(vec![], Type::Number));
        ctx.register_builtin("is_nan", Type::function(vec![Type::Number], Type::Bool));

        // ===== String Functions =====

        // Case conversion: string -> string
        ctx.register_builtin("downcase", Type::function(vec![Type::String], Type::String));
        ctx.register_builtin("upcase", Type::function(vec![Type::String], Type::String));
        ctx.register_builtin("trim", Type::function(vec![Type::String], Type::String));

        // String search/test functions
        ctx.register_builtin(
            "starts_with",
            Type::function(vec![Type::String, Type::String], Type::Bool),
        );
        ctx.register_builtin(
            "ends_with",
            Type::function(vec![Type::String, Type::String], Type::Bool),
        );
        ctx.register_builtin("index", Type::function(vec![Type::String, Type::String], Type::Number));
        ctx.register_builtin("rindex", Type::function(vec![Type::String, Type::String], Type::Number));

        // String manipulation
        ctx.register_builtin(
            "replace",
            Type::function(vec![Type::String, Type::String, Type::String], Type::String),
        );
        ctx.register_builtin(
            "gsub",
            Type::function(vec![Type::String, Type::String, Type::String], Type::String),
        );
        ctx.register_builtin(
            "split",
            Type::function(vec![Type::String, Type::String], Type::array(Type::String)),
        );
        ctx.register_builtin(
            "join",
            Type::function(vec![Type::array(Type::String), Type::String], Type::String),
        );

        // Character/codepoint conversion
        ctx.register_builtin("explode", Type::function(vec![Type::String], Type::array(Type::Number)));
        ctx.register_builtin("implode", Type::function(vec![Type::array(Type::Number)], Type::String));

        // String properties
        ctx.register_builtin("utf8bytelen", Type::function(vec![Type::String], Type::Number));

        // Regular expressions
        ctx.register_builtin(
            "regex_match",
            Type::function(vec![Type::String, Type::String], Type::array(Type::String)),
        );

        // Encoding functions
        ctx.register_builtin("base64", Type::function(vec![Type::String], Type::String));
        ctx.register_builtin("base64d", Type::function(vec![Type::String], Type::String));
        ctx.register_builtin("url_encode", Type::function(vec![Type::String], Type::String));

        // Capture: (string, pattern) -> {string: string}
        let capture_k = ctx.fresh_var();
        let capture_v = ctx.fresh_var();
        ctx.register_builtin(
            "capture",
            Type::function(
                vec![Type::String, Type::String],
                Type::dict(Type::Var(capture_k), Type::Var(capture_v)),
            ),
        );

        // ===== Array Functions =====

        // Array manipulation functions with polymorphic types
        let flatten_a = ctx.fresh_var();
        ctx.register_builtin(
            "flatten",
            Type::function(
                vec![Type::array(Type::array(Type::Var(flatten_a)))],
                Type::array(Type::Var(flatten_a)),
            ),
        );

        let reverse_a = ctx.fresh_var();
        ctx.register_builtin(
            "reverse",
            Type::function(
                vec![Type::array(Type::Var(reverse_a))],
                Type::array(Type::Var(reverse_a)),
            ),
        );

        let sort_a = ctx.fresh_var();
        ctx.register_builtin(
            "sort",
            Type::function(vec![Type::array(Type::Var(sort_a))], Type::array(Type::Var(sort_a))),
        );

        let uniq_a = ctx.fresh_var();
        ctx.register_builtin(
            "uniq",
            Type::function(vec![Type::array(Type::Var(uniq_a))], Type::array(Type::Var(uniq_a))),
        );

        let compact_a = ctx.fresh_var();
        ctx.register_builtin(
            "compact",
            Type::function(
                vec![Type::array(Type::Var(compact_a))],
                Type::array(Type::Var(compact_a)),
            ),
        );

        // Array access/search functions
        let len_a = ctx.fresh_var();
        ctx.register_builtin("len", Type::function(vec![Type::array(Type::Var(len_a))], Type::Number));
        // len also works on strings
        ctx.register_builtin("len", Type::function(vec![Type::String], Type::Number));

        // slice: ([a], number, number) -> [a]
        let slice_a = ctx.fresh_var();
        ctx.register_builtin(
            "slice",
            Type::function(
                vec![Type::array(Type::Var(slice_a)), Type::Number, Type::Number],
                Type::array(Type::Var(slice_a)),
            ),
        );

        // insert: ([a], number, a) -> [a]
        let insert_a = ctx.fresh_var();
        ctx.register_builtin(
            "insert",
            Type::function(
                vec![Type::array(Type::Var(insert_a)), Type::Number, Type::Var(insert_a)],
                Type::array(Type::Var(insert_a)),
            ),
        );

        // Array creation functions
        // array: variadic function, create array from arguments
        // This is tricky to type correctly with variadic args, so we'll use a polymorphic type
        let array_a = ctx.fresh_var();
        ctx.register_builtin(
            "array",
            Type::function(vec![Type::Var(array_a)], Type::array(Type::Var(array_a))),
        );

        // range: (number, number) -> [number]
        ctx.register_builtin(
            "range",
            Type::function(vec![Type::Number, Type::Number], Type::array(Type::Number)),
        );
        // range with step: (number, number, number) -> [number]
        ctx.register_builtin(
            "range",
            Type::function(
                vec![Type::Number, Type::Number, Type::Number],
                Type::array(Type::Number),
            ),
        );

        // repeat: (a, number) -> [a]
        let repeat_a = ctx.fresh_var();
        ctx.register_builtin(
            "repeat",
            Type::function(
                vec![Type::Var(repeat_a), Type::Number],
                Type::array(Type::Var(repeat_a)),
            ),
        );

        // ===== Dictionary Functions =====

        // keys: {k: v} -> [k]
        let keys_k = ctx.fresh_var();
        let keys_v = ctx.fresh_var();
        ctx.register_builtin(
            "keys",
            Type::function(
                vec![Type::dict(Type::Var(keys_k), Type::Var(keys_v))],
                Type::array(Type::Var(keys_k)),
            ),
        );

        // values: {k: v} -> [v]
        let values_k = ctx.fresh_var();
        let values_v = ctx.fresh_var();
        ctx.register_builtin(
            "values",
            Type::function(
                vec![Type::dict(Type::Var(values_k), Type::Var(values_v))],
                Type::array(Type::Var(values_v)),
            ),
        );

        // entries: {k: v} -> [[k, v]]
        let entries_k = ctx.fresh_var();
        let entries_v = ctx.fresh_var();
        ctx.register_builtin(
            "entries",
            Type::function(
                vec![Type::dict(Type::Var(entries_k), Type::Var(entries_v))],
                Type::array(Type::array(Type::Var(entries_k))),
            ),
        );

        // get: ({k: v}, k) -> v
        let get_k = ctx.fresh_var();
        let get_v = ctx.fresh_var();
        ctx.register_builtin(
            "get",
            Type::function(
                vec![Type::dict(Type::Var(get_k), Type::Var(get_v)), Type::Var(get_k)],
                Type::Var(get_v),
            ),
        );

        // set: ({k: v}, k, v) -> {k: v}
        let set_k = ctx.fresh_var();
        let set_v = ctx.fresh_var();
        ctx.register_builtin(
            "set",
            Type::function(
                vec![
                    Type::dict(Type::Var(set_k), Type::Var(set_v)),
                    Type::Var(set_k),
                    Type::Var(set_v),
                ],
                Type::dict(Type::Var(set_k), Type::Var(set_v)),
            ),
        );

        // del: ({k: v}, k) -> {k: v}
        let del_k = ctx.fresh_var();
        let del_v = ctx.fresh_var();
        ctx.register_builtin(
            "del",
            Type::function(
                vec![Type::dict(Type::Var(del_k), Type::Var(del_v)), Type::Var(del_k)],
                Type::dict(Type::Var(del_k), Type::Var(del_v)),
            ),
        );

        // update: ({k: v}, {k: v}) -> {k: v}
        let update_k = ctx.fresh_var();
        let update_v = ctx.fresh_var();
        ctx.register_builtin(
            "update",
            Type::function(
                vec![
                    Type::dict(Type::Var(update_k), Type::Var(update_v)),
                    Type::dict(Type::Var(update_k), Type::Var(update_v)),
                ],
                Type::dict(Type::Var(update_k), Type::Var(update_v)),
            ),
        );

        // dict: variadic function to create dictionaries
        let dict_k = ctx.fresh_var();
        let dict_v = ctx.fresh_var();
        ctx.register_builtin(
            "dict",
            Type::function(
                vec![Type::Var(dict_k), Type::Var(dict_v)],
                Type::dict(Type::Var(dict_k), Type::Var(dict_v)),
            ),
        );

        // ===== Type Conversion Functions =====

        // to_number: string -> number
        ctx.register_builtin("to_number", Type::function(vec![Type::String], Type::Number));

        // to_string: a -> string
        let to_string_a = ctx.fresh_var();
        ctx.register_builtin("to_string", Type::function(vec![Type::Var(to_string_a)], Type::String));

        // to_array: a -> [a]
        let to_array_a = ctx.fresh_var();
        ctx.register_builtin(
            "to_array",
            Type::function(vec![Type::Var(to_array_a)], Type::array(Type::Var(to_array_a))),
        );

        // type: a -> string
        let type_a = ctx.fresh_var();
        ctx.register_builtin("type", Type::function(vec![Type::Var(type_a)], Type::String));

        // ===== Date/Time Functions =====

        // now: () -> number
        ctx.register_builtin("now", Type::function(vec![], Type::Number));

        // from_date: string -> number
        ctx.register_builtin("from_date", Type::function(vec![Type::String], Type::Number));

        // to_date: (number, string) -> string
        ctx.register_builtin(
            "to_date",
            Type::function(vec![Type::Number, Type::String], Type::String),
        );

        // ===== I/O and Control Flow Functions =====

        // print: a -> a (side effect: prints to stdout)
        let print_a = ctx.fresh_var();
        ctx.register_builtin("print", Type::function(vec![Type::Var(print_a)], Type::Var(print_a)));

        // stderr: a -> a (side effect: prints to stderr)
        let stderr_a = ctx.fresh_var();
        ctx.register_builtin("stderr", Type::function(vec![Type::Var(stderr_a)], Type::Var(stderr_a)));

        // error: string -> never (throws error)
        ctx.register_builtin("error", Type::function(vec![Type::String], Type::None));

        // halt: number -> never (exits with code)
        ctx.register_builtin("halt", Type::function(vec![Type::Number], Type::None));

        // input: () -> string
        ctx.register_builtin("input", Type::function(vec![], Type::String));

        // ===== Utility Functions =====

        // coalesce: (a, a) -> a (returns first non-null value)
        let coalesce_a = ctx.fresh_var();
        ctx.register_builtin(
            "coalesce",
            Type::function(
                vec![Type::Var(coalesce_a), Type::Var(coalesce_a)],
                Type::Var(coalesce_a),
            ),
        );

        // ===== Markdown-specific Functions =====

        // to_markdown: a -> markdown
        let to_markdown_a = ctx.fresh_var();
        ctx.register_builtin(
            "to_markdown",
            Type::function(vec![Type::Var(to_markdown_a)], Type::Markdown),
        );

        // to_markdown_string: markdown -> string
        ctx.register_builtin("to_markdown_string", Type::function(vec![Type::Markdown], Type::String));

        // to_text: markdown -> string
        ctx.register_builtin("to_text", Type::function(vec![Type::Markdown], Type::String));

        // to_html: markdown -> string
        ctx.register_builtin("to_html", Type::function(vec![Type::Markdown], Type::String));

        // Markdown manipulation functions (simplified type signatures)
        ctx.register_builtin(
            "to_h",
            Type::function(vec![Type::Markdown, Type::Number], Type::Markdown),
        );
        ctx.register_builtin(
            "to_link",
            Type::function(vec![Type::String, Type::String, Type::String], Type::Markdown),
        );
        ctx.register_builtin(
            "to_image",
            Type::function(vec![Type::String, Type::String, Type::String], Type::Markdown),
        );
        ctx.register_builtin(
            "to_code",
            Type::function(vec![Type::Markdown, Type::String], Type::Markdown),
        );
        ctx.register_builtin("to_code_inline", Type::function(vec![Type::Markdown], Type::Markdown));
        ctx.register_builtin("to_strong", Type::function(vec![Type::Markdown], Type::Markdown));
        ctx.register_builtin("to_em", Type::function(vec![Type::Markdown], Type::Markdown));
        ctx.register_builtin(
            "increase_header_level",
            Type::function(vec![Type::Markdown], Type::Markdown),
        );
        ctx.register_builtin(
            "decrease_header_level",
            Type::function(vec![Type::Markdown], Type::Markdown),
        );

        // Markdown attribute functions
        ctx.register_builtin("get_title", Type::function(vec![Type::Markdown], Type::String));
        ctx.register_builtin("get_url", Type::function(vec![Type::Markdown], Type::String));
        ctx.register_builtin("attr", Type::function(vec![Type::Markdown, Type::String], Type::String));
        ctx.register_builtin(
            "set_attr",
            Type::function(vec![Type::Markdown, Type::String, Type::String], Type::Markdown),
        );

        // Additional Markdown manipulation functions
        ctx.register_builtin("to_hr", Type::function(vec![], Type::Markdown));
        ctx.register_builtin("to_math", Type::function(vec![Type::Markdown], Type::Markdown));
        ctx.register_builtin("to_math_inline", Type::function(vec![Type::Markdown], Type::Markdown));
        ctx.register_builtin(
            "to_md_list",
            Type::function(vec![Type::Markdown, Type::Number], Type::Markdown),
        );
        ctx.register_builtin("to_md_name", Type::function(vec![Type::Markdown], Type::String));
        ctx.register_builtin("to_md_table_row", Type::function(vec![Type::Markdown], Type::Markdown));
        let table_cell_a = ctx.fresh_var();
        ctx.register_builtin(
            "to_md_table_cell",
            Type::function(
                vec![Type::Var(table_cell_a), Type::Number, Type::Number],
                Type::Markdown,
            ),
        );
        ctx.register_builtin("to_md_text", Type::function(vec![Type::Markdown], Type::String));
        ctx.register_builtin("to_mdx", Type::function(vec![Type::Markdown], Type::Markdown));
        ctx.register_builtin(
            "set_check",
            Type::function(vec![Type::Markdown, Type::Bool], Type::Markdown),
        );
        ctx.register_builtin(
            "set_code_block_lang",
            Type::function(vec![Type::Markdown, Type::String], Type::Markdown),
        );
        ctx.register_builtin(
            "set_list_ordered",
            Type::function(vec![Type::Markdown, Type::Bool], Type::Markdown),
        );
        ctx.register_builtin(
            "set_ref",
            Type::function(vec![Type::Markdown, Type::String], Type::Markdown),
        );

        // ===== Variable/Symbol Management Functions =====

        let all_sym_a = ctx.fresh_var();
        ctx.register_builtin("all_symbols", Type::function(vec![], Type::array(Type::Var(all_sym_a))));
        ctx.register_builtin("get_variable", Type::function(vec![Type::String], Type::String));
        ctx.register_builtin(
            "set_variable",
            Type::function(vec![Type::String, Type::String], Type::None),
        );
        ctx.register_builtin("intern", Type::function(vec![Type::String], Type::Symbol));

        // ===== Debug/Control Functions =====

        ctx.register_builtin("is_debug_mode", Type::function(vec![], Type::Bool));
        ctx.register_builtin("breakpoint", Type::function(vec![], Type::None));
        let assert_a = ctx.fresh_var();
        ctx.register_builtin(
            "assert",
            Type::function(vec![Type::Var(assert_a), Type::Var(assert_a)], Type::Var(assert_a)),
        );

        // ===== File I/O Functions =====

        ctx.register_builtin("read_file", Type::function(vec![Type::String], Type::String));
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
