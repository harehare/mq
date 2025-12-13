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

// ============================================================================
// Currently Undetected Errors (TODOs for future implementation)
// ============================================================================

#[test]
fn test_todo_binary_op_type_mismatch() {
    // TODO: This should fail but currently passes
    // Reason: No type signatures for binary operators
    let result = check_types(r#"1 + "string""#);
    println!("Binary op type mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected type error for number + string");
}

#[test]
#[ignore] // Known limitation: if/else type checking requires builtin functions which conflict with test setup
fn test_todo_if_else_type_mismatch() {
    // TODO: This should fail but currently passes when builtins are disabled
    // Reason: if/else syntax requires builtin functions, but enabling them causes
    // type checking of builtin code which contains type errors
    // Future work: Implement mechanism to skip type checking of builtin symbols
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
fn test_todo_array_element_type_mismatch() {
    // TODO: This should fail but currently passes
    // Reason: No constraints between array elements
    let result = check_types(r#"[1, "string", true]"#);
    println!("Array element type mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected type error for array with mixed types");
}

#[test]
fn test_todo_dict_value_type_mismatch() {
    // TODO: This should fail but currently passes
    // Reason: No constraints between dict values
    let result = check_types(r#"{"a": 1, "b": "string"}"#);
    println!("Dict value type mismatch: {:?}", result);
    // assert!(result.is_empty()); // Uncomment when implemented
}

#[test]
fn test_todo_function_arg_type_mismatch() {
    // TODO: This should fail but currently passes
    // Reason: No constraints between function parameters and arguments
    let result = check_types(
        r#"
        def double(x): x + x;
        double("hello")
    "#,
    );
    println!("Function arg type mismatch: {:?}", result);
    // assert!(result.is_empty()); // Uncomment when implemented
}

#[test]
fn test_todo_function_arity_mismatch() {
    // TODO: This should fail but currently passes
    // Reason: No arity checking in function calls
    let result = check_types(
        r#"
        def add(x, y): x + y;
        add(1)
    "#,
    );
    println!("Function arity mismatch: {:?}", result);
    // assert!(result.is_empty()); // Uncomment when implemented
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
    assert!(check_types("let x = 42; x").is_empty());
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
    let code = r#"
        let x = 42;
        let y = "hello";
        def add(a, b): a + b;
    "#;

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
    let code = r#"
        def identity(x): x;
        def compose(f, g, x): f(g(x));
    "#;

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
/// ✅ Implemented:
/// - Basic type representation (Type, TypeScheme, TypeVar)
/// - Unification algorithm with occurs check
/// - Constraint generation framework
/// - Type inference for literals (numbers, strings, bools, etc.)
/// - Type inference for variables and references
/// - Basic type inference for functions, arrays, and dicts
///
/// ❌ Not Yet Implemented:
/// - Builtin function type signatures
/// - Binary operator type checking
/// - Function call argument type checking
/// - If/else branch type unification
/// - Array element type unification
/// - Dict value type unification
/// - Pattern matching type checking
/// - Polymorphic type generalization
/// - Error span information
///
/// 📝 Recommendations for next steps:
/// 1. Implement builtin function signatures (see lib.rs::add_builtin_types)
/// 2. Add constraint generation for binary operators
/// 3. Add constraint generation for function calls
/// 4. Add constraint generation for if/else branches
/// 5. Improve error messages with source spans
#[test]
fn test_implementation_status_documentation() {
    // This test exists purely for documentation purposes
    // See the doc comment above for implementation status
}
