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
    register_type_checks(ctx);
    register_collection(ctx);
    register_datetime(ctx);
    register_io(ctx);
    register_utility(ctx);
    register_markdown(ctx);
    register_variable(ctx);
    register_debug(ctx);
    register_file_io(ctx);
}

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

/// Registers None propagation overloads: `(none) -> none` for multiple unary functions.
fn register_none_propagation_unary(ctx: &mut InferenceContext, names: &[&str]) {
    for name in names {
        register_unary(ctx, name, Type::None, Type::None);
    }
}

/// Registers None propagation overloads: `(none, none) -> none` for multiple binary functions.
fn register_none_propagation_binary(ctx: &mut InferenceContext, names: &[&str]) {
    for name in names {
        register_binary(ctx, name, Type::None, Type::None, Type::None);
    }
}

/// Arithmetic operators: +, -, *, /, %, ^, add, sub, mul, div, mod, pow
fn register_arithmetic(ctx: &mut InferenceContext) {
    // Addition: number + number -> number
    register_binary(ctx, "+", Type::Number, Type::Number, Type::Number);
    register_binary(ctx, "add", Type::Number, Type::Number, Type::Number);

    // Addition: string + string -> string
    register_binary(ctx, "+", Type::String, Type::String, Type::String);
    register_binary(ctx, "add", Type::String, Type::String, Type::String);

    for name in ["+", "add"] {
        // Addition: string + number -> string (coercion)
        register_binary(ctx, name, Type::String, Type::Number, Type::String);
        register_binary(ctx, name, Type::Number, Type::String, Type::String);

        // Addition: [a] + [a] -> [a] (array concatenation)
        let a = ctx.fresh_var();
        register_binary(
            ctx,
            name,
            Type::array(Type::Var(a)),
            Type::array(Type::Var(a)),
            Type::array(Type::Var(a)),
        );
    }

    // Addition: markdown + markdown -> markdown
    register_binary(ctx, "+", Type::Markdown, Type::Markdown, Type::Markdown);
    register_binary(ctx, "add", Type::Markdown, Type::Markdown, Type::Markdown);

    // Addition: [a] + a -> [a] (array element append)
    for name in ["+", "add"] {
        let a = ctx.fresh_var();
        register_binary(
            ctx,
            name,
            Type::array(Type::Var(a)),
            Type::Var(a),
            Type::array(Type::Var(a)),
        );
    }

    // Addition: markdown + string -> markdown
    register_binary(ctx, "+", Type::Markdown, Type::String, Type::Markdown);
    register_binary(ctx, "add", Type::Markdown, Type::String, Type::Markdown);

    // Subtraction: (number, number) -> number
    register_binary(ctx, "-", Type::Number, Type::Number, Type::Number);
    register_binary(ctx, "sub", Type::Number, Type::Number, Type::Number);

    // Multiplication: number * number -> number
    // [a] * number -> [a] (array repetition)
    for name in ["*", "mul"] {
        register_binary(ctx, name, Type::Number, Type::Number, Type::Number);
        register_binary(ctx, name, Type::String, Type::Number, Type::String);
        let a = ctx.fresh_var();
        register_binary(
            ctx,
            name,
            Type::array(Type::Var(a)),
            Type::Var(a),
            Type::array(Type::Var(a)),
        );
    }

    // Division, modulo, power: (number, number) -> number
    register_many(ctx, &["/", "%", "^"], vec![Type::Number, Type::Number], Type::Number);
    register_many(
        ctx,
        &["div", "mod", "pow"],
        vec![Type::Number, Type::Number],
        Type::Number,
    );

    // Bit shift operators: (number, number) -> number
    register_many(
        ctx,
        &["<<", ">>", "shift_left", "shift_right"],
        vec![Type::Number, Type::Number],
        Type::Number,
    );

    // None propagation: (none, none) -> none for all arithmetic operators
    register_none_propagation_binary(
        ctx,
        &[
            "+",
            "-",
            "*",
            "/",
            "%",
            "^",
            "add",
            "sub",
            "mul",
            "div",
            "mod",
            "pow",
            "<<",
            ">>",
            "shift_left",
            "shift_right",
        ],
    );
}

/// Comparison operators: <, >, <=, >=, ==, !=, lt, gt, lte, gte, eq, ne
fn register_comparison(ctx: &mut InferenceContext) {
    // Ordering: supports number, string, symbol, bool
    for name in ["<", ">", "<=", ">=", "lt", "gt", "lte", "gte"] {
        register_binary(ctx, name, Type::Number, Type::Number, Type::Bool);
        register_binary(ctx, name, Type::String, Type::String, Type::Bool);
        register_binary(ctx, name, Type::Symbol, Type::Symbol, Type::Bool);
        register_binary(ctx, name, Type::Bool, Type::Bool, Type::Bool);
    }

    // Equality: forall a. (a, a) -> bool
    for name in ["==", "!=", "eq", "ne"] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::Var(a), Type::Var(a), Type::Bool);
    }

    // Equality: (None, a) -> bool and (a, None) -> bool
    // In mq, comparing None with any type is valid and returns bool
    // (e.g. `len(possibly_none) == 0` where len returns None for None inputs).
    for name in ["==", "!=", "eq", "ne"] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::None, Type::Var(a), Type::Bool);
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::Var(a), Type::None, Type::Bool);
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

    // mq is dynamically typed and uses truthy/falsy semantics for logical operators.
    // `&&` and `||` can accept any types (e.g., `none || "default"`, `is_array(x) && len(x)`).
    for name in ["&&", "||", "and", "or"] {
        let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
        register_binary(ctx, name, Type::Var(a), Type::Var(b), Type::Var(b));
    }

    // Variadic logical: or/and with 3-6 boolean arguments
    for n in 3..=6 {
        let params = vec![Type::Bool; n];
        for name in ["or", "and"] {
            ctx.register_builtin(name, Type::function(params.clone(), Type::Bool));
        }
    }

    // Unary logical: a -> bool
    // `!` uses the same truthy/falsy semantics as `not` in mq,
    // accepting Bool, String, Number, Array, and Dict operands.
    register_unary(ctx, "!", Type::Bool, Type::Bool);
    register_unary(ctx, "!", Type::String, Type::Bool);
    register_unary(ctx, "!", Type::Number, Type::Bool);
    register_unary(ctx, "not", Type::Bool, Type::Bool);
    register_unary(ctx, "not", Type::String, Type::Bool);
    register_unary(ctx, "not", Type::Number, Type::Bool);

    let a = ctx.fresh_var();
    register_unary(ctx, "!", Type::array(Type::Var(a)), Type::Bool);

    let a = ctx.fresh_var();
    register_unary(ctx, "not", Type::array(Type::Var(a)), Type::Bool);

    let k = ctx.fresh_var();
    let v = ctx.fresh_var();
    register_unary(ctx, "!", Type::dict(Type::Var(k), Type::Var(v)), Type::Bool);

    let k = ctx.fresh_var();
    let v = ctx.fresh_var();
    register_unary(ctx, "not", Type::dict(Type::Var(k), Type::Var(v)), Type::Bool);

    // Unary minus: number -> number
    register_unary(ctx, "-", Type::Number, Type::Number);
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

    // None propagation for min/max
    register_none_propagation_binary(ctx, &["min", "max"]);

    // Special number functions
    register_nullary(ctx, "nan", Type::Number);
    register_nullary(ctx, "infinite", Type::Number);
    register_unary(ctx, "is_nan", Type::Number, Type::Bool);
}

