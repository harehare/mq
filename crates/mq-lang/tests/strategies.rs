//! Property-based testing strategies for mq.
//!
//! This module provides reusable proptest strategies for generating
//! various types of mq expressions. These strategies can be used across
//! different test modules (optimizer, parser, evaluator, etc.) to ensure
//! consistent testing patterns.
//!
//! # Examples
//!
//! ```rust,ignore
//! use strategies::*;
//! use proptest::prelude::*;
//!
//! proptest! {
//!     #[test]
//!     fn test_something(expr in arb_arithmetic_expr()) {
//!         // Your test here
//!     }
//! }
//! ```

use proptest::prelude::*;

/// Strategy for generating simple arithmetic expressions that can be fully constant-folded.
pub fn arb_arithmetic_expr() -> impl Strategy<Value = (String, f64)> {
    (0i32..100, 0i32..100, prop::sample::select(vec!["add", "sub", "mul"])).prop_map(|(a, b, op)| {
        let a = a as f64;
        let b = b as f64;
        let expected = match op {
            "add" => a + b,
            "sub" => a - b,
            "mul" => a * b,
            _ => unreachable!(),
        };
        (format!("{}({}, {})", op, a, b), expected)
    })
}

/// Strategy for generating nested arithmetic expressions.
pub fn arb_nested_arithmetic_expr() -> impl Strategy<Value = String> {
    (0i32..20, 0i32..20, 0i32..20).prop_map(|(a, b, c)| format!("add(mul({}, {}), {})", a, b, c))
}

/// Strategy for generating string concatenation expressions.
pub fn arb_string_concat_expr() -> impl Strategy<Value = String> {
    (
        prop::string::string_regex("[a-z]{1,5}").unwrap(),
        prop::string::string_regex("[a-z]{1,5}").unwrap(),
    )
        .prop_map(|(a, b)| format!("add(\"{}\", \"{}\")", a, b))
}

/// Strategy for generating comparison expressions.
pub fn arb_comparison_expr() -> impl Strategy<Value = String> {
    (0i32..100, 0i32..100, prop::sample::select(vec!["eq", "ne", "gt", "lt"]))
        .prop_map(|(a, b, op)| format!("{}({}, {})", op, a, b))
}

/// Strategy for generating logical expressions.
pub fn arb_logical_expr() -> impl Strategy<Value = String> {
    (
        prop::bool::ANY,
        prop::bool::ANY,
        prop::sample::select(vec!["and", "or"]),
    )
        .prop_map(|(a, b, op)| format!("{}({}, {})", op, a, b))
}

/// Strategy for generating division and modulo expressions (avoiding division by zero).
pub fn arb_div_mod_expr() -> impl Strategy<Value = String> {
    (
        0i32..100,
        1i32..100, // Avoid division by zero
        prop::sample::select(vec!["div", "mod"]),
    )
        .prop_map(|(a, b, op)| format!("{}({}, {})", op, a, b))
}

/// Strategy for generating let expressions with simple arithmetic.
pub fn arb_let_expr() -> impl Strategy<Value = String> {
    (1i32..50, 1i32..50).prop_map(|(a, b)| format!("let x = {} | add(x, {})", a, b))
}

/// Strategy for generating deeply nested arithmetic expressions.
pub fn arb_deeply_nested_expr() -> impl Strategy<Value = String> {
    (0i32..10, 0i32..10, 0i32..10, 0i32..10)
        .prop_map(|(a, b, c, d)| format!("add(mul(add({}, {}), sub({}, {})), {})", a, b, c, d, 1))
}

/// Strategy for generating mixed type expressions with type conversions.
pub fn arb_mixed_type_expr() -> impl Strategy<Value = String> {
    (0i32..100).prop_map(|a| format!("to_number(to_string({}))", a))
}

/// Strategy for generating power expressions.
pub fn arb_power_expr() -> impl Strategy<Value = String> {
    (0i32..10, 0i32..5).prop_map(|(a, b)| format!("pow({}, {})", a, b))
}

/// Strategy for generating function definition and inline candidates.
pub fn arb_function_def_expr() -> impl Strategy<Value = String> {
    (1i32..20, 1i32..20).prop_map(|(a, b)| format!("def f(x): add(x, {}) | f({})", a, b))
}

/// Strategy for generating complex expressions combining multiple patterns.
pub fn arb_complex_expr() -> impl Strategy<Value = String> {
    (0i32..20, 0i32..20, 1i32..10).prop_map(|(a, b, c)| format!("let x = add({}, {}) | mul(x, {})", a, b, c))
}

/// Strategy for generating array literal expressions.
pub fn arb_array_expr() -> impl Strategy<Value = String> {
    prop::collection::vec(0i32..100, 1..5).prop_map(|nums| {
        format!(
            "[{}]",
            nums.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
        )
    })
}

