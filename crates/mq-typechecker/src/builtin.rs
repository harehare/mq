//! Builtin function type signatures for the mq type system.
//!
//! This module registers type signatures for all builtin functions and operators.
//! Builtins are organized by category for maintainability. To add a new builtin,
//! find the appropriate category function and add a registration call.

use crate::infer::InferenceContext;
use crate::types::Type;

/// Registers all builtin function and operator type signatures.
pub fn register_all(ctx: &mut InferenceContext) {
    register_arithmetic(ctx);
    register_comparison(ctx);
    register_logical(ctx);
    register_math(ctx);
    register_string(ctx);
    register_array(ctx);
    register_dict(ctx);
    register_type_conversion(ctx);
    register_datetime(ctx);
    register_io(ctx);
    register_utility(ctx);
    register_markdown(ctx);
    register_variable(ctx);
    register_debug(ctx);
    register_file_io(ctx);
}

// =============================================================================
// Helper functions
// =============================================================================

/// Registers multiple builtins with the same type signature.
fn register_many(ctx: &mut InferenceContext, names: &[&str], params: Vec<Type>, ret: Type) {
    for name in names {
        ctx.register_builtin(name, Type::function(params.clone(), ret.clone()));
    }
}

/// Registers a nullary builtin: `() -> ret`.
fn register_nullary(ctx: &mut InferenceContext, name: &str, ret: Type) {
    ctx.register_builtin(name, Type::function(vec![], ret));
}

/// Registers a unary builtin: `(param) -> ret`.
fn register_unary(ctx: &mut InferenceContext, name: &str, param: Type, ret: Type) {
    ctx.register_builtin(name, Type::function(vec![param], ret));
}

/// Registers a binary builtin: `(p1, p2) -> ret`.
fn register_binary(ctx: &mut InferenceContext, name: &str, p1: Type, p2: Type, ret: Type) {
    ctx.register_builtin(name, Type::function(vec![p1, p2], ret));
}

/// Registers a ternary builtin: `(p1, p2, p3) -> ret`.
fn register_ternary(ctx: &mut InferenceContext, name: &str, p1: Type, p2: Type, p3: Type, ret: Type) {
    ctx.register_builtin(name, Type::function(vec![p1, p2, p3], ret));
}

// =============================================================================
// Category registration functions
// =============================================================================

/// Arithmetic operators: +, -, *, /, %, ^, add, sub, mul, div, mod, pow
fn register_arithmetic(ctx: &mut InferenceContext) {
    // Addition: supports both numbers and strings
    register_binary(ctx, "+", Type::Number, Type::Number, Type::Number);
    register_binary(ctx, "+", Type::String, Type::String, Type::String);
    register_binary(ctx, "add", Type::Number, Type::Number, Type::Number);

    // Other arithmetic operators: (number, number) -> number
    register_many(
        ctx,
        &["-", "*", "/", "%", "^"],
        vec![Type::Number, Type::Number],
        Type::Number,
    );
    register_many(
        ctx,
        &["sub", "mul", "div", "mod", "pow"],
        vec![Type::Number, Type::Number],
        Type::Number,
    );
}

/// Comparison operators: <, >, <=, >=, ==, !=, lt, gt, lte, gte, eq, ne
fn register_comparison(ctx: &mut InferenceContext) {
    // Ordering: (number, number) -> bool
    register_many(
        ctx,
        &["<", ">", "<=", ">="],
        vec![Type::Number, Type::Number],
        Type::Bool,
    );
    register_many(
        ctx,
        &["lt", "gt", "lte", "gte"],
        vec![Type::Number, Type::Number],
        Type::Bool,
    );

    // Equality: forall a. (a, a) -> bool
    for name in ["==", "!="] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::Var(a), Type::Var(a), Type::Bool);
    }
    for name in ["eq", "ne"] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::Var(a), Type::Var(a), Type::Bool);
    }
}

