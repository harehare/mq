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
#[case::narrowed_by_a_type_test(r#"let s = "a" | if (is_string(s)): s - 1 else: 0"#)]
#[case::narrowed_away_by_a_type_test(r#"let s = if (true): "a" else: 1 | if (is_string(s)): 0 else: s - "x""#)]
#[case::variable_that_keeps_its_type(r#"var n = 0 | n += 1 | n - "a""#)]
#[case::lambda_over_variable_that_keeps_its_type(r#"var ys = ["a"] | ys = ["b"] | map(ys, fn(s): abs(s);)"#)]
#[case::piped_from_literal(r#""a" | abs()"#)]
#[case::constant_variable(r#"let s = "a" | abs(s)"#)]
#[case::both_branches_exact(r#"if (true): "a" else: "b" | abs()"#)]
#[case::operator_on_literals(r#"[1, 2] - 1"#)]
#[case::parameter_that_no_overload_takes(r#"def f(x): upcase(x, 1);"#)]
#[case::parameter_beside_an_operand_that_no_overload_takes(r#"def f(x): x - [1];"#)]
#[case::parameter_in_a_pipe(r#"def f(x): x + 1 | x - [1] end"#)]
#[case::parameter_in_a_lambda(r#"def f(xs): map(xs, fn(v): v - [1];);"#)]
#[case::parameter_in_a_loop(r#"def f(xs): foreach(v, xs): v - [1];;"#)]
#[case::result_that_does_not_depend_on_the_parameter(r#"def f(x): to_string(x) - [1];"#)]
#[case::comparison_of_a_parameter(r#"def f(x): (x == 1) + 1;"#)]
#[case::result_of_a_parameter_in_a_variable(r#"def f(x): let s = to_string(x) | s - [1];"#)]
#[case::array_of_a_parameter(r#"def f(x): abs([x, 1]);"#)]
#[case::array_of_a_parameter_in_a_pipe(r#"def f(x): [x] | abs();"#)]
#[case::dict_of_a_parameter(r#"def f(x): {"a": x} - 1;"#)]
#[case::argument_of_a_builtin_in_the_body(r#"def f(x): abs(x); | f("a")"#)]
#[case::argument_piped_in_the_body(r#"def f(x): x | abs(); | f("a")"#)]
#[case::call_that_is_piped_to(r#"def f(x): x | abs(); | "a" | f()"#)]
#[case::argument_through_another_function(r#"def f(x): abs(x); | def g(y): f(y); | g("a")"#)]
#[case::argument_beside_a_loop(r#"def f(x): let n = abs(x) | foreach (i, [1, 2]): i + n;; | f("a")"#)]
#[case::argument_through_a_variable(r#"def f(x): let y = x | abs(y); | f("a")"#)]
#[case::parameter_that_no_type_fits(r#"def f(x): abs(x) | upcase(x) end"#)]
#[case::parameters_that_no_type_fits(r#"def f(x, y): abs(x) | upcase(y) | abs(y)"#)]
#[case::lambda_applied_to_an_argument(r#"def apply(f, v): f(v); | apply(fn(x): abs(x);, "a")"#)]
#[case::lambda_applied_twice(r#"def twice(f, x): f(f(x)); | twice(fn(y): abs(y);, "a")"#)]
#[case::lambda_with_an_operator(r#"def apply(f, v): f(v); | apply(fn(x): x - 1;, [1])"#)]
#[case::lambda_of_a_builtin_over_an_argument(r#"def f(xs): map(xs, fn(s): abs(s);); | f(["a"])"#)]
#[case::lambda_of_a_piped_builtin(r#"def f(xs): xs | map(fn(s): abs(s);); | f(["a"])"#)]
#[case::lambda_with_an_unrelated_dictionary(r#"def f(xs, meta): map(xs, fn(s): abs(s);); | f(["a"], {"k": 1})"#)]
#[case::lambda_over_the_result_of_a_map(r#"def f(xs): xs | map(fn(s): to_string(s);) | map(fn(t): abs(t);); | f([1])"#)]
#[case::lambda_over_a_filtered_argument(r#"def f(xs): xs | filter(fn(s): s > 0;) | map(fn(t): upcase(t);); | f([1])"#)]
#[case::argument_beside_a_default(r#"def f(x, y = 1): abs(x) + y; | f("a")"#)]
#[case::argument_and_a_given_default(r#"def f(x, y = 1): abs(x) + y; | f("a", 2)"#)]
#[case::result_of_a_builtin_in_the_body(r#"def f(x): let y = abs(x) | y | upcase(); | f(1)"#)]
fn test_error_on_known_types_is_definite(#[case] code: &str) {
    let found = certainties(code);
    assert!(!found.is_empty(), "expected a type error for {code}");
    assert!(found.iter().all(|c| *c == Certainty::Definite), "{code}: {found:?}");
}

#[rstest]
#[case::reassigned_variable(r#"var x = 1 | x = "a" | x - 1"#)]
#[case::parameter_used_in_a_branch(r#"def f(x): if (true): abs(x) else: 0 | upcase(x)"#)]
#[case::try_body(r#"try: "a" - 1 catch: 0"#)]
#[case::narrowed_variable(r#"let s = "a" | if (len(s) > 0): s - 1 else: 0"#)]
#[case::variable_that_changes_type(r#"var s = "a" | s = 1 | upcase(s)"#)]
#[case::lambda_over_an_array_that_may_be_empty(r#"def f(xs): map(xs, fn(s): abs(s);); | f(slice(["a"], 0, 0))"#)]
#[case::lambda_over_an_array_piped_from_a_call(r#"def f(xs): map(xs, fn(s): abs(s);); | slice(["a"], 0, 0) | f()"#)]
#[case::map_with_unknown_body(r#"var k = 1 | k = "a" | abs(map([1, 2], fn(x): x + k;))"#)]
#[case::narrowed_parameter(r#"def f(x): if (is_string(x)): x - 1 else: 0; | f("a")"#)]
#[case::argument_that_changes_type(r#"def f(x): abs(x); | var s = 1 | s = "a" | f(s)"#)]
#[case::argument_in_a_try_body(r#"def f(x): abs(x); | try: f("a") catch: 0"#)]
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
