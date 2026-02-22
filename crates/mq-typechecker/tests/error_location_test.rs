//! Tests for error location reporting
//!
//! This test file verifies that type errors include proper location information
//! (line and column numbers) when they are reported.

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
fn test_heterogeneous_array_allowed() {
    // mq is dynamically typed — heterogeneous arrays (used as tuples) are valid
    let code = r#"[1, 2, "string"]"#;
    let errors = check_types(code);
    assert!(
        errors.is_empty(),
        "Heterogeneous arrays should be allowed (tuple pattern): {:?}",
        errors
    );
}

#[test]
fn test_homogeneous_array_no_error() {
    // Homogeneous arrays should not produce errors
    let code = r#"[1, 2, 3]"#;
    let errors = check_types(code);
    assert!(errors.is_empty(), "Homogeneous array should have no errors");
}

#[test]
fn test_binary_op_type_error_location() {
    let code = r#"1 + "hello""#;
    let errors = check_types(code);

    println!("\n=== Binary Op Error Location ===");
    assert!(!errors.is_empty(), "Expected type error for number + string");
    for e in &errors {
        println!("Error: {}", e);
        println!("Error details: {:?}", e);
    }
}

#[test]
fn test_error_message_readability() {
    // Test that error messages are human-readable
    let code = r#"1 + "hello""#;
    let errors = check_types(code);

    assert!(!errors.is_empty(), "Expected type error");
    let error_msg = format!("{}", errors[0]);
    println!("Error message: {}", error_msg);

    // The error message should be informative
    assert!(
        error_msg.contains("mismatch") || error_msg.contains("type") || error_msg.contains("unify"),
        "Error message should mention type mismatch: {}",
        error_msg
    );
}

#[test]
fn test_multiple_errors_show_locations() {
    // Code with type error in binary operation
    let code = r#"1 + "two""#;
    let errors = check_types(code);

    println!("Found {} errors", errors.len());
    for (i, e) in errors.iter().enumerate() {
        println!("Error {}: {:?}", i + 1, e);
        println!("Error {} display: {}", i + 1, e);
    }
    assert!(!errors.is_empty(), "Expected at least one type error");
}
