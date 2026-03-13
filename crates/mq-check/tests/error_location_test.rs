//! Tests for error location reporting
//!
//! This test file verifies that type errors include proper location information
//! (line and column numbers) when they are reported.

use mq_check::{TypeChecker, TypeError};
use mq_hir::Hir;

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
