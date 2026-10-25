//! Builtin function type signatures for the mq type system.
//!
//! This module registers type signatures for all builtin functions and operators.
//! Builtins are organized by category for maintainability. To add a new builtin,
//! find the appropriate category function and add a registration call.

use crate::infer::InferenceContext;
use crate::kind_set::KindSet;
use crate::types::Type;
use mq_markdown::NodeKind;

/// The name of `partial`, whose signature is derived per call instead of looked up.
pub(crate) const PARTIAL: &str = "partial";

mod refine;

pub(crate) use refine::{fallback_signature, refine_signature, refines};

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
    register_net(ctx);
    register_bytes(ctx);
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

    // Multiplication: number * string -> string (repetition, either order)
    for name in ["*", "mul"] {
        register_binary(ctx, name, Type::Number, Type::String, Type::String);
    }

    // Addition: markdown + markdown -> markdown
    register_binary(ctx, "+", Type::markdown(), Type::markdown(), Type::markdown());
    register_binary(ctx, "add", Type::markdown(), Type::markdown(), Type::markdown());

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
    register_binary(ctx, "+", Type::markdown(), Type::String, Type::markdown());
    register_binary(ctx, "add", Type::markdown(), Type::String, Type::markdown());

    // Subtraction: (number, number) -> number
    register_binary(ctx, "-", Type::Number, Type::Number, Type::Number);
    register_binary(ctx, "sub", Type::Number, Type::Number, Type::Number);

    // Multiplication: number * number -> number
    for name in ["*", "mul"] {
        register_binary(ctx, name, Type::Number, Type::Number, Type::Number);
        register_binary(ctx, name, Type::String, Type::Number, Type::String);
    }

    // Division, modulo, power: (number, number) -> number
    register_many(ctx, &["/", "%"], vec![Type::Number, Type::Number], Type::Number);
    register_many(
        ctx,
        &["div", "mod", "pow", "atan2", "hypot"],
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

    // A string loses characters from one end, and an array gains a value at one end.
    for name in ["shift_left", "shift_right"] {
        register_binary(ctx, name, Type::String, Type::Number, Type::String);
    }
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "shift_left",
        Type::array(Type::Var(a)),
        Type::Var(b),
        Type::array(Type::union(vec![Type::Var(a), Type::Var(b)])),
    );
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "shift_right",
        Type::Var(b),
        Type::array(Type::Var(a)),
        Type::array(Type::union(vec![Type::Var(a), Type::Var(b)])),
    );

    // A value joins the front of an array, and a number repeats an array.
    for name in ["+", "add"] {
        let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
        register_binary(
            ctx,
            name,
            Type::Var(a),
            Type::array(Type::Var(b)),
            Type::array(Type::union(vec![Type::Var(a), Type::Var(b)])),
        );
    }
    for name in ["*", "mul"] {
        let a = ctx.fresh_var();
        register_binary(
            ctx,
            name,
            Type::Number,
            Type::array(Type::Var(a)),
            Type::array(Type::Var(a)),
        );
        let a = ctx.fresh_var();
        register_binary(
            ctx,
            name,
            Type::array(Type::Var(a)),
            Type::Number,
            Type::array(Type::Var(a)),
        );
    }

    // None propagation: (none, none) -> none for all arithmetic operators
    register_none_propagation_binary(
        ctx,
        &[
            "+",
            "-",
            "*",
            "/",
            "%",
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

/// What a string function takes: a string, or a markdown node as its text.
fn text() -> Type {
    Type::union(vec![Type::String, Type::markdown()])
}

/// Comparison operators: <, >, <=, >=, ==, !=, lt, gt, lte, gte, eq, ne
fn register_comparison(ctx: &mut InferenceContext) {
    // Ordering: supports number, string, symbol, bool, bytes
    for name in ["<", ">", "<=", ">=", "lt", "gt", "lte", "gte"] {
        register_binary(ctx, name, Type::Number, Type::Number, Type::Bool);
        register_binary(ctx, name, Type::String, Type::String, Type::Bool);
        register_binary(ctx, name, Type::Symbol, Type::Symbol, Type::Bool);
        register_binary(ctx, name, Type::Bool, Type::Bool, Type::Bool);
        register_binary(ctx, name, Type::Bytes, Type::Bytes, Type::Bool);
        register_binary(ctx, name, Type::None, Type::None, Type::Bool);
        // Arrays and dicts are ordered by their contents.
        let a = ctx.fresh_var();
        register_binary(
            ctx,
            name,
            Type::array(Type::Var(a)),
            Type::array(Type::Var(a)),
            Type::Bool,
        );
        let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
        register_binary(
            ctx,
            name,
            Type::dict(Type::Var(k), Type::Var(v)),
            Type::dict(Type::Var(k), Type::Var(v)),
            Type::Bool,
        );
    }

    // Equality: forall a b. (a, b) -> bool
    // Values of different types are never equal, which is not an error.
    for name in ["==", "!=", "eq", "ne"] {
        let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
        register_binary(ctx, name, Type::Var(a), Type::Var(b), Type::Bool);
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
        &[
            "abs", "ceil", "floor", "round", "trunc", "ln", "log10", "log", "sqrt", "exp", "sin", "cos", "tan", "asin",
            "acos", "atan", "sinh", "cosh", "tanh", "log2", "cbrt",
        ],
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
    // is_nan: any value -> bool (only a number can be nan)
    let a = ctx.fresh_var();
    register_unary(ctx, "is_nan", Type::Var(a), Type::Bool);

    // Human-readable byte size formatting: number -> string
    register_unary(ctx, "human_bytes", Type::Number, Type::String);
    register_unary(ctx, "human_size", Type::Number, Type::String);

    // Randomness. `seed` is optional; the trailing overloads accept it.
    register_nullary(ctx, "rand", Type::Number);
    register_unary(ctx, "rand", Type::Number, Type::Number);
    register_binary(ctx, "rand_int", Type::Number, Type::Number, Type::Number);
    register_ternary(ctx, "rand_int", Type::Number, Type::Number, Type::Number, Type::Number);
}

/// String functions: downcase, upcase, trim, starts_with, ends_with, etc.
fn register_string(ctx: &mut InferenceContext) {
    // Case conversion and trimming: string -> string
    register_many(
        ctx,
        &[
            "downcase",
            "upcase",
            "ascii_downcase",
            "ascii_upcase",
            "trim",
            "ltrim",
            "rtrim",
        ],
        vec![Type::String],
        Type::String,
    );
    register_many(
        ctx,
        &[
            "downcase",
            "upcase",
            "ascii_downcase",
            "ascii_upcase",
            "trim",
            "ltrim",
            "rtrim",
        ],
        vec![Type::markdown()],
        Type::markdown(),
    );

    // String search: (string, string) -> bool/number
    register_binary(ctx, "starts_with", text(), Type::String, Type::Bool);
    register_binary(ctx, "ends_with", text(), Type::String, Type::Bool);
    register_binary(ctx, "index", text(), Type::String, Type::Number);
    register_binary(ctx, "rindex", text(), Type::String, Type::Number);
    register_binary(ctx, "indices", text(), Type::String, Type::array(Type::Number));
    register_unary(ctx, "token_count", text(), Type::Number);
    register_binary(ctx, "token_count", text(), Type::String, Type::Number);
    // A none input is searched like an empty string.
    register_binary(ctx, "starts_with", Type::None, Type::String, Type::Bool);
    register_binary(ctx, "ends_with", Type::None, Type::String, Type::Bool);
    register_binary(ctx, "index", Type::None, Type::String, Type::Number);
    register_binary(ctx, "rindex", Type::None, Type::String, Type::Number);
    register_unary(ctx, "token_count", Type::None, Type::Number);
    register_binary(ctx, "token_count", Type::None, Type::String, Type::Number);

    // String manipulation
    register_ternary(ctx, "replace", Type::String, Type::String, Type::String, Type::String);
    // Generic fallback for replace: (a, string, string) -> a
    // Handles dynamically typed code where the first argument type is runtime-guarded
    let a = ctx.fresh_var();
    register_ternary(ctx, "replace", Type::Var(a), Type::String, Type::String, Type::Var(a));
    register_ternary(ctx, "gsub", Type::String, Type::String, Type::String, Type::String);
    // gsub also accepts a markdown node at runtime.
    register_ternary(
        ctx,
        "gsub",
        Type::markdown(),
        Type::String,
        Type::String,
        Type::markdown(),
    );
    register_binary(ctx, "split", text(), Type::String, Type::array(Type::String));
    register_binary(ctx, "split", Type::None, Type::String, Type::array(Type::String));
    register_binary(ctx, "split", Type::Bytes, Type::Bytes, Type::array(Type::Bytes));
    // An array is split at the elements equal to the separator.
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "split",
        Type::array(Type::Var(a)),
        Type::Var(b),
        Type::array(Type::array(Type::Var(a))),
    );
    // split_records and extract_urls also accept a markdown node (its text) and none (no records).
    let split_record = Type::array(closed_record(&[
        ("text", Type::String),
        ("index", Type::Number),
        ("start_byte", Type::Number),
        ("end_byte", Type::Number),
        ("terminator", Type::union(vec![Type::String, Type::None])),
    ]));
    for input in [Type::String, Type::markdown(), Type::None] {
        register_binary(ctx, "split_records", input, Type::String, split_record.clone());
    }
    let url_record = Type::array(closed_record(&[
        ("url", Type::String),
        ("start_byte", Type::Number),
        ("end_byte", Type::Number),
        ("kind", Type::String),
    ]));
    for input in [Type::String, Type::markdown(), Type::None] {
        register_unary(ctx, "extract_urls", input, url_record.clone());
    }

    // casefold, graphemes and grapheme_len take a string or a node's text; anything else gives
    // none, an empty array or 0.
    for input in [Type::String, Type::markdown(), Type::None] {
        register_unary(ctx, "grapheme_len", input.clone(), Type::Number);
        register_unary(ctx, "graphemes", input, Type::array(Type::String));
    }
    register_unary(ctx, "casefold", Type::String, Type::String);
    register_unary(ctx, "casefold", Type::markdown(), Type::markdown());
    register_unary(ctx, "casefold", Type::None, Type::None);
    // unicode_normalize: (string | markdown | none, form) -> same
    register_binary(ctx, "unicode_normalize", Type::String, Type::String, Type::String);
    register_binary(
        ctx,
        "unicode_normalize",
        Type::markdown(),
        Type::String,
        Type::markdown(),
    );
    register_binary(ctx, "unicode_normalize", Type::None, Type::String, Type::None);

    // css family: (html, selector) -> [string]; css_attr also takes the attribute name and
    // yields none for elements without it.
    register_binary(ctx, "css", Type::String, Type::String, Type::array(Type::String));
    register_binary(ctx, "css_text", Type::String, Type::String, Type::array(Type::String));
    register_ternary(
        ctx,
        "css_attr",
        Type::String,
        Type::String,
        Type::String,
        Type::array(Type::union(vec![Type::String, Type::None])),
    );

    // word_wrap: (string, number) -> string
    register_binary(ctx, "word_wrap", Type::String, Type::Number, Type::String);
    // truncate: (string, number, string) -> string
    register_ternary(ctx, "truncate", Type::String, Type::Number, Type::String, Type::String);
    // join: ([a], string) -> string. The elements are converted to strings.
    let a = ctx.fresh_var();
    register_binary(ctx, "join", Type::array(Type::Var(a)), Type::String, Type::String);

    // contains: (string, string) -> bool
    register_binary(ctx, "contains", Type::String, Type::String, Type::Bool);
    // A markdown node is searched through its text.
    register_binary(ctx, "contains", Type::markdown(), Type::String, Type::Bool);

    // A string takes text at a position, and anything else is written out as text first.
    let a = ctx.fresh_var();
    register_ternary(ctx, "insert", Type::String, Type::Number, Type::Var(a), Type::String);

    // Character/codepoint conversion
    register_unary(ctx, "explode", text(), Type::array(Type::Number));
    register_unary(ctx, "implode", Type::array(Type::Number), Type::String);

    // String properties
    register_unary(ctx, "utf8bytelen", text(), Type::Number);

    // Regular expressions
    register_binary(ctx, "test", text(), Type::String, Type::Bool);
    register_binary(ctx, "regex_match", text(), Type::String, Type::array(Type::String));
    register_binary(ctx, "is_regex_match", text(), Type::String, Type::Bool);
    register_binary(ctx, "is_not_regex_match", text(), Type::String, Type::Bool);
    register_unary(ctx, "regex_escape", Type::String, Type::String);
    // A none input matches nothing.
    register_binary(ctx, "test", Type::None, Type::String, Type::Bool);
    register_binary(ctx, "is_regex_match", Type::None, Type::String, Type::Bool);
    register_binary(ctx, "is_not_regex_match", Type::None, Type::String, Type::Bool);
    register_binary(ctx, "regex_match", Type::None, Type::String, Type::array(Type::String));

    // Encoding functions
    register_many(
        ctx,
        &[
            "base64",
            "base64d",
            "base64url",
            "base64urld",
            "url_encode",
            "url_decode",
            "html_escape",
            "html_unescape",
            "strip_tags",
            "sanitize_html",
        ],
        vec![Type::String],
        Type::String,
    );
    register_many(
        ctx,
        &[
            "base64",
            "base64d",
            "base64url",
            "base64urld",
            "url_encode",
            "url_decode",
            "html_escape",
            "html_unescape",
            "strip_tags",
            "sanitize_html",
        ],
        vec![Type::markdown()],
        Type::markdown(),
    );
    register_unary(ctx, "base64d_bytes", Type::String, Type::Bytes);
    register_unary(ctx, "base64urld_bytes", Type::String, Type::Bytes);

    // markdown_escape: (string, context) -> string
    register_binary(ctx, "markdown_escape", Type::String, Type::String, Type::String);

    // markdown_escape: (markdown, context) -> markdown
    register_binary(ctx, "markdown_escape", Type::markdown(), Type::String, Type::markdown());

    // Capture: (string, pattern) -> {k: v}
    let k = ctx.fresh_var();
    let v = ctx.fresh_var();
    register_binary(
        ctx,
        "capture",
        text(),
        Type::String,
        Type::dict(Type::Var(k), Type::Var(v)),
    );
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "capture",
        Type::None,
        Type::String,
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // Scan: (string, pattern) -> [a]
    // Element type is left polymorphic since it depends on whether the pattern has
    // capture groups (string per match) or not (array of groups per match).
    let a = ctx.fresh_var();
    register_binary(ctx, "scan", text(), Type::String, Type::array(Type::Var(a)));
    let a = ctx.fresh_var();
    register_binary(ctx, "scan", Type::None, Type::String, Type::array(Type::Var(a)));

    // Numbers, arrays and the like are written out as text.
    for name in ["url_encode", "url_decode"] {
        let a = ctx.fresh_var();
        register_unary(ctx, name, Type::Var(a), Type::String);
    }

    // None propagation for string functions
    register_none_propagation_unary(
        ctx,
        &[
            "downcase",
            "upcase",
            "ascii_downcase",
            "ascii_upcase",
            "trim",
            "ltrim",
            "rtrim",
            "base64",
            "base64d",
            "base64url",
            "base64urld",
            "base64d_bytes",
            "base64urld_bytes",
            "url_encode",
            "url_decode",
            "html_escape",
            "html_unescape",
            "strip_tags",
            "sanitize_html",
            "utf8bytelen",
        ],
    );
    // gsub/replace: (none, string, string) -> none
    register_ternary(ctx, "gsub", Type::None, Type::String, Type::String, Type::None);
    register_ternary(ctx, "replace", Type::None, Type::String, Type::String, Type::None);

    // word_wrap/truncate: none propagation
    register_binary(ctx, "word_wrap", Type::None, Type::Number, Type::None);
    register_ternary(ctx, "truncate", Type::None, Type::Number, Type::String, Type::None);

    // markdown_escape: none propagation
    register_binary(ctx, "markdown_escape", Type::None, Type::String, Type::None);

    // lines: (string) -> [string]
    register_unary(ctx, "lines", Type::String, Type::array(Type::String));
    register_unary(ctx, "lines", Type::None, Type::None);

    // unlines: ([string]) -> string
    register_unary(ctx, "unlines", Type::array(Type::String), Type::String);
    register_unary(ctx, "unlines", Type::None, Type::None);

    // common_indent/dedent: (string) -> string
    register_unary(ctx, "common_indent", Type::String, Type::String);
    register_unary(ctx, "common_indent", Type::None, Type::None);
    register_unary(ctx, "dedent", Type::String, Type::String);
    register_unary(ctx, "dedent", Type::None, Type::None);

    // slugify: (string, string) -> string
    register_unary(ctx, "slugify", Type::String, Type::String);
    // slugify: (string) -> string
    register_binary(ctx, "slugify", Type::String, Type::String, Type::String);

    // is_blank: (string) -> bool
    register_unary(ctx, "is_blank", Type::String, Type::Bool);

    // normalize_newlines: (string, string) -> string
    register_binary(ctx, "normalize_newlines", Type::String, Type::String, Type::String);

    // safe_filename: (string) -> string
    register_unary(ctx, "safe_filename", Type::String, Type::String);
    // safe_filename: (string, number) -> string
    register_binary(ctx, "safe_filename", Type::String, Type::Number, Type::String);

    // md5: a -> string
    let a = ctx.fresh_var();
    register_unary(ctx, "md5", Type::Var(a), Type::String);
    register_unary(ctx, "md5", Type::None, Type::None);

    // sha256: a -> string
    let a = ctx.fresh_var();
    register_unary(ctx, "sha256", Type::Var(a), Type::String);
    register_unary(ctx, "sha256", Type::None, Type::None);

    // uuid/uuid_v4/uuid_v7: () -> string
    register_nullary(ctx, "uuid", Type::String);
    register_nullary(ctx, "uuid_v4", Type::String);
    register_nullary(ctx, "uuid_v7", Type::String);

    // random_string: (number, string) -> string. `seed` is optional; the ternary overload
    // accepts it.
    register_binary(ctx, "random_string", Type::Number, Type::String, Type::String);
    register_ternary(
        ctx,
        "random_string",
        Type::Number,
        Type::String,
        Type::Number,
        Type::String,
    );
}

/// A generator yielding the type variable `yielded`.
fn generator_of(yielded: crate::types::TypeVarId) -> Type {
    Type::Generator(Box::new(Type::Var(yielded)))
}

/// The `{value, done}` record `next` and `send` return; `value` is `none` once it is done.
fn step_result(yielded: crate::types::TypeVarId) -> Type {
    Type::record(
        [
            ("value".to_string(), Type::union(vec![Type::Var(yielded), Type::None])),
            ("done".to_string(), Type::Bool),
        ]
        .into_iter()
        .collect(),
        Type::RowEmpty,
    )
}

/// A node of exactly one kind.
fn node_of(kind: NodeKind) -> Type {
    Type::Node(KindSet::of(kind))
}

/// Array functions: flatten, reverse, sort, uniq, compact, len, slice, insert, range, repeat
fn register_array(ctx: &mut InferenceContext) {
    // Polymorphic array -> array functions
    for name in ["reverse", "sort", "sort_natural", "uniq", "compact", "shuffle"] {
        let a = ctx.fresh_var();
        register_unary(ctx, name, Type::array(Type::Var(a)), Type::array(Type::Var(a)));
    }

    // shuffle: ([a], seed: number) -> [a]. `seed` is optional; this overload accepts it.
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "shuffle",
        Type::array(Type::Var(a)),
        Type::Number,
        Type::array(Type::Var(a)),
    );

    // sample: ([a], number) -> [a]
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "sample",
        Type::array(Type::Var(a)),
        Type::Number,
        Type::array(Type::Var(a)),
    );
    // sample: ([a], number, seed: number) -> [a]. `seed` is optional; this overload accepts it.
    let a = ctx.fresh_var();
    register_ternary(
        ctx,
        "sample",
        Type::array(Type::Var(a)),
        Type::Number,
        Type::Number,
        Type::array(Type::Var(a)),
    );

    // token_compress: ([markdown], number) -> [markdown]
    register_binary(
        ctx,
        "token_compress",
        Type::array(Type::markdown()),
        Type::Number,
        Type::array(Type::markdown()),
    );
    // token_compress: ([markdown], number, model: string) -> [markdown]
    register_ternary(
        ctx,
        "token_compress",
        Type::array(Type::markdown()),
        Type::Number,
        Type::String,
        Type::array(Type::markdown()),
    );

    // flatten: [a] -> [a]. The result element type is computed from the argument by
    // `refine_signature`, since flattening is recursive and the elements may be mixed.
    let a = ctx.fresh_var();
    register_unary(ctx, "flatten", Type::array(Type::Var(a)), Type::array(Type::Var(a)));

    // flatten: generator<a> -> generator<a>. A generator is returned unchanged.
    let a = ctx.fresh_var();
    register_unary(ctx, "flatten", generator_of(a), generator_of(a));

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
    register_unary(ctx, "len", text(), Type::Number);

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

    // insert: ([a], number, b) -> [a | b]. Arrays may hold mixed elements.
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_ternary(
        ctx,
        "insert",
        Type::array(Type::Var(a)),
        Type::Number,
        Type::Var(b),
        Type::array(Type::union(vec![Type::Var(a), Type::Var(b)])),
    );

    // An array starts or ends with text when its first or last element does. Any other element
    // does not.
    for name in ["starts_with", "ends_with"] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::array(Type::Var(a)), Type::String, Type::Bool);
    }
    // An array tells where an element is.
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "indices",
        Type::array(Type::Var(a)),
        Type::Var(b),
        Type::array(Type::Number),
    );

    // array: a -> [a]
    let a = ctx.fresh_var();
    register_unary(ctx, "array", Type::Var(a), Type::array(Type::Var(a)));
    // array takes any number of values: array(1, 2, 3) is [1, 2, 3]. Mixed values give a union.
    for count in 2..=5 {
        let vars: Vec<crate::types::TypeVarId> = (0..count).map(|_| ctx.fresh_var()).collect();
        let element = Type::union(vars.iter().map(|v| Type::Var(*v)).collect());
        ctx.register_builtin(
            "array",
            Type::function(vars.iter().map(|v| Type::Var(*v)).collect(), Type::array(element)),
        );
    }

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
    // range: (string, string) -> [string], (string, string, number) -> [string]
    register_binary(ctx, "range", Type::String, Type::String, Type::array(Type::String));
    register_ternary(
        ctx,
        "range",
        Type::String,
        Type::String,
        Type::Number,
        Type::array(Type::String),
    );

    // .. : (number, number) -> [number] (binary infix range operator)
    register_binary(ctx, "..", Type::Number, Type::Number, Type::array(Type::Number));
    // None propagation for ".."
    register_binary(ctx, "..", Type::None, Type::None, Type::None);

    // repeat: (string, number) -> string (string repetition)
    register_binary(ctx, "repeat", Type::String, Type::Number, Type::String);

    // repeat: (a, number) -> [a] (general repetition)
    let a = ctx.fresh_var();
    register_binary(ctx, "repeat", Type::Var(a), Type::Number, Type::array(Type::Var(a)));

    // slice: (string, number, number) -> string
    register_ternary(ctx, "slice", text(), Type::Number, Type::Number, Type::String);

    // slice: (string, number) -> string  (open-ended slice, e.g. s[1:])
    register_binary(ctx, "slice", text(), Type::Number, Type::String);

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
    register_none_propagation_unary(
        ctx,
        &["reverse", "sort", "sort_natural", "uniq", "compact", "flatten", "len"],
    );
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

    // get: (record, string) -> a. The field type is taken from the literal key once the record
    // type is known (see `DeferredAttrCall`); until then, and for a computed key, it is open.
    let (row, result) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "get",
        Type::record(std::collections::BTreeMap::new(), Type::Var(row)),
        Type::String,
        Type::Var(result),
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

    // set: ({k: v}, k, w) -> {k: v | w}  (a dict may hold values of different types)
    let (k, v, w) = (ctx.fresh_var(), ctx.fresh_var(), ctx.fresh_var());
    register_ternary(
        ctx,
        "set",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::Var(k),
        Type::Var(w),
        Type::dict(Type::Var(k), Type::union(vec![Type::Var(v), Type::Var(w)])),
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

    // del: ([a], number) -> [a], (string, number) -> string. Removes the element at an index.
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "del",
        Type::array(Type::Var(a)),
        Type::Number,
        Type::array(Type::Var(a)),
    );
    register_binary(ctx, "del", Type::String, Type::Number, Type::String);

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

    // get: (string, number) -> string (a character), (markdown, number) -> markdown, (none, a) -> none
    register_binary(ctx, "get", Type::String, Type::Number, Type::String);
    register_binary(ctx, "get", Type::markdown(), Type::Number, Type::markdown());
    let key = ctx.fresh_var();
    register_binary(ctx, "get", Type::None, Type::Var(key), Type::None);

    // pick: ({k: v}, [k]) -> {k: v}
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "pick",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::array(Type::Var(k)),
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // omit: ({k: v}, [k]) -> {k: v}
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "omit",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::array(Type::Var(k)),
        Type::dict(Type::Var(k), Type::Var(v)),
    );

    // has: ({k: v}, k) -> bool
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "has",
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::Var(k),
        Type::Bool,
    );

    // has: ([a], a) -> bool
    let a = ctx.fresh_var();
    register_binary(ctx, "has", Type::array(Type::Var(a)), Type::Var(a), Type::Bool);

    // has: (a, b) -> bool (generic fallback for dynamically typed containers)
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(ctx, "has", Type::Var(a), Type::Var(b), Type::Bool);

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
    // bytes -> [number]
    register_unary(ctx, "to_array", Type::Bytes, Type::array(Type::Number));

    // to_boolean: bool -> bool, string -> bool (parses "true"/"false")
    register_unary(ctx, "to_boolean", Type::Bool, Type::Bool);
    register_unary(ctx, "to_boolean", Type::String, Type::Bool);

    // to_bytes: string -> bytes, [number] -> bytes, bytes -> bytes
    register_unary(ctx, "to_bytes", Type::String, Type::Bytes);
    register_unary(ctx, "to_bytes", Type::array(Type::Number), Type::Bytes);
    register_unary(ctx, "to_bytes", Type::Bytes, Type::Bytes);

    let a = ctx.fresh_var();
    register_unary(ctx, "type", Type::Var(a), Type::String);
}