/// String functions: downcase, upcase, trim, starts_with, ends_with, etc.
fn register_string(ctx: &mut InferenceContext) {
    // Case conversion and trimming: string -> string
    register_many(
        ctx,
        &["downcase", "upcase", "trim", "ltrim", "rtrim"],
        vec![Type::String],
        Type::String,
    );

    // String search: (string, string) -> bool/number
    register_binary(ctx, "starts_with", Type::String, Type::String, Type::Bool);
    register_binary(ctx, "ends_with", Type::String, Type::String, Type::Bool);
    register_binary(ctx, "index", Type::String, Type::String, Type::Number);
    register_binary(ctx, "rindex", Type::String, Type::String, Type::Number);

    // String manipulation
    register_ternary(ctx, "replace", Type::String, Type::String, Type::String, Type::String);
    // Generic fallback for replace: (a, string, string) -> a
    // Handles dynamically typed code where the first argument type is runtime-guarded
    let a = ctx.fresh_var();
    register_ternary(ctx, "replace", Type::Var(a), Type::String, Type::String, Type::Var(a));
    register_ternary(ctx, "gsub", Type::String, Type::String, Type::String, Type::String);
    register_binary(ctx, "split", Type::String, Type::String, Type::array(Type::String));
    register_binary(ctx, "join", Type::array(Type::String), Type::String, Type::String);

    // contains: (string, string) -> bool
    register_binary(ctx, "contains", Type::String, Type::String, Type::Bool);

    // Character/codepoint conversion
    register_unary(ctx, "explode", Type::String, Type::array(Type::Number));
    register_unary(ctx, "implode", Type::array(Type::Number), Type::String);

    // String properties
    register_unary(ctx, "utf8bytelen", Type::String, Type::Number);

    // Regular expressions
    register_binary(ctx, "test", Type::String, Type::String, Type::Bool);
    register_binary(
        ctx,
        "regex_match",
        Type::String,
        Type::String,
        Type::array(Type::String),
    );
    register_binary(ctx, "is_regex_match", Type::String, Type::String, Type::Bool);

    // Encoding functions
    register_many(
        ctx,
        &["base64", "base64d", "base64url", "base64urld", "url_encode"],
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

    // None propagation for string functions
    register_none_propagation_unary(
        ctx,
        &[
            "downcase",
            "upcase",
            "trim",
            "ltrim",
            "rtrim",
            "base64",
            "base64d",
            "base64url",
            "base64urld",
            "url_encode",
            "utf8bytelen",
        ],
    );
    // gsub/replace: (none, string, string) -> none
    register_ternary(ctx, "gsub", Type::None, Type::String, Type::String, Type::None);
    register_ternary(ctx, "replace", Type::None, Type::String, Type::String, Type::None);

    // slugify: (string, string) -> string
    register_unary(ctx, "slugify", Type::String, Type::String);
    // slugify: (string) -> string
    register_binary(ctx, "slugify", Type::String, Type::String, Type::String);

    // md5: a -> string
    let a = ctx.fresh_var();
    register_unary(ctx, "md5", Type::Var(a), Type::String);

    // sha256: a -> string
    let a = ctx.fresh_var();
    register_unary(ctx, "sha256", Type::Var(a), Type::String);
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

    // flatten: [a] -> [a] (identity for already-flat arrays)
    let a = ctx.fresh_var();
    register_unary(ctx, "flatten", Type::array(Type::Var(a)), Type::array(Type::Var(a)));

    // flatten: {k: v} -> {k: v} (identity/passthrough for dicts)
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(
        ctx,
        "flatten",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::dict(Type::Var(k), Type::Var(v)),
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

    // slice: ([a], number) -> [a]  (open-ended slice, e.g. arr[1:])
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "slice",
        Type::array(Type::Var(a)),
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

    // range: (number) -> [number], (number, number) -> [number], (number, number, number) -> [number]
    register_unary(ctx, "range", Type::Number, Type::array(Type::Number));
    register_binary(ctx, "range", Type::Number, Type::Number, Type::array(Type::Number));
    register_ternary(
        ctx,
        "range",
        Type::Number,
        Type::Number,
        Type::Number,
        Type::array(Type::Number),
    );

    // .. : (number, number) -> [number]  — binary infix range operator
    register_binary(ctx, "..", Type::Number, Type::Number, Type::array(Type::Number));
    // None propagation for ".."
    register_binary(ctx, "..", Type::None, Type::None, Type::None);

    // repeat: (string, number) -> string (string repetition)
    register_binary(ctx, "repeat", Type::String, Type::Number, Type::String);

    // repeat: (a, number) -> [a] (general repetition)
    let a = ctx.fresh_var();
    register_binary(ctx, "repeat", Type::Var(a), Type::Number, Type::array(Type::Var(a)));

    // slice: (string, number, number) -> string
    register_ternary(ctx, "slice", Type::String, Type::Number, Type::Number, Type::String);

    // slice: (string, number) -> string  (open-ended slice, e.g. s[1:])
    register_binary(ctx, "slice", Type::String, Type::Number, Type::String);

    // contains: ([a], a) -> bool
    let a = ctx.fresh_var();
    register_binary(ctx, "contains", Type::array(Type::Var(a)), Type::Var(a), Type::Bool);

    // contains: ({k: v}, k) -> bool (dict key membership)
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "contains",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::Var(k),
        Type::Bool,
    );

    // None propagation for array functions
    register_none_propagation_unary(ctx, &["reverse", "sort", "uniq", "compact", "flatten", "len"]);
    // slice: (none, number, number) -> none
    register_ternary(ctx, "slice", Type::None, Type::Number, Type::Number, Type::None);
    // slice: (none, number) -> none  (open-ended slice)
    register_binary(ctx, "slice", Type::None, Type::Number, Type::None);

    // percentile: ([a], number) -> number
    let a = ctx.fresh_var();
    register_binary(ctx, "percentile", Type::array(Type::Var(a)), Type::Number, Type::Number);
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

    // Generic fallback: keys/values/entries accept any type (for dynamically typed code
    // where the argument type is determined by runtime guards like `is_dict`)
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(ctx, "keys", Type::Var(a), Type::array(Type::Var(b)));

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

    // set: ([a], number, a) -> [a]  (array index assignment)
    let a = ctx.fresh_var();
    register_ternary(
        ctx,
        "set",
        Type::array(Type::Var(a)),
        Type::Number,
        Type::Var(a),
        Type::array(Type::Var(a)),
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

    // dict: () -> {k: v} (empty dict constructor)
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_nullary(ctx, "dict", Type::dict(Type::Var(k), Type::Var(v)));

    // dict: (k, v) -> {k: v}
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "dict",
        Type::Var(k),
        Type::Var(v),
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // dict: ([a]) -> {k: v} (construct dict from entries array)
    let (a, k, v) = (ctx.fresh_var(), ctx.fresh_var(), ctx.fresh_var());
    register_unary(
        ctx,
        "dict",
        Type::array(Type::Var(a)),
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // dict: ({k: v}) -> {k: v} (identity/passthrough for dicts)
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(
        ctx,
        "dict",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // len: ({k: v}) -> number
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(ctx, "len", Type::dict(Type::Var(k), Type::Var(v)), Type::Number);

    // get: ([a], number) -> a (array element access)
    let a = ctx.fresh_var();
    register_binary(ctx, "get", Type::array(Type::Var(a)), Type::Number, Type::Var(a));

    // get: (a, b) -> c (generic fallback for dynamically typed access)
    let (a, b, c) = (ctx.fresh_var(), ctx.fresh_var(), ctx.fresh_var());
    register_binary(ctx, "get", Type::Var(a), Type::Var(b), Type::Var(c));

    // get: (a, b, c) -> d (chained access, e.g., get(dict, key1)[key2])
    let (a, b, c, d) = (ctx.fresh_var(), ctx.fresh_var(), ctx.fresh_var(), ctx.fresh_var());
    register_ternary(ctx, "get", Type::Var(a), Type::Var(b), Type::Var(c), Type::Var(d));

    // None propagation for dict functions
    register_none_propagation_unary(ctx, &["keys", "values", "entries"]);
}

/// Type conversion functions: to_number, to_string, to_array, type
fn register_type_conversion(ctx: &mut InferenceContext) {
    let a = ctx.fresh_var();
    register_unary(ctx, "to_number", Type::Var(a), Type::Number);

    let a = ctx.fresh_var();
    register_unary(ctx, "to_string", Type::Var(a), Type::String);

    let a = ctx.fresh_var();
    register_unary(ctx, "to_array", Type::Var(a), Type::array(Type::Var(a)));

    let a = ctx.fresh_var();
    register_unary(ctx, "type", Type::Var(a), Type::String);
}

/// Type check functions: is_none, is_array, is_dict, is_string, is_number, is_bool, is_empty
fn register_type_checks(ctx: &mut InferenceContext) {
    // Type predicate functions: (a) -> bool
    for name in ["is_none", "is_array", "is_dict", "is_string", "is_number", "is_bool"] {
        let a = ctx.fresh_var();
        register_unary(ctx, name, Type::Var(a), Type::Bool);
    }

    // is_empty: (string) -> bool, ([a]) -> bool, ({k: v}) -> bool, (a) -> bool
    register_unary(ctx, "is_empty", Type::String, Type::Bool);
    let a = ctx.fresh_var();
    register_unary(ctx, "is_empty", Type::array(Type::Var(a)), Type::Bool);
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(ctx, "is_empty", Type::dict(Type::Var(k), Type::Var(v)), Type::Bool);
    // Generic fallback for dynamically typed values (e.g., via selectors/index access)
    let a = ctx.fresh_var();
    register_unary(ctx, "is_empty", Type::Var(a), Type::Bool);
}

/// Collection functions: first, last, map
fn register_collection(ctx: &mut InferenceContext) {
    // first: ([a]) -> a
    let a = ctx.fresh_var();
    register_unary(ctx, "first", Type::array(Type::Var(a)), Type::Var(a));

    // first: (string) -> string (first character)
    register_unary(ctx, "first", Type::String, Type::String);

    // last: ([a]) -> a
    let a = ctx.fresh_var();
    register_unary(ctx, "last", Type::array(Type::Var(a)), Type::Var(a));

    // last: (string) -> string (last character)
    register_unary(ctx, "last", Type::String, Type::String);

    // map: ([a], (a) -> b) -> [b]
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "map",
        Type::array(Type::Var(a)),
        Type::function(vec![Type::Var(a)], Type::Var(b)),
        Type::array(Type::Var(b)),
    );

    // map: ({k: v}, (v) -> b) -> {k: b} (dict map)
    let (k, v, b) = (ctx.fresh_var(), ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "map",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::function(vec![Type::Var(v)], Type::Var(b)),
        Type::dict(Type::Var(k), Type::Var(b)),
    );

    // Generic fallback: map(a, (a) -> b) -> [b]
    // Handles dynamically typed code where the collection type is runtime-guarded
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "map",
        Type::Var(a),
        Type::function(vec![Type::Var(a)], Type::Var(b)),
        Type::array(Type::Var(b)),
    );

    // filter: ([a], (a) -> bool) -> [a]
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "filter",
        Type::array(Type::Var(a)),
        Type::function(vec![Type::Var(a)], Type::Bool),
        Type::array(Type::Var(a)),
    );

    // Generic fallback: filter(a, (a) -> bool) -> [a]
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "filter",
        Type::Var(a),
        Type::function(vec![Type::Var(a)], Type::Bool),
        Type::array(Type::Var(a)),
    );
    // None propagation: filter(none, (none) -> a) -> none
    // The lambda return type is irrelevant for None propagation since it's never called
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "filter",
        Type::None,
        Type::function(vec![Type::None], Type::Var(a)),
        Type::None,
    );

    // fold: ([a], b, (b, a) -> b) -> b
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_ternary(
        ctx,
        "fold",
        Type::array(Type::Var(a)),
        Type::Var(b),
        Type::function(vec![Type::Var(b), Type::Var(a)], Type::Var(b)),
        Type::Var(b),
    );

    // sort_by: ([a], (a) -> b) -> [a]
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "sort_by",
        Type::array(Type::Var(a)),
        Type::function(vec![Type::Var(a)], Type::Var(b)),
        Type::array(Type::Var(a)),
    );

    // flat_map: ([a], (a) -> [b]) -> [b]
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "flat_map",
        Type::array(Type::Var(a)),
        Type::function(vec![Type::Var(a)], Type::array(Type::Var(b))),
        Type::array(Type::Var(b)),
    );

    register_binary(
        ctx,
        "flat_map",
        Type::None,
        Type::function(vec![Type::Var(a)], Type::array(Type::Var(b))),
        Type::None,
    );

    // in: ([a], a) -> bool
    let a = ctx.fresh_var();
    register_binary(ctx, "in", Type::array(Type::Var(a)), Type::Var(a), Type::Bool);
    register_binary(ctx, "in", Type::String, Type::String, Type::Bool);
    register_binary(
        ctx,
        "in",
        Type::array(Type::Var(a)),
        Type::array(Type::Var(a)),
        Type::Bool,
    );

    // bsearch: ([a], a) -> number
    let a = ctx.fresh_var();
    register_binary(ctx, "bsearch", Type::array(Type::Var(a)), Type::Var(a), Type::Number);

    // None propagation
    register_none_propagation_unary(ctx, &["first", "last"]);
}

