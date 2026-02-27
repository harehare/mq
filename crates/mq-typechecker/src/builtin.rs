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

    // Addition: string + number -> string (coercion)
    register_binary(ctx, "+", Type::String, Type::Number, Type::String);
    register_binary(ctx, "add", Type::String, Type::Number, Type::String);

    // Addition: [a] + [a] -> [a] (array concatenation)
    for name in ["+", "add"] {
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

    // Addition: string + any -> string (dynamic coercion fallback)
    // mq uses `+` for string concatenation with any type (e.g., `"text" + value`
    // where `value` is type-guarded at runtime via `is_string`, `is_bool`, etc.)
    for name in ["+", "add"] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::String, Type::Var(a), Type::String);
    }

    // Subtraction: (number, number) -> number
    register_binary(ctx, "-", Type::Number, Type::Number, Type::Number);
    register_binary(ctx, "sub", Type::Number, Type::Number, Type::Number);

    // Multiplication: number * number -> number
    register_binary(ctx, "*", Type::Number, Type::Number, Type::Number);
    register_binary(ctx, "mul", Type::Number, Type::Number, Type::Number);

    // Multiplication: [a] * number -> [a] (array repetition)
    for name in ["*", "mul"] {
        let a = ctx.fresh_var();
        register_binary(
            ctx,
            name,
            Type::array(Type::Var(a)),
            Type::Number,
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

    // repeat: (string, number) -> string (string repetition)
    register_binary(ctx, "repeat", Type::String, Type::Number, Type::String);

    // repeat: (a, number) -> [a] (general repetition)
    let a = ctx.fresh_var();
    register_binary(ctx, "repeat", Type::Var(a), Type::Number, Type::array(Type::Var(a)));

    // slice: (string, number, number) -> string
    register_ternary(ctx, "slice", Type::String, Type::Number, Type::Number, Type::String);

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

    // in: ([a], a) -> bool
    let a = ctx.fresh_var();
    register_binary(ctx, "in", Type::array(Type::Var(a)), Type::Var(a), Type::Bool);

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

/// Utility functions: coalesce
fn register_utility(ctx: &mut InferenceContext) {
    let a = ctx.fresh_var();
    register_binary(ctx, "coalesce", Type::Var(a), Type::Var(a), Type::Var(a));
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
    register_binary(ctx, "assert", Type::Var(a), Type::Var(a), Type::Var(a));
}

/// File I/O functions
fn register_file_io(ctx: &mut InferenceContext) {
    register_unary(ctx, "read_file", Type::String, Type::String);
}
