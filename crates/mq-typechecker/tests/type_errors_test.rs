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
    // mq is dynamically typed — if/else branches with different types are allowed
    let result = check_types(r#"if (true): 42 else: "string";"#);
    assert!(
        result.is_empty(),
        "if/else branches with different types should be allowed in dynamically typed mq: {:?}",
        result
    );
}

#[test]
fn test_heterogeneous_array_allowed() {
    // mq is dynamically typed — heterogeneous arrays (used as tuples) are valid
    let result = check_types(r#"[1, "string", true]"#);
    println!("Heterogeneous array: {:?}", result);
    assert!(
        result.is_empty(),
        "Heterogeneous arrays should be allowed (tuple pattern): {:?}",
        result
    );
}

#[test]
fn test_dict_heterogeneous_values_allowed() {
    // With row polymorphism, each field has its own type in a Record
    // {"a": 1, "b": "string"} → Record({a: int, b: string}, RowEmpty)
    let result = check_types(r#"{"a": 1, "b": "string"}"#);
    println!("Dict with heterogeneous values: {:?}", result);
    assert!(
        result.is_empty(),
        "Dict with mixed value types should be allowed via row polymorphism"
    );
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
    // mq is dynamically typed — match arms with different body types are allowed
    let result = check_types(r#"match (1): | 1: "one" | 2: 222 end"#);
    println!("Match arm body type mismatch: {:?}", result);
    assert!(
        result.is_empty(),
        "Match arms with different body types should be allowed in dynamically typed mq: {:?}",
        result
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