/// Date/time functions: now, from_date, to_date
fn register_datetime(ctx: &mut InferenceContext) {
    register_nullary(ctx, "now", Type::Number);
    register_unary(ctx, "from_date", Type::String, Type::Number);
    register_binary(ctx, "to_date", Type::Number, Type::String, Type::String);
}

/// I/O and control flow functions: print, stderr, error, halt, input
fn register_io(ctx: &mut InferenceContext) {
    // print/stderr: a -> a (side effect), also (a, b) -> a for format strings
    let a = ctx.fresh_var();
    register_unary(ctx, "print", Type::Var(a), Type::Var(a));
    let a = ctx.fresh_var();
    register_unary(ctx, "stderr", Type::Var(a), Type::Var(a));
    // stderr with 2 args (e.g., format string with interpolation)
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(ctx, "stderr", Type::Var(a), Type::Var(b), Type::Var(a));

    // error/halt never return, so use a fresh type variable (acts as bottom/never type)
    // This allows error() in if/else branches without type conflicts
    let a = ctx.fresh_var();
    register_unary(ctx, "error", Type::String, Type::Var(a));
    let a = ctx.fresh_var();
    register_unary(ctx, "halt", Type::Number, Type::Var(a));
    register_nullary(ctx, "input", Type::String);
}

