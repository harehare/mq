//! Property-based testing strategies for mq-lang.
//!
//! This module provides reusable proptest strategies for generating
//! various types of mq expressions. These strategies can be used across
//! different test modules (optimizer, parser, evaluator, etc.) to ensure
//! consistent testing patterns.
//!
//! # Examples
//!
//! ```rust,ignore
//! use mq_test::proptest_strategies::*;
//! use proptest::prelude::*;
//!
//! proptest! {
//!     #[test]
//!     fn test_something(expr in arb_arithmetic_expr()) {
//!         // Your test here
//!     }
//! }
//! ```

pub mod expr;

use proptest::prelude::*;

/// Strategy for generating simple arithmetic expressions that can be fully constant-folded.
///
/// Generates expressions like `add(5, 3)`, `sub(10, 2)`, `mul(4, 7)`.
///
/// Returns a tuple of (expression_string, expected_result).
pub fn arb_arithmetic_expr() -> impl Strategy<Value = (String, f64)> {
    (
        0i32..100,
        0i32..100,
        prop::sample::select(vec!["add", "sub", "mul"]),
    )
        .prop_map(|(a, b, op)| {
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
///
/// Generates expressions like `add(mul(5, 3), 7)`.
pub fn arb_nested_arithmetic_expr() -> impl Strategy<Value = String> {
    (0i32..20, 0i32..20, 0i32..20).prop_map(|(a, b, c)| format!("add(mul({}, {}), {})", a, b, c))
}

/// Strategy for generating string concatenation expressions.
///
/// Generates expressions like `add("hello", "world")`.
pub fn arb_string_concat_expr() -> impl Strategy<Value = String> {
    (
        prop::string::string_regex("[a-z]{1,5}").unwrap(),
        prop::string::string_regex("[a-z]{1,5}").unwrap(),
    )
        .prop_map(|(a, b)| format!("add(\"{}\", \"{}\")", a, b))
}

/// Strategy for generating comparison expressions.
///
/// Generates expressions like `eq(5, 3)`, `gt(10, 20)`, etc.
pub fn arb_comparison_expr() -> impl Strategy<Value = String> {
    (
        0i32..100,
        0i32..100,
        prop::sample::select(vec!["eq", "ne", "gt", "lt"]),
    )
        .prop_map(|(a, b, op)| format!("{}({}, {})", op, a, b))
}

/// Strategy for generating logical expressions.
///
/// Generates expressions like `and(true, false)`, `or(true, true)`.
pub fn arb_logical_expr() -> impl Strategy<Value = String> {
    (
        prop::bool::ANY,
        prop::bool::ANY,
        prop::sample::select(vec!["and", "or"]),
    )
        .prop_map(|(a, b, op)| format!("{}({}, {})", op, a, b))
}

/// Strategy for generating division and modulo expressions (avoiding division by zero).
///
/// Generates expressions like `div(10, 2)`, `mod(7, 3)`.
pub fn arb_div_mod_expr() -> impl Strategy<Value = String> {
    (
        0i32..100,
        1i32..100, // Avoid division by zero
        prop::sample::select(vec!["div", "mod"]),
    )
        .prop_map(|(a, b, op)| format!("{}({}, {})", op, a, b))
}

/// Strategy for generating let expressions with simple arithmetic.
///
/// Generates expressions like `let x = 5 | add(x, 3)`.
pub fn arb_let_expr() -> impl Strategy<Value = String> {
    (1i32..50, 1i32..50).prop_map(|(a, b)| format!("let x = {} | add(x, {})", a, b))
}

/// Strategy for generating deeply nested arithmetic expressions.
///
/// Generates expressions like `add(mul(add(1, 2), sub(3, 4)), 1)`.
pub fn arb_deeply_nested_expr() -> impl Strategy<Value = String> {
    (0i32..10, 0i32..10, 0i32..10, 0i32..10)
        .prop_map(|(a, b, c, d)| format!("add(mul(add({}, {}), sub({}, {})), {})", a, b, c, d, 1))
}

/// Strategy for generating mixed type expressions with type conversions.
///
/// Generates expressions like `to_number(to_string(42))`.
pub fn arb_mixed_type_expr() -> impl Strategy<Value = String> {
    (0i32..100).prop_map(|a| format!("to_number(to_string({}))", a))
}

/// Strategy for generating power expressions.
///
/// Generates expressions like `pow(2, 3)`.
pub fn arb_power_expr() -> impl Strategy<Value = String> {
    (0i32..10, 0i32..5).prop_map(|(a, b)| format!("pow({}, {})", a, b))
}

/// Strategy for generating function definition and inline candidates.
///
/// Generates expressions like `def f(x): add(x, 5) | f(10)`.
pub fn arb_function_def_expr() -> impl Strategy<Value = String> {
    (1i32..20, 1i32..20).prop_map(|(a, b)| format!("def f(x): add(x, {}) | f({})", a, b))
}

/// Strategy for generating complex expressions combining multiple patterns.
///
/// Generates expressions like `let x = add(1, 2) | mul(x, 3)`.
pub fn arb_complex_expr() -> impl Strategy<Value = String> {
    (0i32..20, 0i32..20, 1i32..10)
        .prop_map(|(a, b, c)| format!("let x = add({}, {}) | mul(x, {})", a, b, c))
}

/// Strategy for generating any valid mq expression.
///
/// This is a union of all expression types for comprehensive testing.
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
    ]
}
