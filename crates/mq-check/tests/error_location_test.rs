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
    // mq is dynamically typed: heterogeneous arrays (used as tuples) are valid
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

fn classified(code: &str) -> Vec<mq_check::Classified> {
    let mut hir = mq_hir::Hir::default();
    hir.add_code(None, code);
    mq_check::TypeChecker::new().check_classified(&hir)
}

#[test]
fn test_argument_mismatch_names_the_expected_type_first() {
    let errors = classified("let s = \"a\"\n| abs(s)");
    assert!(matches!(
        errors.as_slice(),
        [c] if c.error.to_string() == "Type mismatch: expected number, found string"
    ));
}

#[test]
fn test_variable_argument_points_at_its_definition() {
    let errors = classified("let s = \"a\"\n| let n = 1\n| abs(s)");
    let [c] = errors.as_slice() else {
        panic!("{errors:?}");
    };
    assert_eq!(c.related.len(), 1, "{:?}", c.related);
    assert_eq!(c.related[0].message, "`s` is `string`");
    assert_eq!(c.related[0].range.start.line, 1);
}

#[test]
fn test_literal_argument_has_nothing_to_point_at() {
    let errors = classified("abs(\"a\")");
    assert!(errors.iter().all(|c| c.related.is_empty()), "{errors:?}");
}
