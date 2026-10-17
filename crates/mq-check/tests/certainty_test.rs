//! Tests for the split of diagnostics into definite and possible errors

use mq_check::{Certainty, TypeChecker};
use mq_hir::Hir;
use rstest::rstest;

fn certainties(code: &str) -> Vec<Certainty> {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    TypeChecker::new()
        .check_classified(&hir)
        .into_iter()
        .map(|c| c.certainty)
        .collect()
}

#[rstest]
#[case::literal_operands(r#""a" - 1"#)]
#[case::builtin_argument("abs(\"a\")")]
#[case::user_function_call_site("def f(x): x + 1; | f(true)")]
fn test_error_on_known_types_is_definite(#[case] code: &str) {
    let found = certainties(code);
    assert!(!found.is_empty(), "expected a type error for {code}");
    assert!(found.iter().all(|c| *c == Certainty::Definite), "{code}: {found:?}");
}

#[rstest]
#[case::reassigned_variable(r#"var x = 1 | x = "a" | x - 1"#)]
#[case::parameter(r#"def f(x): x + 1 | x - "a" end"#)]
#[case::try_body(r#"try: "a" - 1 catch: 0"#)]
#[case::unreachable(r#"let x = 1 | if (is_none(x)): 1 else: 2"#)]
fn test_error_that_relies_on_an_approximation_is_possible(#[case] code: &str) {
    let found = certainties(code);
    assert!(!found.is_empty(), "expected a diagnostic for {code}");
    assert!(found.iter().all(|c| *c == Certainty::Possible), "{code}: {found:?}");
}

#[test]
fn test_check_returns_the_same_errors_as_check_classified() {
    let mut hir = Hir::default();
    hir.add_code(None, r#""a" - 1 | try: abs("b") catch: 0"#);
    let plain = TypeChecker::new().check(&hir);
    let classified = TypeChecker::new().check_classified(&hir);
    assert_eq!(plain.len(), classified.len());
}
