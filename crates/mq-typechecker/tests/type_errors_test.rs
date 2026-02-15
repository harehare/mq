//! Tests for type error detection
//!
//! This test file demonstrates which type errors are currently detected
//! and which ones are not yet implemented.

use mq_hir::Hir;
use mq_typechecker::{TypeChecker, TypeError};

fn create_hir(code: &str) -> Hir {
    let mut hir = Hir::default();
    // Disable builtins before adding code to avoid type checking builtin functions
    hir.builtin.disabled = true;
    hir.add_code(None, code);
    hir.resolve();
    hir
}

fn check_types(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::new();
    checker.check(&hir)
}

#[test]
fn test_binary_op_type_mismatch() {
    let result = check_types(r#"1 + "string""#);
    println!("Binary op type mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected type error for number + string");
}

#[test]
fn test_if_else_type_mismatch() {
    let result = check_types(r#"if (true): 42 else: "string";"#);
    assert!(
        !result.is_empty(),
        "Expected type error for if/else branches with different types"
    );
}

#[test]
fn test_array_element_type_mismatch() {
    let result = check_types(r#"[1, "string", true]"#);
    println!("Array element type mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected type error for array with mixed types");
}

#[test]
fn test_dict_heterogeneous_values_allowed() {
    // mq dicts are like JSON objects - values can have different types
    // So this should succeed (heterogeneous values are allowed)
    let result = check_types(r#"{"a": 1, "b": "string"}"#);
    println!("Dict value type mismatch: {:?}", result);
    assert!(result.is_empty(), "Dict with mixed value types should be allowed");
}

#[test]
fn test_function_arity_mismatch() {
    let result = check_types("def add(x, y): x + y;\n| add(1)");
    println!("Function arity mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected arity mismatch error");
}

#[test]
fn test_match_pattern_type_mismatch() {
    // Matching a number against a string pattern should produce a type error
    let result = check_types(r#"match (1): | "hello": "matched" end"#);
    println!("Match pattern type mismatch: {:?}", result);
    assert!(
        !result.is_empty(),
        "Expected type error for matching number against string pattern"
    );
}

#[test]
fn test_match_arm_body_type_mismatch() {
    // Match arms with different body types should produce a type error
    let result = check_types(r#"match (1): | 1: "one" | 2: 222 end"#);
    println!("Match arm body type mismatch: {:?}", result);
    assert!(
        !result.is_empty(),
        "Expected type error for match arms with different body types"
    );
}

// ============================================================================
// Expected Success Cases (should always pass)
// ============================================================================

#[test]
fn test_success_simple_literal() {
    let result = check_types("42");
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty());
}

#[test]
fn test_success_simple_variable() {
    assert!(check_types("let x = 42 | x").is_empty());
}

#[test]
fn test_success_simple_function() {
    assert!(check_types("def id(x): x;").is_empty());
}

#[test]
fn test_success_homogeneous_array() {
    assert!(check_types("[1, 2, 3]").is_empty());
}

#[test]
fn test_success_homogeneous_dict() {
    assert!(check_types(r#"{"a": 1, "b": 2}"#).is_empty());
}

#[test]
fn test_success_match_consistent_patterns() {
    let result = check_types(r#"match (1): | 1: "one" | 2: "two" end"#);
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(
        result.is_empty(),
        "Match with consistent pattern/body types should succeed"
    );
}

#[test]
fn test_success_match_variable_pattern() {
    let result = check_types(r#"match (1): | x: x end"#);
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "Match with variable pattern should succeed");
}

#[test]
fn test_success_while_loop() {
    let result = check_types("while (true): 1;");
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "While loop with bool condition should succeed");
}

#[test]
fn test_while_condition_type_mismatch() {
    let result = check_types("while (42): 1;");
    println!("While condition type mismatch: {:?}", result);
    assert!(
        !result.is_empty(),
        "Expected type error for while with non-bool condition"
    );
}

#[test]
fn test_success_macro_definition() {
    let result = check_types("macro inc(x): x + 1;");
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "Macro definition should succeed");
}

// Demonstration: How to use the typechecker programmatically
#[test]
fn test_inspect_inferred_types() {
    let code = "def add(a, b): a + b;";

    let hir = create_hir(code);
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    println!("\n=== Inferred Types ===");
    for (symbol_id, type_scheme) in checker.symbol_types() {
        if let Some(symbol) = hir.symbol(*symbol_id)
            && let Some(name) = &symbol.value
        {
            println!("{}: {}", name, type_scheme);
        }
    }
}

#[test]
fn test_inspect_type_variables() {
    let code = "def identity(x): x;";

    let hir = create_hir(code);
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    println!("\n=== Type Variables ===");
    for (symbol_id, type_scheme) in checker.symbol_types() {
        if let Some(symbol) = hir.symbol(*symbol_id)
            && symbol.is_function()
            && let Some(name) = &symbol.value
        {
            println!("{}: {}", name, type_scheme);
        }
    }
}