/// Type check functions: is_none, is_array, is_dict, is_string, is_number, is_bool, is_empty
fn register_type_checks(ctx: &mut InferenceContext) {
    // Type predicate functions: (a) -> bool
    for name in [
        "is_none",
        "is_array",
        "is_dict",
        "is_string",
        "is_number",
        "is_bool",
        "is_bytes",
        "is_coroutine",
    ] {
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

    // last: (generator<a>) -> a | none. Consumes the generator.
    let a = ctx.fresh_var();
    register_unary(
        ctx,
        "last",
        generator_of(a),
        Type::union(vec![Type::Var(a), Type::None]),
    );

    // compact: (generator<a>) -> generator<a>. Lazy.
    let a = ctx.fresh_var();
    register_unary(ctx, "compact", generator_of(a), generator_of(a));

    // first: (generator<a>) -> a | none. Consumes one value.
    let a = ctx.fresh_var();
    register_unary(
        ctx,
        "first",
        generator_of(a),
        Type::union(vec![Type::Var(a), Type::None]),
    );

    // map/filter/flat_map/fold over a generator are lazy and give a generator (fold consumes it).
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "map",
        generator_of(a),
        Type::function(vec![Type::Var(a)], Type::Var(b)),
        generator_of(b),
    );
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "filter",
        generator_of(a),
        Type::function(vec![Type::Var(a)], Type::Bool),
        generator_of(a),
    );
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "flat_map",
        generator_of(a),
        Type::function(vec![Type::Var(a)], Type::array(Type::Var(b))),
        generator_of(b),
    );
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_ternary(
        ctx,
        "fold",
        generator_of(a),
        Type::Var(b),
        Type::function(vec![Type::Var(b), Type::Var(a)], Type::Var(b)),
        Type::Var(b),
    );

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

    // map: (string, (string) -> b) -> [b] over the characters, (none, (a) -> b) -> none
    let b = ctx.fresh_var();
    register_binary(
        ctx,
        "map",
        Type::String,
        Type::function(vec![Type::String], Type::Var(b)),
        Type::array(Type::Var(b)),
    );
    let (a, b) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "map",
        Type::None,
        Type::function(vec![Type::Var(a)], Type::Var(b)),
        Type::None,
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

    // filter: (string, (string) -> bool) -> [string] over the characters
    register_binary(
        ctx,
        "filter",
        Type::String,
        Type::function(vec![Type::String], Type::Bool),
        Type::array(Type::String),
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

    // gmtime/localtime: number (unix timestamp) -> [number] (broken-down time array)
    register_unary(ctx, "gmtime", Type::Number, Type::array(Type::Number));
    register_unary(ctx, "localtime", Type::Number, Type::array(Type::Number));

    // mktime: [number] (broken-down time array) -> number (unix timestamp)
    register_unary(ctx, "mktime", Type::array(Type::Number), Type::Number);

    // strftime: (number, string) -> string
    register_binary(ctx, "strftime", Type::Number, Type::String, Type::String);

    // strptime: (string, string) -> number
    register_binary(ctx, "strptime", Type::String, Type::String, Type::Number);

    // date_add: ([number], number, string) -> [number]
    register_ternary(
        ctx,
        "date_add",
        Type::array(Type::Number),
        Type::Number,
        Type::String,
        Type::array(Type::Number),
    );

    // date_diff: ([number], [number], string) -> number
    register_ternary(
        ctx,
        "date_diff",
        Type::array(Type::Number),
        Type::array(Type::Number),
        Type::String,
        Type::Number,
    );

    // date_relative: (number, string) -> number
    register_binary(ctx, "date_relative", Type::Number, Type::String, Type::Number);
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
    register_nullary(ctx, "env", Type::dict(Type::String, Type::String));
    register_unary(ctx, "env", Type::String, Type::String);
}