/// Logical operators: &&, ||, !, and, or, not, unary-, negate
fn register_logical(ctx: &mut InferenceContext) {
    // Binary logical: (bool, bool) -> bool
    register_many(
        ctx,
        &["and", "or", "&&", "||"],
        vec![Type::Bool, Type::Bool],
        Type::Bool,
    );

    // Unary logical: bool -> bool
    register_unary(ctx, "!", Type::Bool, Type::Bool);
    register_unary(ctx, "not", Type::Bool, Type::Bool);

    // Unary minus: number -> number
    register_unary(ctx, "unary-", Type::Number, Type::Number);
    register_unary(ctx, "negate", Type::Number, Type::Number);
}

/// Mathematical functions: abs, ceil, floor, round, trunc, min, max, nan, infinite, is_nan
fn register_math(ctx: &mut InferenceContext) {
    // Unary math: number -> number
    register_many(
        ctx,
        &["abs", "ceil", "floor", "round", "trunc"],
        vec![Type::Number],
        Type::Number,
    );

    // min/max: support numbers, strings, and symbols
    for name in ["min", "max"] {
        register_binary(ctx, name, Type::Number, Type::Number, Type::Number);
        register_binary(ctx, name, Type::String, Type::String, Type::String);
        register_binary(ctx, name, Type::Symbol, Type::Symbol, Type::Symbol);
    }

    // Special number functions
    register_nullary(ctx, "nan", Type::Number);
    register_nullary(ctx, "infinite", Type::Number);
    register_unary(ctx, "is_nan", Type::Number, Type::Bool);
}

/// String functions: downcase, upcase, trim, starts_with, ends_with, etc.
fn register_string(ctx: &mut InferenceContext) {
    // Case conversion: string -> string
    register_many(ctx, &["downcase", "upcase", "trim"], vec![Type::String], Type::String);

    // String search: (string, string) -> bool/number
    register_binary(ctx, "starts_with", Type::String, Type::String, Type::Bool);
    register_binary(ctx, "ends_with", Type::String, Type::String, Type::Bool);
    register_binary(ctx, "index", Type::String, Type::String, Type::Number);
    register_binary(ctx, "rindex", Type::String, Type::String, Type::Number);

    // String manipulation
    register_ternary(ctx, "replace", Type::String, Type::String, Type::String, Type::String);
    register_ternary(ctx, "gsub", Type::String, Type::String, Type::String, Type::String);
    register_binary(ctx, "split", Type::String, Type::String, Type::array(Type::String));
    register_binary(ctx, "join", Type::array(Type::String), Type::String, Type::String);

    // Character/codepoint conversion
    register_unary(ctx, "explode", Type::String, Type::array(Type::Number));
    register_unary(ctx, "implode", Type::array(Type::Number), Type::String);

    // String properties
    register_unary(ctx, "utf8bytelen", Type::String, Type::Number);

    // Regular expressions
    register_binary(
        ctx,
        "regex_match",
        Type::String,
        Type::String,
        Type::array(Type::String),
    );

    // Encoding functions
    register_many(
        ctx,
        &["base64", "base64d", "url_encode"],
        vec![Type::String],
        Type::String,
    );

    // Capture: (string, pattern) -> {k: v}
    let k = ctx.fresh_var();
    let v = ctx.fresh_var();
    register_binary(
        ctx,
        "capture",
        Type::String,
        Type::String,
        Type::dict(Type::Var(k), Type::Var(v)),
    );
}