/// Utility functions: coalesce, convert
fn register_utility(ctx: &mut InferenceContext) {
    // coalesce / ?? : (None, a) -> a  — left is None, return right (null-coalescing)
    for name in ["coalesce", "??"] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::None, Type::Var(a), Type::Var(a));
    }
    // coalesce / ?? : (a, a) -> a  — same-type fallback
    for name in ["coalesce", "??"] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::Var(a), Type::Var(a), Type::Var(a));
    }

    // convert: (a, b) -> c  (the @ operator, e.g., "text" @ :html)
    // Registered under both "convert" (for function call syntax) and "@" (for operator syntax)
    // Return type is polymorphic because it depends on the runtime format symbol
    for name in ["convert", "@"] {
        let (a, b, c) = (ctx.fresh_var(), ctx.fresh_var(), ctx.fresh_var());
        register_binary(ctx, name, Type::Var(a), Type::Var(b), Type::Var(c));
    }
}

/// Markdown-specific functions
fn register_markdown(ctx: &mut InferenceContext) {
    // Markdown type check functions: (markdown) -> bool
    for name in [
        "is_h",
        "is_h1",
        "is_h2",
        "is_h3",
        "is_h4",
        "is_h5",
        "is_h6",
        "is_p",
        "is_code",
        "is_code_inline",
        "is_code_block",
        "is_em",
        "is_strong",
        "is_link",
        "is_image",
        "is_list",
        "is_list_item",
        "is_table",
        "is_table_row",
        "is_table_cell",
        "is_blockquote",
        "is_hr",
        "is_html",
        "is_text",
        "is_softbreak",
        "is_hardbreak",
        "is_task_list_item",
        "is_footnote",
        "is_footnote_ref",
        "is_strikethrough",
        "is_math",
        "is_math_inline",
        "is_toml",
        "is_yaml",
    ] {
        register_unary(ctx, name, Type::Markdown, Type::Bool);
    }

    // is_h_level: (markdown, number) -> bool
    register_binary(ctx, "is_h_level", Type::Markdown, Type::Number, Type::Bool);

    // Markdown type check functions also accept any type (dynamic usage)
    for name in [
        "is_h",
        "is_h1",
        "is_h2",
        "is_h3",
        "is_h4",
        "is_h5",
        "is_h6",
        "is_p",
        "is_code",
        "is_code_inline",
        "is_code_block",
        "is_em",
        "is_strong",
        "is_link",
        "is_image",
        "is_list",
        "is_list_item",
        "is_table",
        "is_table_row",
        "is_table_cell",
        "is_blockquote",
        "is_hr",
        "is_html",
        "is_text",
        "is_softbreak",
        "is_hardbreak",
        "is_task_list_item",
        "is_footnote",
        "is_footnote_ref",
        "is_strikethrough",
        "is_math",
        "is_math_inline",
        "is_toml",
        "is_yaml",
    ] {
        let a = ctx.fresh_var();
        register_unary(ctx, name, Type::Var(a), Type::Bool);
    }
    {
        let a = ctx.fresh_var();
        register_binary(ctx, "is_h_level", Type::Var(a), Type::Number, Type::Bool);
    }

    // a -> markdown
    let a = ctx.fresh_var();
    register_unary(ctx, "to_markdown", Type::Var(a), Type::Markdown);
    let a = ctx.fresh_var();
    register_unary(ctx, "to_mdx", Type::Var(a), Type::Markdown);

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
        ],
        vec![Type::Markdown],
        Type::Markdown,
    );

    // (markdown, number) -> markdown
    let a = ctx.fresh_var();
    register_binary(ctx, "to_h", Type::Var(a), Type::Number, Type::Markdown);
    let a = ctx.fresh_var();
    register_binary(ctx, "to_md_list", Type::Var(a), Type::Number, Type::Markdown);

    // (markdown, string) -> markdown/string
    let a = ctx.fresh_var();
    register_binary(ctx, "to_code", Type::Var(a), Type::String, Type::Markdown);
    register_binary(ctx, "attr", Type::Markdown, Type::String, Type::String);
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "attr",
        Type::array(Type::Var(a)),
        Type::String,
        Type::array(Type::Var(a)),
    );

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

    // None propagation for markdown -> string functions
    register_none_propagation_unary(ctx, &["to_text", "to_html", "to_markdown_string"]);
}

/// Variable/symbol management functions
fn register_variable(ctx: &mut InferenceContext) {
    register_nullary(ctx, "all_symbols", Type::array(Type::Symbol));
    register_unary(ctx, "get_variable", Type::String, Type::String);
    register_binary(ctx, "set_variable", Type::String, Type::String, Type::None);
    register_unary(ctx, "intern", Type::String, Type::Symbol);
}

/// Debug/control functions
fn register_debug(ctx: &mut InferenceContext) {
    register_nullary(ctx, "is_debug_mode", Type::Bool);
    register_nullary(ctx, "breakpoint", Type::None);

    let a = ctx.fresh_var();
    register_unary(ctx, "assert", Type::Var(a), Type::Var(a));
}

/// File I/O functions
fn register_file_io(ctx: &mut InferenceContext) {
    register_unary(ctx, "read_file", Type::String, Type::String);
}

#[cfg(test)]
mod tests {
    use mq_hir::Hir;
    use rstest::rstest;

    use crate::{TypeChecker, TypeError};

    /// Helper function to create HIR from code
    fn create_hir(code: &str) -> Hir {
        let mut hir = Hir::default();
        // Enable builtins to test builtin function types
        hir.builtin.disabled = false;
        hir.add_builtin(); // Add builtin functions to HIR
        hir.add_code(None, code);
        hir
    }

    /// Helper function to run type checker
    fn check_types(code: &str) -> Vec<TypeError> {
        let hir = create_hir(code);
        let mut checker = TypeChecker::new();
        checker.check(&hir)
    }

    // Mathematical Functions