/// Utility functions: coalesce, convert
fn register_utility(ctx: &mut InferenceContext) {
    // system: (command) -> string, (command, [string]) -> string
    register_unary(ctx, "system", Type::String, Type::String);
    register_binary(ctx, "system", Type::String, Type::array(Type::String), Type::String);

    // partial: (fn, bound args...) -> fn over the remaining parameters. The signature follows the
    // arity of `fn`, so calls are typed by `InferenceContext::partial_signature`. This entry only
    // names the builtin and serves as its documented form.
    let (a, b, r) = (ctx.fresh_var(), ctx.fresh_var(), ctx.fresh_var());
    ctx.register_builtin(
        PARTIAL,
        Type::function(
            vec![
                Type::function(vec![Type::Var(a), Type::Var(b)], Type::Var(r)),
                Type::Var(a),
            ],
            Type::function(vec![Type::Var(b)], Type::Var(r)),
        ),
    );

    // coalesce / ?? : (None, a) -> a (left is None, return right; null-coalescing)
    for name in ["coalesce", "??"] {
        let a = ctx.fresh_var();
        register_binary(ctx, name, Type::None, Type::Var(a), Type::Var(a));
    }
    // coalesce / ?? : (a, a) -> a (same-type fallback)
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
        "is_code",
        "is_em",
        "is_list",
        "is_table_cell",
        "is_html",
        "is_text",
        "is_toml",
        "is_yaml",
        "is_callout",
        "is_table_align",
    ] {
        register_unary(ctx, name, Type::markdown(), Type::Bool);
    }

    // is_h_level: (markdown, number) -> bool
    register_binary(ctx, "is_h_level", Type::markdown(), Type::Number, Type::Bool);

    // Markdown type check functions also accept any type (dynamic usage)
    for name in [
        "is_h",
        "is_h1",
        "is_h2",
        "is_h3",
        "is_h4",
        "is_h5",
        "is_h6",
        "is_code",
        "is_em",
        "is_list",
        "is_table_cell",
        "is_html",
        "is_text",
        "is_toml",
        "is_yaml",
        "is_callout",
        "is_table_align",
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
    register_unary(ctx, "to_markdown", Type::Var(a), Type::array(Type::markdown()));
    let a = ctx.fresh_var();
    register_unary(ctx, "to_mdx", Type::Var(a), Type::array(Type::markdown()));

    // string (HTML) -> array(markdown)
    register_unary(ctx, "from_html", Type::String, Type::array(Type::markdown()));

    // markdown -> string functions
    register_many(
        ctx,
        &["to_markdown_string", "to_text", "to_html"],
        vec![Type::markdown()],
        Type::String,
    );

    // Markdown constructors: any value (a string or a node) -> a node of one fixed kind
    for (name, kind) in [
        ("to_code_inline", NodeKind::CodeInline),
        ("to_strong", NodeKind::Strong),
        ("to_em", NodeKind::Emphasis),
        ("to_blockquote", NodeKind::Blockquote),
        ("to_delete", NodeKind::Delete),
        ("to_math", NodeKind::Math),
        ("to_math_inline", NodeKind::MathInline),
        ("to_md_table_row", NodeKind::TableRow),
    ] {
        let value = ctx.fresh_var();
        register_unary(ctx, name, Type::Var(value), node_of(kind));
    }

    // to_callout: (a, string, string) -> callout. The body is a string or a node.
    let callout_body = ctx.fresh_var();
    register_ternary(
        ctx,
        "to_callout",
        Type::Var(callout_body),
        Type::String,
        Type::String,
        node_of(NodeKind::Callout),
    );

    // to_md_fragment: markdown -> markdown, [a] -> markdown
    register_unary(ctx, "to_md_fragment", Type::markdown(), Type::markdown());
    let a = ctx.fresh_var();
    register_unary(ctx, "to_md_fragment", Type::array(Type::Var(a)), Type::markdown());

    // to_md_table_align: [a] -> markdown
    let a = ctx.fresh_var();
    register_unary(
        ctx,
        "to_md_table_align",
        Type::array(Type::Var(a)),
        node_of(NodeKind::TableAlign),
    );

    // (markdown, number) -> markdown
    let a = ctx.fresh_var();
    // A depth outside 1..=6 gives a heading that no `is_h<n>` matches, so the depth is not narrowed.
    register_binary(ctx, "to_h", Type::Var(a), Type::Number, Type::Node(KindSet::HEADING));
    let a = ctx.fresh_var();
    register_binary(ctx, "to_md_list", Type::Var(a), Type::Number, node_of(NodeKind::List));

    // (markdown, string) -> markdown/string
    let a = ctx.fresh_var();
    register_binary(ctx, "to_code", Type::Var(a), Type::String, node_of(NodeKind::Code));
    // The language is optional: none gives a fence without one.
    let a = ctx.fresh_var();
    register_binary(ctx, "to_code", Type::Var(a), Type::None, node_of(NodeKind::Code));
    // The result depends on the attribute name (`attr(h, "depth")` is a number), which is not part
    // of the type, so it stays open and is fixed by how it is used.
    let attr_ret = ctx.fresh_var();
    register_binary(ctx, "attr", Type::markdown(), Type::String, Type::Var(attr_ret));
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
        Type::markdown(),
        Type::String,
        Type::String,
        Type::markdown(),
    );

    // (markdown, array) -> markdown
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "set_children",
        Type::markdown(),
        Type::array(Type::Var(a)),
        Type::markdown(),
    );

    // (string, string, string) -> markdown
    register_ternary(
        ctx,
        "to_link",
        Type::String,
        Type::String,
        Type::String,
        node_of(NodeKind::Link),
    );
    register_ternary(
        ctx,
        "to_image",
        Type::String,
        Type::String,
        Type::String,
        node_of(NodeKind::Image),
    );

    // Markdown attribute functions
    register_unary(ctx, "get_title", Type::markdown(), Type::String);
    register_unary(ctx, "get_url", Type::markdown(), Type::String);
    // Anything else than a node has no position, which gives `none` at runtime.
    let value = ctx.fresh_var();
    register_unary(
        ctx,
        "get_location",
        Type::Var(value),
        Type::dict(Type::String, Type::Number),
    );

    // Other markdown functions
    register_nullary(ctx, "to_hr", node_of(NodeKind::HorizontalRule));
    register_nullary(ctx, "to_break", node_of(NodeKind::Break));
    // (url, ident, title) -> definition; (value, ident) -> footnote; ident -> footnote reference
    register_ternary(
        ctx,
        "to_definition",
        Type::String,
        Type::String,
        Type::String,
        node_of(NodeKind::Definition),
    );
    let a = ctx.fresh_var();
    register_binary(
        ctx,
        "to_footnote",
        Type::Var(a),
        Type::String,
        node_of(NodeKind::Footnote),
    );
    register_unary(ctx, "to_footnote_ref", Type::String, node_of(NodeKind::FootnoteRef));
    let a = ctx.fresh_var();
    register_unary(ctx, "to_md_html", Type::Var(a), node_of(NodeKind::Html));
    register_unary(ctx, "to_md_name", Type::markdown(), Type::String);
    let value = ctx.fresh_var();
    register_unary(ctx, "to_md_text", Type::Var(value), Type::String);

    // to_md_table_cell: (a, number, number) -> markdown
    let a = ctx.fresh_var();
    register_ternary(
        ctx,
        "to_md_table_cell",
        Type::Var(a),
        Type::Number,
        Type::Number,
        node_of(NodeKind::TableCell),
    );

    // (markdown, bool) -> markdown
    register_binary(ctx, "set_check", Type::markdown(), Type::Bool, Type::markdown());
    // `none` leaves the node unchecked (a plain list item).
    register_binary(ctx, "set_check", Type::markdown(), Type::None, Type::markdown());
    register_binary(ctx, "set_list_ordered", Type::markdown(), Type::Bool, Type::markdown());

    // (markdown, string) -> markdown
    register_binary(
        ctx,
        "set_code_block_lang",
        Type::markdown(),
        Type::String,
        Type::markdown(),
    );
    register_binary(ctx, "set_ref", Type::markdown(), Type::String, Type::markdown());

    // A node takes a new text
    let a = ctx.fresh_var();
    register_binary(ctx, "update", Type::markdown(), Type::Var(a), Type::markdown());

    // Any value is written out as text.
    for name in ["to_markdown_string", "to_text"] {
        let a = ctx.fresh_var();
        register_unary(ctx, name, Type::Var(a), Type::String);
    }
    // Only text is rendered as HTML.
    for input in [Type::String, Type::Symbol] {
        register_unary(ctx, "to_html", input, Type::String);
    }

    // None propagation for markdown -> string functions
    register_none_propagation_unary(ctx, &["to_text", "to_html", "to_markdown_string"]);
}