/// Strategy for generating if expressions.
pub fn arb_if_expr() -> impl Strategy<Value = String> {
    (0i32..100, 0i32..100, 0i32..100, 0i32..100)
        .prop_map(|(a, b, then_val, else_val)| format!("if (eq({}, {})): {} else: {}", a, b, then_val, else_val))
}

/// Strategy for generating while loop expressions.
pub fn arb_while_expr() -> impl Strategy<Value = String> {
    (1i32..20, 1i32..10)
        .prop_map(|(init, step)| format!("let x = {} | while(gt(x, 0)): let x = sub(x, {}) | x", init, step))
}

/// Strategy for generating foreach loop expressions.
pub fn arb_foreach_expr() -> impl Strategy<Value = String> {
    (prop::collection::vec(1i32..20, 2..5), 1i32..10).prop_map(|(items, multiplier)| {
        format!(
            "foreach(x, [{}]): mul(x, {})",
            items.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", "),
            multiplier
        )
    })
}

/// Strategy for generating anonymous function (lambda) expressions.
pub fn arb_lambda_expr() -> impl Strategy<Value = String> {
    (1i32..50, 1i32..50).prop_map(|(a, b)| format!("let f = fn(x): add(x, {}); | f({})", a, b))
}

/// Strategy for generating try-catch expressions.
pub fn arb_try_catch_expr() -> impl Strategy<Value = String> {
    (1i32..100, 0i32..2, 0i32..100).prop_map(|(a, b, fallback)| format!("try: div({}, {}) catch: {}", a, b, fallback))
}

/// Strategy for generating not expressions.
pub fn arb_not_expr() -> impl Strategy<Value = String> {
    prop::bool::ANY.prop_map(|b| format!("not({})", b))
}

/// Strategy for generating ge/le comparison expressions.
pub fn arb_ge_le_expr() -> impl Strategy<Value = String> {
    (0i32..100, 0i32..100, prop::sample::select(vec!["gte", "lte"]))
        .prop_map(|(a, b, op)| format!("{}({}, {})", op, a, b))
}

/// Strategy for generating none literal expressions.
pub fn arb_none_expr() -> impl Strategy<Value = String> {
    Just("None".to_string())
}

/// Strategy for generating nested if expressions.
pub fn arb_nested_if_expr() -> impl Strategy<Value = String> {
    (0i32..30, 0i32..10, 0i32..10, 0i32..10).prop_map(|(x, v1, v2, v3)| {
        format!(
            "let x = {} | if (gt(x, 10)): (if (lt(x, 20)): {} else: {}) else: {}",
            x, v1, v2, v3
        )
    })
}

/// Strategy for generating chained comparison expressions.
pub fn arb_chained_comparison_expr() -> impl Strategy<Value = String> {
    (0i32..100, 0i32..50, 50i32..100)
        .prop_map(|(x, lower, upper)| format!("let x = {} | and(gt(x, {}), lt(x, {}))", x, lower, upper))
}

/// Strategy for generating a single base expression (non-recursive, suitable for piping).
fn arb_base_expr() -> impl Strategy<Value = String> {
    prop_oneof![
        arb_arithmetic_expr().prop_map(|(expr, _)| expr),
        arb_nested_arithmetic_expr(),
        arb_comparison_expr(),
        arb_logical_expr(),
        arb_div_mod_expr(),
        arb_power_expr(),
        arb_not_expr(),
        arb_ge_le_expr(),
        (0i32..100).prop_map(|n| n.to_string()),
        arb_array_expr(),
    ]
}

/// Strategy for generating pipe-chained expressions.
pub fn arb_piped_expr() -> impl Strategy<Value = String> {
    prop::collection::vec(arb_base_expr(), 2..5).prop_map(|exprs| exprs.join(" | "))
}

/// Strategy for generating any valid mq expression.
pub fn arb_any_expr() -> impl Strategy<Value = String> {
    prop_oneof![
        arb_arithmetic_expr().prop_map(|(expr, _)| expr),
        arb_nested_arithmetic_expr(),
        arb_string_concat_expr(),
        arb_comparison_expr(),
        arb_logical_expr(),
        arb_div_mod_expr(),
        arb_let_expr(),
        arb_deeply_nested_expr(),
        arb_mixed_type_expr(),
        arb_power_expr(),
        arb_function_def_expr(),
        arb_complex_expr(),
        arb_array_expr(),
        arb_if_expr(),
        arb_while_expr(),
        arb_while_expr(),
        arb_foreach_expr(),
        arb_lambda_expr(),
        arb_try_catch_expr(),
        arb_not_expr(),
        arb_ge_le_expr(),
        arb_none_expr(),
        arb_nested_if_expr(),
        arb_chained_comparison_expr(),
        arb_piped_expr(),
    ]
}
