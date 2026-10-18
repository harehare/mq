//! Tests for the split of diagnostics into definite and uncertain errors

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
#[case::result_of_user_function(r#"def f(x): "a"; | f(1) | abs()"#)]
#[case::lambda_over_exact_collection(r#"map(["a", "b"], fn(s): abs(s);)"#)]
#[case::inner_error_in_user_call(r#"def f(x): x; | f("a" - 1)"#)]
#[case::index_of_exact_array(r#"let a = [true, true] | a[0] + a[1]"#)]
#[case::filter_with_exact_body(r#"abs(filter([1, 2], fn(x): x > 1;))"#)]
#[case::piped_from_literal(r#""a" | abs()"#)]
#[case::constant_variable(r#"let s = "a" | abs(s)"#)]
#[case::both_branches_exact(r#"if (true): "a" else: "b" | abs()"#)]
#[case::operator_on_literals(r#"[1, 2] - 1"#)]
fn test_error_on_known_types_is_definite(#[case] code: &str) {
    let found = certainties(code);
    assert!(!found.is_empty(), "expected a type error for {code}");
    assert!(found.iter().all(|c| *c == Certainty::Definite), "{code}: {found:?}");
}

#[rstest]
#[case::reassigned_variable(r#"var x = 1 | x = "a" | x - 1"#)]
#[case::parameter(r#"def f(x): x + 1 | x - "a" end"#)]
#[case::try_body(r#"try: "a" - 1 catch: 0"#)]
#[case::narrowed_variable(r#"let s = "a" | if (is_string(s)): s - 1 else: 0"#)]
#[case::lambda_over_unknown_collection(r#"var ys = ["a"] | ys = ["b"] | map(ys, fn(s): abs(s);)"#)]
#[case::map_with_unknown_body(r#"var k = 1 | k = "a" | abs(map([1, 2], fn(x): x + k;))"#)]
#[case::narrowed_parameter(r#"def f(x): if (is_string(x)): x - 1 else: 0; | f("a")"#)]
#[case::unaudited_builtin(r#"date_add("2024-01-02", 1, "days")"#)]
#[case::unreachable(r#"let x = 1 | if (is_none(x)): 1 else: 2"#)]
fn test_error_that_relies_on_an_approximation_is_uncertain(#[case] code: &str) {
    let found = certainties(code);
    assert!(!found.is_empty(), "expected a diagnostic for {code}");
    assert!(found.iter().all(|c| *c == Certainty::Uncertain), "{code}: {found:?}");
}

#[test]
fn test_check_returns_the_same_errors_as_check_classified() {
    let mut hir = Hir::default();
    hir.add_code(None, r#""a" - 1 | try: abs("b") catch: 0"#);
    let plain = TypeChecker::new().check(&hir);
    let classified = TypeChecker::new().check_classified(&hir);
    assert_eq!(plain.len(), classified.len());
}