/// Variable/symbol management functions
fn register_variable(ctx: &mut InferenceContext) {
    register_nullary(ctx, "all_symbols", Type::array(Type::Symbol));
    register_unary(ctx, "intern", Type::String, Type::Symbol);
}

/// Debug/control functions
/// A record type with exactly the given fields.
fn closed_record(fields: &[(&str, Type)]) -> Type {
    Type::record(
        fields.iter().map(|(name, ty)| (name.to_string(), ty.clone())).collect(),
        Type::RowEmpty,
    )
}

fn register_debug(ctx: &mut InferenceContext) {
    register_nullary(ctx, "is_debug_mode", Type::Bool);
    register_nullary(ctx, "breakpoint", Type::None);

    // `next()`/`send()` may take their coroutine from the pipeline or explicitly as the first
    // argument. Coroutines and their `{ value, done }` records are runtime-only VM types for
    // now, so the checker models both sides as dynamic while still resolving the builtin name.
    register_nullary(ctx, "next", Type::Dynamic);
    for name in ["next", "send"] {
        let yielded = ctx.fresh_var();
        register_unary(ctx, name, generator_of(yielded), step_result(yielded));
    }
    let (yielded, sent) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "send",
        generator_of(yielded),
        Type::Var(sent),
        step_result(yielded),
    );
    let yielded = ctx.fresh_var();
    register_unary(ctx, "close", generator_of(yielded), generator_of(yielded));
    let yielded = ctx.fresh_var();
    register_unary(ctx, "status", generator_of(yielded), Type::Symbol);
}