/// Array functions: flatten, reverse, sort, uniq, compact, len, slice, insert, range, repeat
fn register_array(ctx: &mut InferenceContext) {
    // Polymorphic array -> array functions
    for name in ["reverse", "sort", "uniq", "compact"] {
        let a = ctx.fresh_var();
        register_unary(ctx, name, Type::array(Type::Var(a)), Type::array(Type::Var(a)));
    }

    // flatten: [[a]] -> [a]
    let a = ctx.fresh_var();
    register_unary(
        ctx,
        "flatten",
        Type::array(Type::array(Type::Var(a))),
        Type::array(Type::Var(a)),
    );

    // len: [a] -> number, string -> number
    let a = ctx.fresh_var();
    register_unary(ctx, "len", Type::array(Type::Var(a)), Type::Number);
    register_unary(ctx, "len", Type::String, Type::Number);

    // slice: ([a], number, number) -> [a]
    let a = ctx.fresh_var();
    register_ternary(
        ctx,
        "slice",
        Type::array(Type::Var(a)),
        Type::Number,
        Type::Number,
        Type::array(Type::Var(a)),
    );

    // insert: ([a], number, a) -> [a]
    let a = ctx.fresh_var();
    register_ternary(
        ctx,
        "insert",
        Type::array(Type::Var(a)),
        Type::Number,
        Type::Var(a),
        Type::array(Type::Var(a)),
    );

    // array: a -> [a]
    let a = ctx.fresh_var();
    register_unary(ctx, "array", Type::Var(a), Type::array(Type::Var(a)));

    // range: (number, number) -> [number], (number, number, number) -> [number]
    register_binary(ctx, "range", Type::Number, Type::Number, Type::array(Type::Number));
    register_ternary(
        ctx,
        "range",
        Type::Number,
        Type::Number,
        Type::Number,
        Type::array(Type::Number),
    );

    // repeat: (a, number) -> [a]
    let a = ctx.fresh_var();
    register_binary(ctx, "repeat", Type::Var(a), Type::Number, Type::array(Type::Var(a)));
}

/// Dictionary functions: keys, values, entries, get, set, del, update, dict
fn register_dict(ctx: &mut InferenceContext) {
    // keys: {k: v} -> [k]
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(
        ctx,
        "keys",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::array(Type::Var(k)),
    );

    // values: {k: v} -> [v]
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(
        ctx,
        "values",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::array(Type::Var(v)),
    );

    // entries: {k: v} -> [[k, v]]
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(
        ctx,
        "entries",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::array(Type::array(Type::Var(k))),
    );

    // get: ({k: v}, k) -> v
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "get",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::Var(k),
        Type::Var(v),
    );

    // set: ({k: v}, k, v) -> {k: v}
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_ternary(
        ctx,
        "set",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::Var(k),
        Type::Var(v),
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // del: ({k: v}, k) -> {k: v}
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "del",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::Var(k),
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // update: ({k: v}, {k: v}) -> {k: v}
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "update",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // dict: (k, v) -> {k: v}
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "dict",
        Type::Var(k),
        Type::Var(v),
        Type::dict(Type::Var(k), Type::Var(v)),
    );
}

/// Type conversion functions: to_number, to_string, to_array, type
fn register_type_conversion(ctx: &mut InferenceContext) {
    register_unary(ctx, "to_number", Type::String, Type::Number);

    let a = ctx.fresh_var();
    register_unary(ctx, "to_string", Type::Var(a), Type::String);

    let a = ctx.fresh_var();
    register_unary(ctx, "to_array", Type::Var(a), Type::array(Type::Var(a)));

    let a = ctx.fresh_var();
    register_unary(ctx, "type", Type::Var(a), Type::String);
}

/// Date/time functions: now, from_date, to_date
fn register_datetime(ctx: &mut InferenceContext) {
    register_nullary(ctx, "now", Type::Number);
    register_unary(ctx, "from_date", Type::String, Type::Number);
    register_binary(ctx, "to_date", Type::Number, Type::String, Type::String);
}

/// I/O and control flow functions: print, stderr, error, halt, input
fn register_io(ctx: &mut InferenceContext) {
    // print/stderr: a -> a (side effect)
    let a = ctx.fresh_var();
    register_unary(ctx, "print", Type::Var(a), Type::Var(a));
    let a = ctx.fresh_var();
    register_unary(ctx, "stderr", Type::Var(a), Type::Var(a));

    register_unary(ctx, "error", Type::String, Type::None);
    register_unary(ctx, "halt", Type::Number, Type::None);
    register_nullary(ctx, "input", Type::String);
}