    #[rstest]
    #[case::abs("abs(42)", true)]
    #[case::abs_negative("abs(-10)", true)]
    #[case::ceil("ceil(3.14)", true)]
    #[case::floor("floor(3.14)", true)]
    #[case::round("round(3.14)", true)]
    #[case::trunc("trunc(3.14)", true)]
    #[case::abs_string("abs(\"hello\")", false)] // Should fail: wrong type
    fn test_unary_math_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::add_numbers("1 + 2")]
    #[case::add_number_string("1 + \"string\"")]
    #[case::add_strings("\"hello\" + \"world\"")]
    #[case::add_string_number("\"value: \" + 42")] // String + number coercion
    #[case::add_arrays("[1, 2] + [3, 4]")] // Array concatenation
    #[case::add_mixed("1 + \"world\"")] // Number + string coercion
    #[case::sub("10 - 5")]
    #[case::mul("3 * 4")]
    #[case::mul_array_repeat("[1, 2] * 3")] // Array repetition
    #[case::div("10 / 2")]
    #[case::mod_op("10 % 3")]
    #[case::pow("2 ^ 8")]
    fn test_arithmetic_operators(#[case] code: &str) {
        let result = check_types(code);
        assert!(result.is_empty(), "Code: {}\nResult: {:?}", code, result);
    }

    #[rstest]
    #[case::lt("5 < 10", true)]
    #[case::gt("10 > 5", true)]
    #[case::lte("5 <= 10", true)]
    #[case::gte("10 >= 5", true)]
    #[case::lt_string("\"a\" < \"b\"", true)] // String comparison
    #[case::gt_string("\"z\" > \"a\"", true)] // String comparison
    #[case::lt_bool("true < false", true)] // Bool comparison (false < true)
    #[case::lt_mixed("\"a\" < 1", false)] // Should fail: mixed types
    fn test_comparison_operators(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::eq_numbers("1 == 1", true)]
    #[case::eq_strings("\"a\" == \"b\"", true)]
    #[case::ne_numbers("1 != 2", true)]
    #[case::ne_strings("\"a\" != \"b\"", true)]
    fn test_equality_operators(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::min_numbers("min(1, 2)", true)]
    #[case::max_numbers("max(1, 2)", true)]
    #[case::min_strings("min(\"a\", \"b\")", true)]
    #[case::max_strings("max(\"a\", \"b\")", true)]
    fn test_min_max(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::and_op("true && false", true)]
    #[case::or_op("true || false", true)]
    #[case::not_op("!true", true)]
    #[case::bang_op("!false", true)]
    #[case::and_number("1 && 2", true)] // mq supports truthy/falsy semantics
    fn test_logical_operators(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::nan("nan()", true)]
    #[case::infinite("infinite()", true)]
    #[case::is_nan("is_nan(1.0)", true)]
    fn test_special_number_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::downcase("downcase(\"HELLO\")", true)]
    #[case::upcase("upcase(\"hello\")", true)]
    #[case::trim("trim(\"  hello  \")", true)]
    #[case::downcase_number("downcase(42)", false)] // Should fail: wrong type
    #[case::rtrim("rtrim(\"  hello  \")", true)]
    #[case::rindex("rindex(\"hello world hello\", \"hello\")", true)]
    #[case::capture("capture(\"hello 42\", \"(?P<word>\\\\w+)\")", true)]
    #[case::is_regex_match("is_regex_match(\"hello123\", \"[0-9]+\")", true)]
    #[case::base64url("base64url(\"hello\")", true)]
    #[case::base64urld("base64urld(\"aGVsbG8=\")", true)]
    #[case::ltrim_number("ltrim(42)", false)]
    #[case::rtrim_number("rtrim(42)", false)]
    fn test_string_case_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::starts_with("starts_with(\"hello\", \"he\")", true)]
    #[case::ends_with("ends_with(\"hello\", \"lo\")", true)]
    #[case::index("index(\"hello\", \"ll\")", true)]
    #[case::rindex("rindex(\"hello\", \"l\")", true)]
    fn test_string_search_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::replace("replace(\"hello\", \"l\", \"r\")", true)]
    #[case::gsub("gsub(\"hello\", \"l\", \"r\")", true)]
    #[case::split("split(\"a,b,c\", \",\")", true)]
    #[case::join("join([\"a\", \"b\"], \",\")", true)]
    fn test_string_manipulation_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::explode("explode(\"hello\")", true)]
    #[case::implode("implode([104, 101, 108, 108, 111])", true)]
    #[case::utf8bytelen("utf8bytelen(\"hello\")", true)]
    fn test_string_codepoint_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::regex_match("regex_match(\"hello123\", \"[0-9]+\")", true)]
    #[case::base64("base64(\"hello\")", true)]
    #[case::base64d("base64d(\"aGVsbG8=\")", true)]
    #[case::url_encode("url_encode(\"hello world\")", true)]
    fn test_string_encoding_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Array Functions

    #[rstest]
    #[case::flatten("flatten([[1, 2], [3, 4]])", true)]
    #[case::reverse("reverse([1, 2, 3])", true)]
    #[case::sort("sort([3, 1, 2])", true)]
    #[case::uniq("uniq([1, 2, 2, 3])", true)]
    #[case::compact("compact([1, none, 2])", true)]
    fn test_array_manipulation_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::len_array("len([1, 2, 3])", true)]
    #[case::len_string("len(\"hello\")", true)]
    #[case::slice("slice([1, 2, 3, 4], 1, 3)", true)]
    #[case::insert("insert([1, 3], 1, 2)", true)]
    fn test_array_access_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::range_two_args("range(1, 5)", true)]
    #[case::range_three_args("range(1, 10, 2)", true)]
    #[case::repeat("repeat(\"x\", 3)", true)]
    fn test_array_creation_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Dictionary Functions

    #[rstest]
    #[case::keys("keys({\"a\": 1, \"b\": 2})", true)]
    #[case::values("values({\"a\": 1, \"b\": 2})", true)]
    #[case::entries("entries({\"a\": 1, \"b\": 2})", true)]
    fn test_dict_query_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::get("get({\"a\": 1}, \"a\")", true)]
    #[case::get_array("get([1, 2, 3], 0)", true)]
    #[case::get_generic_binary("let d = dict() | get(d, \"key\")", true)]
    #[case::get_generic_ternary("let d = dict() | get(d, \"key\")[\"name\"]", true)]
    #[case::set("set({\"a\": 1}, \"b\", 2)", true)]
    #[case::del("del({\"a\": 1, \"b\": 2}, \"a\")", true)]
    #[case::update("update({\"a\": 1}, {\"b\": 2})", true)]
    fn test_dict_manipulation_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Type Conversion Functions

    #[rstest]
    #[case::to_number("to_number(\"42\")", true)]
    #[case::to_string("to_string(42)", true)]
    #[case::to_array("to_array(42)", true)]
    #[case::type_of("type(42)", true)]
    fn test_type_conversion_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Date/Time Functions

    #[rstest]
    #[case::now("now()", true)]
    #[case::from_date("from_date(\"2024-01-01\")", true)]
    #[case::to_date("to_date(1704067200000, \"%Y-%m-%d\")", true)]
    fn test_datetime_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // I/O And Utility Functions

    #[rstest]
    #[case::print("print(42)", true)]
    #[case::stderr("stderr(\"error\")", true)]
    #[case::input("input()", true)]
    fn test_io_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Complex Expressions With Builtins

    #[rstest]
    #[case::chained_string_ops("upcase(trim(\"  hello  \"))", true)]
    #[case::math_expression("abs(min(-5, -10) + max(3, 7))", true)]
    #[case::array_pipeline("len(reverse(sort([3, 1, 2])))", true)]
    #[case::mixed_operations("to_string(len(split(\"a,b,c\", \",\")))", true)]
    fn test_complex_builtin_expressions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Pipe Type Propagation

    #[rstest]
    #[case::string_to_upcase("\"hello\" | upcase", true)]
    #[case::string_to_trim("\"  hello  \" | trim", true)]
    #[case::number_to_abs("-42 | abs", true)]
    #[case::string_to_len("\"hello\" | len", true)]
    #[case::chained_pipes("\"  hello  \" | trim | upcase", true)]
    #[case::chained_string_to_len("\"hello\" | upcase | len", true)]
    #[case::chained_split_to_len("\"hello\" | split(\",\") | len", true)]
    #[case::chained_array_reverse_first("[1,2,3] | reverse | first", true)]
    #[case::number_to_upcase("42 | upcase", false)] // Number piped to string function
    fn test_pipe_type_propagation(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Type Error Cases
    #[rstest]
    #[case::add_bool_number("true + 1", false)] // bool + number is invalid
    #[case::add_bool_string("true + \"s\"", false)] // bool + string is invalid
    #[case::sub_strings("\"a\" - \"b\"", false)] // strings cannot be subtracted
    #[case::sub_string_number("\"a\" - 1", false)] // string - number is invalid
    #[case::mul_string_number("\"a\" * 3", true)] // string * number is invalid
    #[case::div_strings("\"a\" / \"b\"", false)] // strings cannot be divided
    #[case::mod_strings("\"a\" % \"b\"", false)] // strings cannot use modulo
    #[case::sub_bool_bool("true - false", false)] // booleans cannot be subtracted
    #[case::mul_bool_number("true * 2", false)] // bool * number is invalid
    #[case::div_bool_number("true / 2", false)] // bool / number is invalid
    #[case::mod_bool_number("true % 2", false)] // bool % number is invalid
    #[case::mod_number_string("10 % \"2\"", false)] // number % string is invalid
    fn test_arithmetic_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::lt_string_number("\"a\" < 1", false)] // mixed types
    #[case::gt_number_string("1 > \"a\"", false)] // mixed types
    #[case::lte_bool_number("true <= 1", false)] // mixed types
    #[case::gte_string_bool("\"a\" >= true", false)] // mixed types
    #[case::lt_number_bool("1 < true", false)] // mixed types
    #[case::gt_bool_string("true > \"x\"", false)] // mixed types
    #[case::lte_string_number("\"a\" <= 1", false)] // mixed types
    #[case::gte_number_bool("1 >= false", false)] // mixed types
    fn test_comparison_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::not_number("!42", true)] // number is truthy/falsy → bool
    #[case::not_string("!\"hello\"", true)] // string is truthy/falsy → bool
    #[case::not_array("![1, 2, 3]", true)] // array is truthy/falsy → bool
    fn test_logical_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::abs_string("abs(\"not a number\")", false)] // wrong argument type
    #[case::downcase_number("downcase(42)", false)] // wrong argument type
    #[case::ceil_string("ceil(\"hello\")", false)] // wrong argument type
    #[case::floor_bool("floor(true)", false)] // wrong argument type
    #[case::len_no_args("len()", false)] // missing argument
    #[case::split_numbers("split(1, 2)", false)] // wrong argument types
    #[case::join_numbers("join(1, 2)", false)] // wrong argument types
    #[case::starts_with_numbers("starts_with(1, 2)", false)] // wrong argument types
    #[case::round_string("round(\"hello\")", false)] // wrong argument type
    #[case::trunc_string("trunc(\"hello\")", false)] // wrong argument type
    #[case::abs_bool("abs(true)", false)] // wrong argument type
    #[case::floor_string("floor(\"hello\")", false)] // wrong argument type
    #[case::ceil_bool("ceil(false)", false)] // wrong argument type
    #[case::upcase_number("upcase(42)", false)] // wrong argument type
    #[case::trim_number("trim(42)", false)] // wrong argument type
    #[case::ltrim_bool("ltrim(true)", false)] // wrong argument type
    #[case::rtrim_bool("rtrim(true)", false)] // wrong argument type
    #[case::explode_number("explode(42)", false)] // expects string
    #[case::implode_string("implode(\"hello\")", false)] // expects array of numbers
    #[case::split_wrong_sep("split(\"hello\", 42)", false)] // separator must be string
    #[case::join_wrong_sep("join([\"a\", \"b\"], 42)", false)] // separator must be string
    #[case::replace_wrong_sep("replace(\"hello\", 42, \"r\")", false)] // wrong separator type
    #[case::gsub_wrong_first("gsub(42, \"l\", \"r\")", false)] // expects string first arg
    #[case::starts_with_num_sep("starts_with(\"hello\", 42)", false)] // expects string prefix
    #[case::ends_with_num_sep("ends_with(\"hello\", 42)", false)] // expects string suffix
    #[case::flatten_string("flatten(\"hello\")", false)] // expects array
    #[case::sort_string("sort(\"hello\")", false)] // expects array
    #[case::uniq_string("uniq(\"hello\")", false)] // expects array
    #[case::compact_string("compact(\"hello\")", false)] // expects array
    #[case::reverse_number("reverse(42)", false)] // expects array or string
    #[case::values_number("values(42)", false)] // expects dict
    #[case::values_string("values(\"hello\")", false)] // expects dict
    #[case::entries_string("entries(\"hello\")", false)] // expects dict
    #[case::range_string("range(\"a\", 5)", false)] // expects number
    #[case::repeat_bool("repeat(\"x\", true)", false)] // second arg must be number
    fn test_function_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::number_to_upcase("42 | upcase", false)] // number piped to string function
    #[case::number_to_trim("42 | trim", false)] // number piped to string function
    #[case::string_to_abs("\"hello\" | abs", false)] // string piped to number function
    #[case::string_to_ceil("\"hello\" | ceil", false)] // string piped to number function
    #[case::number_to_downcase("42 | downcase", false)] // number piped to string function
    #[case::bool_to_upcase("true | upcase", false)] // bool piped to string function
    #[case::bool_to_trim("true | trim", false)] // bool piped to string function
    #[case::bool_to_abs("true | abs", false)] // bool piped to number function
    #[case::string_to_floor("\"hello\" | floor", false)] // string piped to number function
    #[case::string_to_round("\"hello\" | round", false)] // string piped to number function
    #[case::string_to_trunc("\"hello\" | trunc", false)] // string piped to number function
    #[case::number_to_len("42 | len", false)] // number piped to len (expects array or string)
    #[case::bool_to_len("true | len", false)] // bool piped to len
    #[case::number_to_sort("42 | sort", false)] // number piped to sort (expects array)
    #[case::number_to_reverse("42 | reverse", false)] // number piped to reverse
    #[case::number_to_flatten("42 | flatten", false)] // number piped to flatten
    #[case::number_to_uniq("42 | uniq", false)] // number piped to uniq
    fn test_pipe_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    #[rstest]
    #[case::piped_join("[\"a\", \"b\"] | join(\",\")", true)] // array piped as first arg to join
    #[case::piped_split("\"a,b,c\" | split(\",\")", true)] // string piped as first arg to split
    #[case::piped_starts_with("\"hello\" | starts_with(\"he\")", true)]
    #[case::piped_ends_with("\"hello\" | ends_with(\"lo\")", true)]
    #[case::piped_index("\"hello\" | index(\"ll\")", true)]
    #[case::piped_replace("\"hello\" | replace(\"l\", \"r\")", true)]
    #[case::piped_gsub("\"hello\" | gsub(\"l\", \"r\")", true)]
    #[case::piped_gsub_variable_arg("def slugify(s, separator = \"-\"): s | gsub(\"[^a-z0-9]+\", separator) end", true)] // regression: default param used in gsub should not produce false error
    #[case::piped_slice("[1, 2, 3, 4] | slice(1, 3)", true)]
    #[case::piped_repeat("\"x\" | repeat(3)", true)]
    #[case::piped_join_wrong_type("42 | join(\",\")", false)] // number piped to join (expects array)
    #[case::piped_split_wrong_type("42 | split(\",\")", false)] // number piped to split (expects string)
    fn test_piped_function_calls(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Collection Functions

    #[rstest]
    #[case::first_array("first([1, 2, 3])", true)]
    #[case::last_array("last([1, 2, 3])", true)]
    #[case::first_string("first(\"hello\")", true)]
    #[case::last_string("last(\"hello\")", true)]
    #[case::contains_string("contains(\"hello world\", \"world\")", true)]
    #[case::contains_array("contains([1, 2, 3], 2)", true)]
    #[case::contains_dict("contains({\"a\": 1, \"b\": 2}, \"a\")", true)]
    #[case::in_array("in([1, 2, 3], 1)", true)]
    #[case::in_string("in(\"hello world\", \"world\")", true)]
    #[case::in_sub_array("in([1, 2, 3, 4], [2, 3])", true)]
    #[case::in_return_plus_number("in([1, 2, 3], 1) + 1", false)] // in returns bool; bool + number is invalid
    #[case::contains_return_plus_number("contains(\"hello\", \"he\") + 1", false)] // contains returns bool; bool + number is invalid
    #[case::bsearch("bsearch([1, 2, 3, 4], 2)", true)]
    #[case::bsearch_error("bsearch([1, 2, 3, 4], \"2\")", false)]
    fn test_collection_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Higher-Order Collection Functions

    #[rstest]
    #[case::map_array("map([1, 2, 3], fn(x): x + 1;)", true)]
    #[case::filter_array("filter([1, 2, 3], fn(x): x > 1;)", true)]
    #[case::fold_array("fold([1, 2, 3], 0, fn(acc, x): acc + x;)", true)]
    #[case::sort_by_array("sort_by([1, 2, 3], fn(x): x;)", true)]
    #[case::flat_map_array("flat_map([1, 2, 3], fn(x): [x];)", true)]
    fn test_higher_order_collection_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // None Propagation for Higher-Order Functions

    #[rstest]
    #[case::filter_none_identity("filter(None, fn(x): x;)", true)]
    #[case::filter_none_array_return("filter(None, fn(x): [x[0], x[1]];)", true)]
    #[case::flat_map_none("flat_map(None, fn(x): [x];)", true)]
    #[case::map_none_identity("map(None, fn(x): x;)", true)]
    fn test_none_propagation_higher_order(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Type Check Functions

    #[rstest]
    #[case::is_none("is_none(42)", true)]
    #[case::is_array("is_array([1,2,3])", true)]
    #[case::is_dict("is_dict({\"a\": 1})", true)]
    #[case::is_string("is_string(\"hello\")", true)]
    #[case::is_number("is_number(42)", true)]
    #[case::is_bool("is_bool(true)", true)]
    #[case::is_empty_string("is_empty(\"\")", true)]
    #[case::is_empty_array("is_empty([])", true)]
    fn test_type_check_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Utility Functions

    #[rstest]
    #[case::coalesce("coalesce(1, 2)", true)]
    #[case::error_func("error(\"message\")", true)]
    #[case::halt_func("halt(1)", true)]
    #[case::all_symbols("all_symbols()", true)]
    #[case::get_variable("get_variable(\"key\")", true)]
    #[case::set_variable("set_variable(\"key\", \"value\")", true)]
    #[case::intern("intern(\"symbol\")", true)]
    #[case::is_debug_mode("is_debug_mode()", true)]
    #[case::breakpoint("breakpoint()", true)]
    #[case::assert_func("assert(42)", true)]
    #[case::assert_func_bool("assert(true)", true)]
    #[case::negate_func("negate(5)", true)]
    #[case::pow_func("pow(2, 8)", true)]
    #[case::convert_at_operator("\"hello\" @ :text", true)]
    #[case::convert_func("convert(\"hello\", :text)", true)]
    fn test_utility_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Numeric Bit-Shift Functions

    #[rstest]
    #[case::shift_left_numbers("shift_left(3, 1)", true)]
    #[case::shift_right_numbers("shift_right(8, 2)", true)]
    #[case::shift_left_wrong_second_arg("shift_left(\"hello\", \"str\")", false)]
    #[case::shift_right_wrong_second_arg("shift_right(\"hello\", \"str\")", false)]
    fn test_bitshift_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Markdown Type Check Functions

    #[rstest]
    #[case::is_h("to_markdown(\"# hello\") | is_h", true)]
    #[case::is_p("to_markdown(\"hello\") | is_p", true)]
    #[case::is_code("to_markdown(\"hello\") | is_code", true)]
    #[case::is_code_block("to_markdown(\"hello\") | is_code_block", true)]
    #[case::is_code_inline("to_markdown(\"hello\") | is_code_inline", true)]
    #[case::is_em("to_markdown(\"hello\") | is_em", true)]
    #[case::is_strong("to_markdown(\"hello\") | is_strong", true)]
    #[case::is_link("to_markdown(\"hello\") | is_link", true)]
    #[case::is_image("to_markdown(\"hello\") | is_image", true)]
    #[case::is_list("to_markdown(\"hello\") | is_list", true)]
    #[case::is_list_item("to_markdown(\"hello\") | is_list_item", true)]
    #[case::is_table("to_markdown(\"hello\") | is_table", true)]
    #[case::is_table_row("to_markdown(\"hello\") | is_table_row", true)]
    #[case::is_table_cell("to_markdown(\"hello\") | is_table_cell", true)]
    #[case::is_blockquote("to_markdown(\"hello\") | is_blockquote", true)]
    #[case::is_hr("to_markdown(\"hello\") | is_hr", true)]
    #[case::is_html("to_markdown(\"hello\") | is_html", true)]
    #[case::is_text("to_markdown(\"hello\") | is_text", true)]
    #[case::is_softbreak("to_markdown(\"hello\") | is_softbreak", true)]
    #[case::is_hardbreak("to_markdown(\"hello\") | is_hardbreak", true)]
    #[case::is_task_list_item("to_markdown(\"hello\") | is_task_list_item", true)]
    #[case::is_footnote("to_markdown(\"hello\") | is_footnote", true)]
    #[case::is_footnote_ref("to_markdown(\"hello\") | is_footnote_ref", true)]
    #[case::is_strikethrough("to_markdown(\"hello\") | is_strikethrough", true)]
    #[case::is_math("to_markdown(\"hello\") | is_math", true)]
    #[case::is_math_inline("to_markdown(\"hello\") | is_math_inline", true)]
    #[case::is_toml("to_markdown(\"hello\") | is_toml", true)]
    #[case::is_yaml("to_markdown(\"hello\") | is_yaml", true)]
    #[case::is_h_level("to_markdown(\"# hello\") | is_h_level(1)", true)]
    #[case::is_h_level_wrong_type("is_h_level(42, \"str\")", false)]
    fn test_markdown_type_check_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Markdown Conversion Functions

    #[rstest]
    #[case::to_markdown("to_markdown(\"hello\")", true)]
    #[case::to_text("to_markdown(\"hello\") | to_text", true)]
    #[case::to_html("to_markdown(\"hello\") | to_html", true)]
    #[case::to_markdown_string("to_markdown(\"hello\") | to_markdown_string", true)]
    #[case::to_code("to_markdown(\"hello\") | to_code(\"rust\")", true)]
    #[case::to_code_inline("to_markdown(\"hello\") | to_code_inline", true)]
    #[case::to_h("to_markdown(\"hello\") | to_h(1)", true)]
    #[case::to_hr("to_hr()", true)]
    #[case::to_image("to_image(\"alt\", \"url\", \"title\")", true)]
    #[case::to_link("to_link(\"text\", \"url\", \"title\")", true)]
    #[case::to_md_list("to_markdown(\"hello\") | to_md_list(1)", true)]
    #[case::to_md_name("to_markdown(\"hello\") | to_md_name", true)]
    #[case::to_md_table_cell("to_md_table_cell(\"hello\", 0, 0)", true)]
    #[case::to_md_table_row("to_markdown(\"hello\") | to_md_table_row", true)]
    #[case::to_md_text("to_markdown(\"hello\") | to_md_text", true)]
    #[case::to_math("to_markdown(\"hello\") | to_math", true)]
    #[case::to_math_inline("to_markdown(\"hello\") | to_math_inline", true)]
    #[case::to_mdx("to_mdx(\"hello\")", true)]
    #[case::to_strong("to_markdown(\"hello\") | to_strong", true)]
    #[case::to_em("to_markdown(\"hello\") | to_em", true)]
    fn test_markdown_conversion_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Markdown Manipulation Functions

    #[rstest]
    #[case::attr("to_markdown(\"[link](url)\") | attr(\"href\")", true)]
    #[case::set_attr("to_markdown(\"[link](url)\") | set_attr(\"href\", \"new\")", true)]
    #[case::get_title("to_markdown(\"[link](url)\") | get_title", true)]
    #[case::get_url("to_markdown(\"[link](url)\") | get_url", true)]
    #[case::set_check("to_markdown(\"- [ ] task\") | set_check(true)", true)]
    #[case::set_list_ordered("to_markdown(\"- item\") | set_list_ordered(false)", true)]
    #[case::set_code_block_lang("to_markdown(\"```\\ncode\\n```\") | set_code_block_lang(\"rust\")", true)]
    #[case::set_ref("to_markdown(\"[link][ref]\") | set_ref(\"ref\")", true)]
    fn test_markdown_manipulation_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Dict/Array Constructor Functions

    #[rstest]
    #[case::dict_nullary("dict()", true)]
    #[case::array_func("array(42)", true)]
    fn test_constructor_functions(#[case] code: &str, #[case] should_succeed: bool) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "Code: {}\nResult: {:?}",
            code,
            result
        );
    }

    // Higher-Order Function Lambda Type Errors

    #[rstest]
    #[case::map_lambda_sub_error("map([1, 2, 3], fn(x): x - \"str\";)", false, "map lambda body type error")]
    #[case::map_lambda_bool_op("map([1, 2, 3], fn(x): x - true;)", false, "map lambda bool operand")]
    #[case::filter_lambda_sub_error("filter([1, 2, 3], fn(x): x - \"str\";)", false, "filter lambda body type error")]
    #[case::fold_lambda_type_error(
        "fold([1, 2, 3], 0, fn(acc, x): acc - \"str\";)",
        false,
        "fold lambda body type error"
    )]
    #[case::map_lambda_valid("map([1, 2, 3], fn(x): x + 1;)", true, "map lambda valid")]
    #[case::filter_lambda_valid("filter([1, 2, 3], fn(x): x > 1;)", true, "filter lambda valid")]
    #[case::fold_lambda_valid("fold([1, 2, 3], 0, fn(acc, x): acc + x;)", true, "fold lambda valid")]
    fn test_higher_order_lambda_type_errors(
        #[case] code: &str,
        #[case] should_succeed: bool,
        #[case] description: &str,
    ) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "{}: Code='{}' Result={:?}",
            description,
            code,
            result
        );
    }

    // Bit-Shift Operator Type Errors

    #[rstest]
    #[case::shift_left_bool_rhs("3 << true", false, "shift left with bool right operand")]
    #[case::shift_right_bool_rhs("8 >> true", false, "shift right with bool right operand")]
    #[case::shift_left_string_rhs("3 << \"1\"", false, "shift left with string right operand")]
    #[case::shift_right_string_rhs("8 >> \"2\"", false, "shift right with string right operand")]
    #[case::shift_left_valid("3 << 1", true, "shift left with numbers is valid")]
    #[case::shift_right_valid("8 >> 2", true, "shift right with numbers is valid")]
    fn test_bitshift_op_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "{}: Code='{}' Result={:?}",
            description,
            code,
            result
        );
    }

    // Markdown Function Type Errors

    #[rstest]
    #[case::to_markdown_number("to_markdown(42)", true, "to_markdown accepts any type")]
    #[case::to_markdown_bool("to_markdown(true)", true, "to_markdown accepts any type")]
    #[case::to_h_wrong_level("to_markdown(\"hello\") | to_h(\"one\")", false, "to_h expects number level")]
    #[case::to_md_list_wrong_type("to_markdown(\"hello\") | to_md_list(\"x\")", false, "to_md_list expects number")]
    #[case::to_md_table_cell_wrong_col(
        "to_md_table_cell(\"hello\", \"col\", 0)",
        false,
        "to_md_table_cell expects number col"
    )]
    #[case::to_md_table_cell_wrong_row(
        "to_md_table_cell(\"hello\", 0, \"row\")",
        false,
        "to_md_table_cell expects number row"
    )]
    #[case::set_check_wrong_type("to_markdown(\"- [ ] task\") | set_check(42)", false, "set_check expects bool")]
    #[case::set_list_ordered_wrong_type(
        "to_markdown(\"- item\") | set_list_ordered(42)",
        false,
        "set_list_ordered expects bool"
    )]
    #[case::set_code_block_lang_wrong_type(
        "to_markdown(\"hello\") | set_code_block_lang(42)",
        false,
        "set_code_block_lang expects string"
    )]
    #[case::to_markdown_valid("to_markdown(\"# hello\")", true, "to_markdown with string is valid")]
    #[case::to_h_valid("to_markdown(\"hello\") | to_h(1)", true, "to_h with number is valid")]
    fn test_markdown_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "{}: Code='{}' Result={:?}",
            description,
            code,
            result
        );
    }

    // Piped Wrong-Type Comprehensive Cases

    #[rstest]
    #[case::number_to_explode("42 | explode", false, "number piped to explode (expects string)")]
    #[case::number_to_downcase("42 | downcase", false, "number piped to downcase")]
    #[case::number_to_ltrim("42 | ltrim", false, "number piped to ltrim")]
    #[case::number_to_rtrim("42 | rtrim", false, "number piped to rtrim")]
    #[case::bool_to_floor("true | floor", false, "bool piped to floor")]
    #[case::bool_to_round("true | round", false, "bool piped to round")]
    #[case::bool_to_trunc("true | trunc", false, "bool piped to trunc")]
    #[case::bool_to_sort("true | sort", false, "bool piped to sort")]
    #[case::string_to_flatten("\"hello\" | flatten", false, "string piped to flatten")]
    #[case::string_to_sort("\"hello\" | sort", false, "string piped to sort")]
    #[case::string_to_uniq("\"hello\" | uniq", false, "string piped to uniq")]
    #[case::string_to_compact("\"hello\" | compact", false, "string piped to compact")]
    fn test_pipe_wrong_type_comprehensive(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
        let result = check_types(code);
        assert_eq!(
            result.is_empty(),
            should_succeed,
            "{}: Code='{}' Result={:?}",
            description,
            code,
            result
        );
    }
}