/// File I/O functions
fn register_file_io(ctx: &mut InferenceContext) {
    register_unary(ctx, "read_file", Type::String, Type::String);
    register_unary(ctx, "read_file_bytes", Type::String, Type::Bytes);

    // File handles are runtime-only, like coroutines, so the checker models them as dynamic.
    register_unary(ctx, "open_file", Type::String, Type::Dynamic);
    register_unary(ctx, "read_line", Type::Dynamic, Type::Dynamic);
    register_binary(ctx, "read_bytes", Type::Dynamic, Type::Number, Type::Dynamic);

    register_unary(ctx, "file_exists", Type::String, Type::Bool);
    register_unary(ctx, "file_size", Type::String, Type::Number);

    // file_info: string (path) -> {path, kind, size, modified}
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(ctx, "file_info", Type::String, Type::dict(Type::Var(k), Type::Var(v)));

    // collection: string (dir path) -> [{path, title, frontmatter, content}]
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_unary(
        ctx,
        "collection",
        Type::String,
        Type::array(Type::dict(Type::Var(k), Type::Var(v))),
    );

    // collection: (string, bool) (dir path, respect_gitignore) -> [{path, title, frontmatter, content}]
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_binary(
        ctx,
        "collection",
        Type::String,
        Type::Bool,
        Type::array(Type::dict(Type::Var(k), Type::Var(v))),
    );

    // walk_files: string (root) -> [string] (root-relative paths)
    register_unary(ctx, "walk_files", Type::String, Type::array(Type::String));

    // walk_files: (string, string) (root, pattern) -> [string]
    register_binary(ctx, "walk_files", Type::String, Type::String, Type::array(Type::String));

    // walk_files: (string, string, dict) (root, pattern, options) -> [string]
    let (k, v) = (ctx.fresh_var(), ctx.fresh_var());
    register_ternary(
        ctx,
        "walk_files",
        Type::String,
        Type::String,
        Type::dict(Type::Var(k), Type::Var(v)),
        Type::array(Type::String),
    );

    // Path manipulation: string -> string
    register_many(
        ctx,
        &["basename", "dirname", "extname", "stem"],
        vec![Type::String],
        Type::String,
    );
    register_binary(ctx, "path_join", Type::String, Type::String, Type::String);
    register_binary(ctx, "glob_match", Type::String, Type::String, Type::Bool);

    // write_file: (string, string | bytes) -> none
    register_binary(ctx, "write_file", Type::String, Type::String, Type::None);
    register_binary(ctx, "write_file", Type::String, Type::Bytes, Type::None);

    // mkdir: string -> none
    register_unary(ctx, "mkdir", Type::String, Type::None);

    // embed_images: (markdown[, base_dir]) -> markdown; extract_images: (markdown, dir) -> markdown
    register_unary(ctx, "embed_images", Type::markdown(), Type::markdown());
    register_binary(ctx, "embed_images", Type::markdown(), Type::String, Type::markdown());
    register_binary(ctx, "extract_images", Type::markdown(), Type::String, Type::markdown());
}