/// Utility functions: coalesce
fn register_utility(ctx: &mut InferenceContext) {
    let a = ctx.fresh_var();
    register_binary(ctx, "coalesce", Type::Var(a), Type::Var(a), Type::Var(a));
}

/// Markdown-specific functions
fn register_markdown(ctx: &mut InferenceContext) {
    // to_markdown: a -> markdown
    let a = ctx.fresh_var();
    register_unary(ctx, "to_markdown", Type::Var(a), Type::Markdown);

    // markdown -> string functions
    register_many(
        ctx,
        &["to_markdown_string", "to_text", "to_html"],
        vec![Type::Markdown],
        Type::String,
    );

    // Markdown manipulation: markdown -> markdown
    register_many(
        ctx,
        &[
            "to_code_inline",
            "to_strong",
            "to_em",
            "increase_header_level",
            "decrease_header_level",
            "to_math",
            "to_math_inline",
            "to_md_table_row",
            "to_mdx",
        ],
        vec![Type::Markdown],
        Type::Markdown,
    );

    // (markdown, number) -> markdown
    register_binary(ctx, "to_h", Type::Markdown, Type::Number, Type::Markdown);
    register_binary(ctx, "to_md_list", Type::Markdown, Type::Number, Type::Markdown);

    // (markdown, string) -> markdown/string
    register_binary(ctx, "to_code", Type::Markdown, Type::String, Type::Markdown);
    register_binary(ctx, "attr", Type::Markdown, Type::String, Type::String);

    // (markdown, string, string) -> markdown
    register_ternary(
        ctx,
        "set_attr",
        Type::Markdown,
        Type::String,
        Type::String,
        Type::Markdown,
    );

    // (string, string, string) -> markdown
    register_ternary(ctx, "to_link", Type::String, Type::String, Type::String, Type::Markdown);
    register_ternary(
        ctx,
        "to_image",
        Type::String,
        Type::String,
        Type::String,
        Type::Markdown,
    );

    // Markdown attribute functions
    register_unary(ctx, "get_title", Type::Markdown, Type::String);
    register_unary(ctx, "get_url", Type::Markdown, Type::String);

    // Other markdown functions
    register_nullary(ctx, "to_hr", Type::Markdown);
    register_unary(ctx, "to_md_name", Type::Markdown, Type::String);
    register_unary(ctx, "to_md_text", Type::Markdown, Type::String);

    // to_md_table_cell: (a, number, number) -> markdown
    let a = ctx.fresh_var();
    register_ternary(
        ctx,
        "to_md_table_cell",
        Type::Var(a),
        Type::Number,
        Type::Number,
        Type::Markdown,
    );

    // (markdown, bool) -> markdown
    register_binary(ctx, "set_check", Type::Markdown, Type::Bool, Type::Markdown);
    register_binary(ctx, "set_list_ordered", Type::Markdown, Type::Bool, Type::Markdown);

    // (markdown, string) -> markdown
    register_binary(ctx, "set_code_block_lang", Type::Markdown, Type::String, Type::Markdown);
    register_binary(ctx, "set_ref", Type::Markdown, Type::String, Type::Markdown);
}

/// Variable/symbol management functions
fn register_variable(ctx: &mut InferenceContext) {
    let a = ctx.fresh_var();
    register_nullary(ctx, "all_symbols", Type::array(Type::Var(a)));
    register_unary(ctx, "get_variable", Type::String, Type::String);
    register_binary(ctx, "set_variable", Type::String, Type::String, Type::None);
    register_unary(ctx, "intern", Type::String, Type::Symbol);
}

/// Debug/control functions
fn register_debug(ctx: &mut InferenceContext) {
    register_nullary(ctx, "is_debug_mode", Type::Bool);
    register_nullary(ctx, "breakpoint", Type::None);

    let a = ctx.fresh_var();
    register_binary(ctx, "assert", Type::Var(a), Type::Var(a), Type::Var(a));
}

/// File I/O functions
fn register_file_io(ctx: &mut InferenceContext) {
    register_unary(ctx, "read_file", Type::String, Type::String);
}
