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

// ============================================================================
// Type Error Detection Tests
// ============================================================================

#[test]
fn test_binary_op_type_mismatch() {
    let result = check_types(r#"1 + "string""#);
    println!("Binary op type mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected type error for number + string");
}

#[test]
#[ignore] // Known limitation: if/else type checking requires builtin functions which conflict with test setup
fn test_if_else_type_mismatch() {
    let result = check_types(
        r#"
        if true:
            42
        else:
            "string"
        ;
    "#,
    );
    println!("If/else type mismatch: {:?}", result);
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

// ============================================================================
// Demonstration: How to use the typechecker programmatically
// ============================================================================

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

// ============================================================================
// Documentation: Current Implementation Status
// ============================================================================

/// Current implementation status of the type checker:
///
/// Implemented:
/// - Basic type representation (Type, TypeScheme, TypeVar)
/// - Unification algorithm with occurs check
/// - Constraint generation framework
/// - Type inference for literals (numbers, strings, bools, etc.)
/// - Type inference for variables and references
/// - Type inference for functions, arrays, and dicts
/// - Builtin function type signatures and overload resolution
/// - Binary operator type checking
/// - User-defined function call argument type checking and arity checking
/// - Array element type unification
/// - Dict key type unification (values are heterogeneous like JSON)
/// - Foreach iterator type checking
/// - Match arm type unification
/// - Try/catch type unification
/// - Error collection (multiple errors reported)
///
/// Not Yet Implemented:
/// - If/else branch type unification (requires builtin function handling)
/// - Pattern matching type checking (detailed)
/// - Polymorphic type generalization
/// - Error span information
#[test]
fn test_implementation_status_documentation() {
    // This test exists purely for documentation purposes
    // See the doc comment above for implementation status
}