/// Networking functions: http(method, url) / http(method, url, body | headers) /
/// http(method, url, body, headers)
/// `method` may be a string (`"post"`) or a symbol (`:post`). `headers` is a dict of
/// string to string.
fn register_net(ctx: &mut InferenceContext) {
    let headers = Type::dict(Type::String, Type::String);

    for method in [Type::String, Type::Symbol] {
        register_binary(ctx, "http", method.clone(), Type::String, Type::String);
        register_ternary(ctx, "http", method.clone(), Type::String, Type::String, Type::String);
        register_ternary(ctx, "http", method.clone(), Type::String, headers.clone(), Type::String);
        register_many(
            ctx,
            &["http"],
            vec![method, Type::String, Type::String, headers.clone()],
            Type::String,
        );
    }

    // open_http takes the same arguments as http and streams the response body.
    for method in [Type::String, Type::Symbol] {
        register_binary(ctx, "open_http", method.clone(), Type::String, Type::Dynamic);
        register_ternary(
            ctx,
            "open_http",
            method.clone(),
            Type::String,
            Type::String,
            Type::Dynamic,
        );
        register_ternary(
            ctx,
            "open_http",
            method.clone(),
            Type::String,
            headers.clone(),
            Type::Dynamic,
        );
        register_many(
            ctx,
            &["open_http"],
            vec![method, Type::String, Type::String, headers.clone()],
            Type::Dynamic,
        );
    }

    // http_all: ([request]) -> [string], where each request is a dict with `url` and optional
    // `method`, `body` and `headers`. The values differ in type, so the element stays open.
    let request = ctx.fresh_var();
    register_unary(
        ctx,
        "http_all",
        Type::array(Type::Var(request)),
        Type::array(Type::String),
    );

    // mock_fetch: (url, body) -> none
    register_binary(ctx, "mock_fetch", Type::String, Type::String, Type::None);
}

