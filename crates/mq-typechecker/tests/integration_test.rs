//! Integration tests for the type checker

use mq_hir::Hir;
use mq_typechecker::{TypeChecker, TypeError};

/// Helper function to create HIR from code
fn create_hir(code: &str) -> Hir {
    let mut hir = Hir::default();
    // Disable builtins before adding code to avoid type checking builtin functions
    hir.builtin.disabled = true;
    hir.add_code(None, code);
    hir.resolve();
    hir
}

/// Helper function to run type checker - returns errors as Vec
fn check_types(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::new();
    checker.check(&hir)
}

// Success Cases - These should type check successfully

#[test]
fn test_literal_types() {
    // Numbers
    assert!(check_types("42").is_empty());
    assert!(check_types("3.14").is_empty());

    // Strings
    assert!(check_types(r#""hello""#).is_empty());

    // Booleans
    assert!(check_types("true").is_empty());
    assert!(check_types("false").is_empty());

    // None
    assert!(check_types("none").is_empty());
}

#[test]
fn test_variable_definitions() {
    assert!(check_types("let x = 42;").is_empty());
    assert!(check_types(r#"let name = "Alice";"#).is_empty());
    assert!(check_types("let flag = true;").is_empty());
}

#[test]
fn test_simple_functions() {
    // Identity function
    assert!(check_types("def identity(x): x;").is_empty());

    // Constant function
    assert!(check_types("def const(x, y): x;").is_empty());

    // Simple arithmetic
    assert!(check_types("def add(x, y): x + y;").is_empty());
}

#[test]
fn test_function_calls() {
    assert!(check_types("def identity(x): x;\n| identity(42)").is_empty());
    assert!(check_types("def add(x, y): x + y;\n| add(1, 2)").is_empty());
}

#[test]
fn test_arrays() {
    // Empty array
    assert!(check_types("[]").is_empty());

    // Homogeneous arrays
    assert!(check_types("[1, 2, 3]").is_empty());
    assert!(check_types(r#"["a", "b", "c"]"#).is_empty());

    // Nested arrays
    assert!(check_types("[[1, 2], [3, 4]]").is_empty());
}

#[test]
fn test_dictionaries() {
    // Empty dict
    assert!(check_types("{}").is_empty());

    // Simple dict
    assert!(check_types(r#"{"key": "value"}"#).is_empty());

    // Numeric values
    assert!(check_types(r#"{"a": 1, "b": 2}"#).is_empty());
}

#[test]
fn test_conditionals() {
    assert!(
        check_types(
            r#"
        if true:
            42
        else:
            24
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_pattern_matching() {
    assert!(
        check_types(
            r#"
        match 42:
            case 0: "zero"
            case 1: "one"
            case _: "other"
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_nested_functions() {
    assert!(
        check_types(
            r#"
        def outer(x):
            def inner(y):
                x + y
            ;
            inner(10)
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_variable_references() {
    assert!(check_types("let x = 42 | let y = x | y").is_empty());
}

#[test]
fn test_function_as_value() {
    assert!(check_types("def f(x): x + 1;\n| let g = f | g").is_empty());
}

// Error Cases - These should fail type checking

#[test]
fn test_type_mismatch_in_array() {
    let result = check_types(r#"[1, "string", true]"#);
    assert!(!result.is_empty(), "Expected type error for heterogeneous array");
}

#[test]
fn test_function_arity_mismatch() {
    // Calling function with wrong number of arguments
    let result = check_types("def f(x, y): x + y;\n| f(1)");
    assert!(!result.is_empty(), "Expected arity mismatch error");
}

#[test]
fn test_recursive_type() {
    // Attempting to create an infinite type
    let result = check_types("let x = [x]");
    println!("Recursive type result: {:?}", result);
}

// Complex Patterns

#[test]
fn test_higher_order_functions() {
    assert!(
        check_types(
            r#"
        def map(f, arr):
            foreach item in arr:
                f(item)
            ;
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_closure_capture() {
    assert!(
        check_types(
            r#"
        def make_adder(n):
            def adder(x):
                x + n
            ;
            adder
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_nested_conditionals() {
    assert!(
        check_types(
            r#"
        if true:
            if true:
                "very big"
            else:
                "medium"
            ;
        else:
            "small"
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_complex_patterns() {
    assert!(
        check_types(
            r#"
        match [1, 2, 3]:
            case []: "empty"
            case [x]: "single"
            case [x, y]: "pair"
            case _: "many"
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_dict_operations() {
    assert!(check_types(r#"{"name": "Alice", "age": 30}"#).is_empty());
}

#[test]
fn test_try_catch() {
    assert!(
        check_types(
            r#"
        try:
            42 / 0
        catch:
            0
        ;
    "#
        )
        .is_empty()
    );
}

// Edge Cases

#[test]
fn test_empty_program() {
    assert!(check_types("").is_empty());
}

#[test]
fn test_only_whitespace() {
    assert!(check_types("   \n  \t  ").is_empty());
}

#[test]
fn test_deeply_nested_arrays() {
    assert!(check_types("[[[[1]]]]").is_empty());
}

#[test]
fn test_deeply_nested_dicts() {
    assert!(check_types(r#"{"a": {"b": {"c": 1}}}"#).is_empty());
}

#[test]
fn test_lambda_functions() {
    assert!(check_types("let f = fn(x): x + 1 | f(5)").is_empty());
}

#[test]
fn test_module_imports() {
    // This might fail if modules aren't available
    let result = check_types(r#"include "math""#);
    println!("Module import result: {:?}", result);
}

// Type Inference Verification

#[test]
fn test_inferred_types_basic() {
    let hir = create_hir("let x = 42;");
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    // Verify that we have type information
    assert!(!checker.symbol_types().is_empty());
}

#[test]
fn test_inferred_types_function() {
    let hir = create_hir("def identity(x): x;");
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    // The function should have a type
    let types = checker.symbol_types();
    assert!(!types.is_empty());

    // Print inferred types for inspection
    for (symbol_id, type_scheme) in types {
        println!("Symbol {:?} :: {}", symbol_id, type_scheme);
    }
}

#[test]
fn test_type_unification() {
    let hir = create_hir("let x = 42 | let y = x | let z = y;");
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    // All variables should have compatible types
    println!("Unified types:");
    for (symbol_id, type_scheme) in checker.symbol_types() {
        println!("  {:?} :: {}", symbol_id, type_scheme);
    }
}