fn register_bytes(ctx: &mut InferenceContext) {
    // _cbor_parse: string -> a, bytes -> a
    let a = ctx.fresh_var();
    register_unary(ctx, "_cbor_parse", Type::String, Type::Var(a));
    let a = ctx.fresh_var();
    register_unary(ctx, "_cbor_parse", Type::Bytes, Type::Var(a));

    // _cbor_stringify: (a) -> bytes
    let a = ctx.fresh_var();
    register_unary(ctx, "_cbor_stringify", Type::Var(a), Type::Bytes);

    // add: (bytes, bytes) -> bytes
    register_binary(ctx, "add", Type::Bytes, Type::Bytes, Type::Bytes);
    register_binary(ctx, "+", Type::Bytes, Type::Bytes, Type::Bytes);

    // slice: (bytes, number, number) -> bytes
    register_ternary(ctx, "slice", Type::Bytes, Type::Number, Type::Number, Type::Bytes);

    // reverse: (bytes) -> bytes
    register_unary(ctx, "reverse", Type::Bytes, Type::Bytes);
    register_unary(ctx, "reverse", Type::String, Type::String);

    // len: (bytes) -> number
    register_unary(ctx, "len", Type::Bytes, Type::Number);

    // base64: (bytes) -> string
    register_unary(ctx, "base64", Type::Bytes, Type::String);

    // sha512: (a) -> string  (accepts string, bytes, or any)
    let a = ctx.fresh_var();
    register_unary(ctx, "sha512", Type::Var(a), Type::String);
    register_unary(ctx, "sha512", Type::None, Type::None);

    // to_hex: (bytes) -> string
    register_unary(ctx, "to_hex", Type::Bytes, Type::String);

    // from_hex: (string) -> bytes
    register_unary(ctx, "from_hex", Type::String, Type::Bytes);
    register_unary(ctx, "from_hex", Type::None, Type::None);

    // hexdump: (bytes) -> string
    register_unary(ctx, "hexdump", Type::Bytes, Type::String);
    register_unary(ctx, "hexdump", Type::None, Type::None);

    // utf8: (bytes) -> string
    register_unary(ctx, "utf8", Type::Bytes, Type::String);
    register_unary(ctx, "utf8", Type::None, Type::None);

    // decode: (bytes, string) -> string
    register_binary(ctx, "decode", Type::Bytes, Type::String, Type::String);

    // encode: (string, string) -> bytes
    register_binary(ctx, "encode", Type::String, Type::String, Type::Bytes);

    // xor: (bytes, bytes) -> bytes
    register_binary(ctx, "xor", Type::Bytes, Type::Bytes, Type::Bytes);

    // band: (bytes, bytes) -> bytes
    register_binary(ctx, "band", Type::Bytes, Type::Bytes, Type::Bytes);

    // bor: (bytes, bytes) -> bytes
    register_binary(ctx, "bor", Type::Bytes, Type::Bytes, Type::Bytes);

    // bnot: (bytes) -> bytes
    register_unary(ctx, "bnot", Type::Bytes, Type::Bytes);

    // starts_with/ends_with: (bytes, bytes) -> bool
    register_binary(ctx, "starts_with", Type::Bytes, Type::Bytes, Type::Bool);
    register_binary(ctx, "ends_with", Type::Bytes, Type::Bytes, Type::Bool);
    register_binary(ctx, "contains", Type::Bytes, Type::Bytes, Type::Bool);

    // index/rindex: (bytes, bytes) -> number
    register_binary(ctx, "index", Type::Bytes, Type::Bytes, Type::Number);
    register_binary(ctx, "rindex", Type::Bytes, Type::Bytes, Type::Number);

    // index: ([a], a) -> number
    let a = ctx.fresh_var();
    register_binary(ctx, "index", Type::array(Type::Var(a)), Type::Var(a), Type::Number);
    // rindex: ([a], string) -> number; non-string elements never match
    let b = ctx.fresh_var();
    register_binary(ctx, "rindex", Type::array(Type::Var(b)), Type::String, Type::Number);
    register_binary(ctx, "indices", Type::Bytes, Type::Bytes, Type::array(Type::Number));

    // pack: (string, number) -> bytes
    register_binary(ctx, "pack", Type::String, Type::Number, Type::Bytes);
    register_binary(ctx, "pack", Type::String, Type::None, Type::None);

    // unpack: (string, bytes) -> number
    register_binary(ctx, "unpack", Type::String, Type::Bytes, Type::Number);
    register_binary(ctx, "unpack", Type::String, Type::None, Type::None);

    // repeat: (bytes, number) -> bytes
    register_binary(ctx, "repeat", Type::Bytes, Type::Number, Type::Bytes);

    // gt/gte/lt/lte are registered in register_comparison; no duplicate needed here
}

#[cfg(test)]
mod tests;
