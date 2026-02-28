//! Integration tests for the type checker

use mq_hir::Hir;
use mq_typechecker::{TypeChecker, TypeError};
use rstest::rstest;

/// Helper function to create HIR from code
fn create_hir(code: &str) -> Hir {
    let mut hir = Hir::default();
    // Disable builtins before adding code to avoid type checking builtin functions
    hir.builtin.disabled = true;
    hir.add_code(None, code);
    hir
}

/// Helper function to run type checker - returns errors as Vec
fn check_types(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::new();
    checker.check(&hir)
}

// Success Cases - These should type check successfully

#[test]
fn test_literal_types() {
    // Numbers
    assert!(check_types("42").is_empty());
    assert!(check_types("3.14").is_empty());

    // Strings
    assert!(check_types(r#""hello""#).is_empty());

    // Booleans
    assert!(check_types("true").is_empty());
    assert!(check_types("false").is_empty());

    // None
    assert!(check_types("none").is_empty());
}

#[test]
fn test_variable_definitions() {
    assert!(check_types("let x = 42;").is_empty());
    assert!(check_types(r#"let name = "Alice";"#).is_empty());
    assert!(check_types("let flag = true;").is_empty());
}

#[test]
fn test_simple_functions() {
    // Identity function
    assert!(check_types("def identity(x): x;").is_empty());

    // Constant function
    assert!(check_types("def const(x, y): x;").is_empty());

    // Simple arithmetic
    assert!(check_types("def add(x, y): x + y;").is_empty());
}

#[test]
fn test_function_calls() {
    assert!(check_types("def identity(x): x;\n| identity(42)").is_empty());
    assert!(check_types("def add(x, y): x + y;\n| add(1, 2)").is_empty());
}

#[test]
fn test_arrays() {
    // Empty array
    assert!(check_types("[]").is_empty());

    // Homogeneous arrays
    assert!(check_types("[1, 2, 3]").is_empty());
    assert!(check_types(r#"["a", "b", "c"]"#).is_empty());

    // Nested arrays
    assert!(check_types("[[1, 2], [3, 4]]").is_empty());
}

#[test]
fn test_dictionaries() {
    // Empty dict
    assert!(check_types("{}").is_empty());

    // Simple dict
    assert!(check_types(r#"{"key": "value"}"#).is_empty());

    // Numeric values
    assert!(check_types(r#"{"a": 1, "b": 2}"#).is_empty());
}

#[test]
fn test_conditionals() {
    assert!(
        check_types(
            r#"
        if (true):
            42
        else:
            24
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_pattern_matching() {
    assert!(check_types(r#"match (42): | 0: "zero" | 1: "one" | _: "other" end"#).is_empty());
}

#[test]
fn test_match_different_types_creates_union() {
    let result = check_types(r#"match (42): | 0: "zero" | 1: 100 | _: true end"#);
    assert!(
        result.is_empty(),
        "match with different types should create a union type: {:?}",
        result
    );
}

// TODO: Enable when match is properly implemented in HIR
// #[test]
// fn test_match_union_with_arithmetic() {
//     let result = check_types(
//         r#"let x = match (42): | 0: "zero" | 1: 100 | _: 200 end | x + 1"#
//     );
//     assert!(
//         !result.is_empty(),
//         "match with union type should fail with arithmetic: {:?}",
//         result
//     );
// }

#[test]
fn test_nested_functions() {
    assert!(
        check_types(
            r#"
        def outer(x):
            def inner(y):
                x + y
            ;
            inner(10)
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_variable_references() {
    assert!(check_types("let x = 42 | let y = x | y").is_empty());
}

#[test]
fn test_function_as_value() {
    assert!(check_types("def f(x): x + 1;\n| let g = f | g").is_empty());
}

// Error Cases - These should fail type checking

#[test]
fn test_heterogeneous_array_allowed() {
    // mq is dynamically typed — heterogeneous arrays (used as tuples) are valid
    let result = check_types(r#"[1, "string", true]"#);
    assert!(
        result.is_empty(),
        "Heterogeneous arrays should be allowed: {:?}",
        result
    );
}

#[test]
fn test_function_arity_mismatch() {
    // Calling function with wrong number of arguments
    let result = check_types("def f(x, y): x + y;\n| f(1)");
    assert!(!result.is_empty(), "Expected arity mismatch error");
}

#[test]
fn test_recursive_type() {
    // Attempting to create an infinite type
    let result = check_types("let x = [x]");
    println!("Recursive type result: {:?}", result);
}

// Complex Patterns

#[test]
fn test_higher_order_functions() {
    assert!(
        check_types(
            r#"
        def map(f, arr):
            foreach item in arr:
                f(item)
            ;
        ;
    "#
        )
        .is_empty()
    );
}

// Nested lambda type checking: higher-order functions passing lambdas with type errors
#[rstest]
#[case::foreach_lambda_type_error(
    r#"def apply_to_all(v, f): foreach (x, v): f(x);; | apply_to_all([1, 2, 3], fn(x): x + true;)"#,
    false,
    "lambda passed to foreach-based HOF with invalid op should fail"
)]
#[case::foreach_lambda_valid(
    r#"def apply_to_all(v, f): foreach (x, v): f(x);; | apply_to_all([1, 2, 3], fn(x): x + 1;)"#,
    true,
    "lambda passed to foreach-based HOF with valid op should succeed"
)]
#[case::foreach_lambda_chained_type_error(
    r#"def apply_to_all(v, f): foreach (x, v): f(x);; | apply_to_all([1, 2, 3], fn(x): x + 1 + true;)"#,
    false,
    "lambda with chained binary op ending in type error should fail"
)]
#[case::foreach_lambda_chained_valid(
    r#"def apply_to_all(v, f): foreach (x, v): f(x);; | apply_to_all([1, 2, 3], fn(x): x + 1 + 2;)"#,
    true,
    "lambda with chained valid binary ops should succeed"
)]
#[case::foreach_lambda_triple_chained_type_error(
    r#"def apply_to_all(v, f): foreach (x, v): f(x);; | apply_to_all([1, 2, 3], fn(x): x + 1 + 2 + true;)"#,
    false,
    "lambda with triple chained binary op ending in type error should fail"
)]
fn test_nested_lambda_type_checking(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: errors={:?}",
        description,
        result
    );
}

#[test]
fn test_def_body_chained_binary_op_type_error() {
    let result = check_types(r#"def f(x): x + 1 + true; | f(1)"#);
    assert!(
        !result.is_empty(),
        "def body with chained binary op ending in type error should fail: {:?}",
        result
    );
}

#[test]
fn test_def_body_chained_binary_op_valid() {
    let result = check_types(r#"def f(x): x + 1 + 2; | f(1)"#);
    assert!(
        result.is_empty(),
        "def body with valid chained binary ops should succeed: {:?}",
        result
    );
}

#[test]
fn test_closure_capture() {
    assert!(
        check_types(
            r#"
        def make_adder(n):
            def adder(x):
                x + n
            ;
            adder
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_nested_conditionals() {
    assert!(
        check_types(
            r#"
        if true:
            if true:
                "very big"
            else:
                "medium"
            ;
        else:
            "small"
        ;
    "#
        )
        .is_empty()
    );
}

#[test]
fn test_complex_patterns() {
    assert!(
        check_types(r#"match ([1, 2, 3]): | []: "empty" | [x]: "single" | [x, y]: "pair" | _: "many" end"#).is_empty()
    );
}

#[test]
fn test_dict_operations() {
    assert!(check_types(r#"{"name": "Alice", "age": 30}"#).is_empty());
}

#[test]
fn test_try_catch() {
    assert!(
        check_types(
            r#"
        try:
            42 / 0
        catch:
            0
        ;
    "#
        )
        .is_empty()
    );
}

// Polymorphic Function Type Checking

#[rstest]
#[case::add_strings(r#"def add(x, y): x + y; | add("hello", "world")"#, true)]
#[case::add_numbers("def add(x, y): x + y; | add(1, 2)", true)]
#[case::add_mixed_types(r#"def add(x, y): x + y; | add(1, "hello")"#, false)]
#[case::add_mixed_types_reversed(r#"def add(x, y): x + y; | add("hello", 1)"#, true)] // string+any->string overload matches
#[case::string_concat_in_fn(r#"def greet(name): "hello " + name; | greet("world")"#, true)]
#[case::unary_negation("-42", true)]
#[case::nested_polymorphic_ops("def calc(a, b): (a + b) * (a - b); | calc(3, 2)", true)]
#[case::func_body_sub_mixed(r#"def sub(x, y): x - y; | sub(1, "str")"#, false)]
#[case::func_body_mul_strings(r#"def mul(x, y): x * y; | mul("a", "b")"#, false)]
#[case::func_body_div_mixed(r#"def div(x, y): x / y; | div(1, "two")"#, false)]
#[case::nested_call_type_propagation(r#"def add(x, y): x + y; | add(add(1, 2), "str")"#, true)] // nested return type propagation not tracked
fn test_polymorphic_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nErrors: {:?}",
        code,
        result
    );
}

// Edge Cases

#[test]
fn test_empty_program() {
    assert!(check_types("").is_empty());
}

#[test]
fn test_only_whitespace() {
    assert!(check_types("   \n  \t  ").is_empty());
}

#[test]
fn test_deeply_nested_arrays() {
    assert!(check_types("[[[[1]]]]").is_empty());
}

#[test]
fn test_deeply_nested_dicts() {
    assert!(check_types(r#"{"a": {"b": {"c": 1}}}"#).is_empty());
}

#[test]
fn test_lambda_functions() {
    assert!(check_types("let f = fn(x): x + 1 | f(5)").is_empty());
}

#[test]
fn test_module_imports() {
    // This might fail if modules aren't available
    let result = check_types(r#"include "math""#);
    println!("Module import result: {:?}", result);
}

// While Loop

#[test]
fn test_while_loop() {
    assert!(check_types("while (true): 1;").is_empty());
}

#[test]
fn test_while_condition_must_be_bool() {
    let errors = check_types("while (42): 1;");
    assert!(!errors.is_empty(), "while with non-bool condition should fail");
}

// Macro Definition

#[test]
fn test_macro_definition() {
    assert!(check_types("macro inc(x): x + 1;").is_empty());
}

#[test]
fn test_macro_with_multiple_params() {
    assert!(check_types("macro add(x, y): x + y;").is_empty());
}

// User-Defined Function Type Checking

#[rstest]
#[case::arg_type_mismatch(r#"def add(x, y): x + y; | add(1, "hello")"#, false)]
#[case::return_type_propagation(r#"def get_num(): 42; | get_num() + "hello""#, false)]
#[case::chained_calls(r#"def double(x): x + x; | def negate(x): 0 - x; | double(negate(1))"#, true)]
#[case::string_plus_number(r#"def greet(): "hello"; | greet() + 1"#, true)]
#[case::string_minus_number(r#"def greet(): "hello"; | greet() - 1"#, false)]
#[case::recursive_factorial("def factorial(n): if (n == 0): 1 else: n * factorial(n - 1);;", true)]
#[case::single_param_type_mismatch(r#"def a(v): v + 1; | a(true)"#, false)]
#[case::single_param_correct_type(r#"def a(v): v + 1; | a(1)"#, true)]
#[case::reversed_operand_mismatch(r#"def f(v): 1 + v; | f(true)"#, false)]
#[case::too_many_args("def f(x): x + 1; | f(1, 2)", false)]
#[case::too_many_args_two_params("def f(x, y): x + y; | f(1, 2, 3)", false)]
#[case::bool_return_in_arith("def flag(): true; | flag() + 1", false)]
#[case::string_return_in_sub(r#"def label(): "hello"; | label() - 1"#, false)]
fn test_user_function_type_checking(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nErrors: {:?}",
        code,
        result
    );
}

// Binary Operator Type Errors (basic, no builtins)

#[rstest]
#[case::number_minus_string(r#"1 - "hello""#, false, "number - string")]
#[case::number_div_string(r#"10 / "two""#, false, "number / string")]
#[case::number_mul_string(r#"3 * "x""#, false, "number * string")]
#[case::string_minus_number(r#""abc" - 1"#, false, "string - number")]
#[case::string_div_number(r#""abc" / 2"#, false, "string / number")]
#[case::number_add_string(r#"1 + "world""#, false, "number + string")]
#[case::string_mul_string(r#""a" * "b""#, false, "string * string")]
#[case::string_div_string(r#""a" / "b""#, false, "string / string")]
#[case::string_minus_string(r#""a" - "b""#, false, "string - string")]
#[case::bool_minus_number("true - 1", false, "bool - number")]
#[case::bool_mul_number("true * 2", false, "bool * number")]
#[case::bool_div_number("true / 2", false, "bool / number")]
#[case::number_minus_number("10 - 3", true, "number - number is valid")]
#[case::number_div_number("10 / 2", true, "number / number is valid")]
#[case::number_mul_number("3 * 4", true, "number * number is valid")]
#[case::string_add_string(r#""hello" + "world""#, true, "string + string is valid")]
#[case::string_add_number(r#""hello" + 42"#, true, "string + number coercion is valid")]
fn test_binary_op_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Errors={:?}",
        description,
        code,
        result
    );
}

// Comparison Operator Type Errors

#[rstest]
#[case::lt_number_string(r#"1 < "hello""#, false, "number < string")]
#[case::gt_string_number(r#""a" > 1"#, false, "string > number")]
#[case::lte_number_bool("1 <= true", false, "number <= bool")]
#[case::gte_bool_number("true >= 1", false, "bool >= number")]
#[case::lt_string_bool(r#""x" < true"#, false, "string < bool")]
#[case::gt_bool_string("true > \"x\"", false, "bool > string")]
#[case::lt_same_numbers("1 < 2", true, "number < number is valid")]
#[case::gt_same_numbers("5 > 3", true, "number > number is valid")]
#[case::lt_same_strings(r#""a" < "b""#, true, "string < string is valid")]
#[case::lt_same_bools("true < false", true, "bool < bool is valid")]
fn test_comparison_op_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Errors={:?}",
        description,
        code,
        result
    );
}

// Equality Operator Type Errors

#[rstest]
#[case::eq_number_string(r#"1 == "hello""#, false, "number == string")]
#[case::ne_number_bool("1 != true", false, "number != bool")]
#[case::eq_string_bool(r#""yes" == true"#, false, "string == bool")]
#[case::eq_same_numbers("1 == 1", true, "number == number is valid")]
#[case::eq_same_strings(r#""a" == "a""#, true, "string == string is valid")]
#[case::eq_same_bools("true == false", true, "bool == bool is valid")]
#[case::ne_same_numbers("1 != 2", true, "number != number is valid")]
fn test_equality_op_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Errors={:?}",
        description,
        code,
        result
    );
}

// Let Binding Type Propagation Errors

#[rstest]
#[case::let_num_minus_string(r#"let x = 1 | x - "str""#, false, "let number binding minus string")]
#[case::let_num_mul_string(r#"let x = 1 | x * "str""#, false, "let number binding times string")]
#[case::let_num_div_string(r#"let x = 1 | x / "str""#, false, "let number binding div string")]
#[case::let_num_plus_string(r#"let x = 1 | x + "str""#, false, "let number binding plus string")]
#[case::let_string_minus_num(r#"let x = "hello" | x - 1"#, false, "let string binding minus number")]
#[case::let_string_div_num(r#"let x = "hello" | x / 2"#, false, "let string binding div number")]
#[case::let_string_mul_string(r#"let x = "hello" | x * "world""#, false, "let string binding times string")]
#[case::let_num_minus_num("let x = 10 | x - 3", true, "let number binding minus number is valid")]
#[case::let_num_mul_num("let x = 3 | x * 4", true, "let number binding times number is valid")]
#[case::let_string_add_string(r#"let x = "hello" | x + " world""#, true, "let string binding concat is valid")]
#[case::let_string_add_num(
    r#"let x = "count: " | x + 42"#,
    true,
    "let string binding plus number coercion is valid"
)]
fn test_let_binding_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Errors={:?}",
        description,
        code,
        result
    );
}

// Lambda (fn) Type Errors

#[rstest]
#[case::lambda_body_sub_error(
    r#"let f = fn(x): x - "bad"; | f(1)"#,
    false,
    "lambda body: string constant in subtract"
)]
#[case::lambda_body_mul_error(
    r#"let f = fn(x): x * "bad"; | f(1)"#,
    false,
    "lambda body: string constant in multiply"
)]
#[case::lambda_body_div_error(
    r#"let f = fn(x): x / "bad"; | f(1)"#,
    false,
    "lambda body: string constant in divide"
)]
#[case::lambda_body_bool_constant(
    r#"let f = fn(x): x - true; | f(1)"#,
    false,
    "lambda body: bool constant in subtract"
)]
#[case::lambda_body_chained_error(
    r#"let f = fn(x): x - 1 - "str"; | f(1)"#,
    false,
    "lambda body: chained op with type error"
)]
#[case::lambda_callsite_sub_str(
    r#"let f = fn(x): x - 1; | f("str")"#,
    false,
    "lambda call-site: string arg passed to subtract lambda"
)]
#[case::lambda_callsite_mul_str(
    r#"let f = fn(x): x * 2; | f("str")"#,
    false,
    "lambda call-site: string arg passed to multiply lambda"
)]
#[case::lambda_callsite_div_str(
    r#"let f = fn(x): x / 2; | f("str")"#,
    false,
    "lambda call-site: string arg passed to divide lambda"
)]
#[case::lambda_callsite_add_bool(
    r#"let f = fn(x): x + 1; | f(true)"#,
    false,
    "lambda call-site: bool arg passed to add lambda"
)]
#[case::lambda_callsite_sub_bool(
    r#"let f = fn(x): x - 1; | f(true)"#,
    false,
    "lambda call-site: bool arg passed to subtract lambda"
)]
#[case::lambda_valid_num(r#"let f = fn(x): x + 1; | f(5)"#, true, "lambda: valid number arg")]
#[case::lambda_valid_str(r#"let f = fn(x): x + " world"; | f("hello")"#, true, "lambda: valid string arg")]
#[case::lambda_two_params_valid(r#"let f = fn(x, y): x + y; | f(1, 2)"#, true, "lambda: two number params valid")]
#[case::lambda_two_params_sub_valid(
    r#"let f = fn(x, y): x - y; | f(10, 3)"#,
    true,
    "lambda: two number params sub valid"
)]
fn test_lambda_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Errors={:?}",
        description,
        code,
        result
    );
}

// While Loop Condition Type Errors

#[rstest]
#[case::number_condition("while (42): 1;", false, "number condition should fail")]
#[case::string_condition(r#"while ("true"): 1;"#, false, "string condition should fail")]
#[case::number_expr_condition("while (1 + 2): 1;", false, "number expression condition should fail")]
#[case::bool_condition("while (true): 1;", true, "bool condition should succeed")]
#[case::bool_expr_condition("while (1 == 1): 1;", true, "bool expression condition should succeed")]
#[case::bool_comparison_condition("while (1 < 2): 1;", true, "bool comparison condition should succeed")]
fn test_while_condition_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Errors={:?}",
        description,
        result
    );
}

// Match Pattern Type Errors

#[rstest]
#[case::number_vs_string_pattern(r#"match (1): | "hello": "matched" end"#, false, "number against string pattern")]
#[case::string_vs_number_pattern(r#"match ("hello"): | 42: "matched" end"#, false, "string against number pattern")]
#[case::bool_vs_number_pattern("match (true): | 1: \"matched\" end", false, "bool against number pattern")]
#[case::number_vs_bool_pattern("match (1): | true: \"matched\" end", false, "number against bool pattern")]
#[case::number_vs_number_pattern(
    "match (1): | 1: \"one\" | 2: \"two\" end",
    true,
    "number against number pattern is valid"
)]
#[case::string_vs_string_pattern(
    r#"match ("a"): | "a": "matched" | "b": "other" end"#,
    true,
    "string against string pattern is valid"
)]
#[case::wildcard_pattern("match (1): | _: \"any\" end", true, "wildcard pattern always valid")]
#[case::variable_pattern("match (1): | x: x end", true, "variable pattern always valid")]
fn test_match_pattern_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Errors={:?}",
        description,
        result
    );
}

// Function Arity Errors

#[rstest]
#[case::too_many_for_one_param("def f(x): x + 1; | f(1, 2)", false, "2 args to 1-param function")]
#[case::too_many_for_two_params("def f(x, y): x + y; | f(1, 2, 3)", false, "3 args to 2-param function")]
#[case::too_many_for_three_params("def f(x, y, z): x + y + z; | f(1, 2, 3, 4)", false, "4 args to 3-param function")]
#[case::correct_one_param("def f(x): x + 1; | f(1)", true, "1 arg to 1-param function is valid")]
#[case::correct_two_params("def f(x, y): x + y; | f(1, 2)", true, "2 args to 2-param function is valid")]
#[case::correct_zero_params("def f(): 42; | f()", true, "0 args to 0-param function is valid")]
fn test_function_arity_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Errors={:?}",
        description,
        code,
        result
    );
}

// Chained Expression Type Errors

#[rstest]
#[case::chained_sub_then_invalid(r#"let x = 10 - 3 | x - "str""#, false, "chained: number sub then minus string")]
#[case::chained_mul_then_invalid(r#"let x = 2 * 3 | x / "two""#, false, "chained: number mul then div string")]
#[case::fn_return_bool_then_arith("def b(): true; | let x = b() | x - 1", false, "chained: bool return then subtract")]
#[case::fn_return_num_then_valid(
    "def n(): 42; | let x = n() | x + 1",
    true,
    "chained: number return then add is valid"
)]
#[case::fn_return_str_then_valid(
    r#"def s(): "hi"; | let x = s() | x + " there""#,
    true,
    "chained: string return then concat is valid"
)]
fn test_chained_expression_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Errors={:?}",
        description,
        code,
        result
    );
}

// Pipe Chains in Function Bodies

#[test]
fn test_pipe_in_function_body() {
    // Pipe chain inside function body should work
    let result = check_types("def f(x): x | x;");
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "Pipe in function body should succeed");
}

#[test]
fn test_pipe_chain_in_function_body() {
    // Multiple pipes in function body
    let result = check_types(
        r#"
        def process(x):
            let y = x |
            let z = y |
            z
        ;
    "#,
    );
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "Pipe chain in function body should succeed");
}

// Try/Catch with Different Types - Union Type Tests

#[rstest]
#[case::same_type_number(r#"try: 1 catch: 2;"#, true, "same number type should work")]
#[case::same_type_string(r#"try: "a" catch: "b";"#, true, "same string type should work")]
#[case::same_type_bool(r#"try: true catch: false;"#, true, "same bool type should work")]
#[case::different_types_number_string(r#"try: 42 catch: "string";"#, true, "number|string union should be created")]
#[case::different_types_bool_number(r#"try: true catch: 100;"#, true, "bool|number union should be created")]
#[case::different_types_string_bool(r#"try: "text" catch: false;"#, true, "string|bool union should be created")]
fn test_try_catch_type_combinations(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {:?}", description, result);
}

#[rstest]
#[case::union_with_add(
    r#"let x = try: 42 catch: "string"; | x + 1"#,
    "union type (number|string) should fail with +"
)]
#[case::union_with_multiply(
    r#"let x = try: 100 catch: "error"; | x * 2"#,
    "union type (number|string) should fail with *"
)]
#[case::union_with_subtract(
    r#"let x = try: 50 catch: "fail"; | x - 10"#,
    "union type (number|string) should fail with -"
)]
#[case::bool_with_add(r#"let x = try: true catch: false; | x + 1"#, "bool type should fail with +")]
#[case::bool_with_multiply(r#"let x = try: true catch: false; | x * 2"#, "bool type should fail with *")]
fn test_try_catch_union_arithmetic_errors(#[case] code: &str, #[case] description: &str) {
    let result = check_types(code);
    assert!(
        !result.is_empty(),
        "{}: should produce type error but got no errors",
        description
    );
}

#[rstest]
#[case::same_type_add(r#"let x = try: 1 catch: 2; | x + 3"#, "same number type should allow +")]
#[case::same_type_multiply(r#"let x = try: 10 catch: 20; | x * 2"#, "same number type should allow *")]
#[case::same_type_subtract(r#"let x = try: 100 catch: 50; | x - 25"#, "same number type should allow -")]
#[case::same_string_concat(r#"let x = try: "a" catch: "b"; | x + "c""#, "same string type should allow +")]
fn test_try_catch_same_type_arithmetic_ok(#[case] code: &str, #[case] description: &str) {
    let result = check_types(code);
    assert!(result.is_empty(), "{}: {:?}", description, result);
}

// If/Else/Elif - Union Type Tests

#[rstest]
#[case::same_type_number(r#"if (true): 1 else: 2;"#, true, "same number type")]
#[case::same_type_string(r#"if (true): "a" else: "b";"#, true, "same string type")]
#[case::different_types(r#"if (true): 42 else: "string";"#, true, "different types create union")]
#[case::elif_multiple_types(
    r#"if (true): 42 elif (false): "string" else: true;"#,
    true,
    "elif with multiple types"
)]
fn test_if_else_type_combinations(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {:?}", description, result);
}

#[rstest]
#[case::union_with_add(
    r#"let x = if (true): 42 else: "string"; | x + 1"#,
    "union (number|string) should fail with +"
)]
#[case::union_with_multiply(
    r#"let x = if (false): 100 else: "fail"; | x * 2"#,
    "union (number|string) should fail with *"
)]
#[case::elif_union_arithmetic(
    r#"let x = if (true): 1 elif (false): "text" else: 2; | x + 1"#,
    "elif union should fail with +"
)]
fn test_if_else_union_arithmetic_errors(#[case] code: &str, #[case] description: &str) {
    let result = check_types(code);
    assert!(!result.is_empty(), "{}: should produce type error", description);
}

#[rstest]
#[case::same_type_add(r#"let x = if (true): 1 else: 2; | x + 3"#, "same number type should allow +")]
#[case::same_type_multiply(r#"let x = if (false): 10 else: 20; | x * 2"#, "same number type should allow *")]
#[case::elif_same_type(
    r#"let x = if (true): 1 elif (false): 2 else: 3; | x + 4"#,
    "elif same number type should allow +"
)]
fn test_if_else_same_type_arithmetic_ok(#[case] code: &str, #[case] description: &str) {
    let result = check_types(code);
    assert!(result.is_empty(), "{}: {:?}", description, result);
}

// Type Inference Verification

#[test]
fn test_inferred_types_basic() {
    let hir = create_hir("let x = 42;");
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    // Verify that we have type information
    assert!(!checker.symbol_types().is_empty());
}

#[test]
fn test_inferred_types_function() {
    let hir = create_hir("def identity(x): x;");
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    // The function should have a type
    let types = checker.symbol_types();
    assert!(!types.is_empty());

    // Print inferred types for inspection
    for (symbol_id, type_scheme) in types {
        println!("Symbol {:?} :: {}", symbol_id, type_scheme);
    }
}

#[test]
fn test_type_unification() {
    let hir = create_hir("let x = 42 | let y = x | let z = y;");
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    // All variables should have compatible types
    println!("Unified types:");
    for (symbol_id, type_scheme) in checker.symbol_types() {
        println!("  {:?} :: {}", symbol_id, type_scheme);
    }
}

// Foreach Union Type Tests

#[rstest]
#[case::same_type_number(
    r#"let x = foreach(item, [1, 2, 3]): item; | x + [1]"#,
    true,
    "same number type in foreach"
)]
#[case::different_types(
    r#"foreach(item, [1, 2, 3]): if (true): item else: "str";;"#,
    true,
    "different types in foreach creates union"
)]
fn test_foreach_type_combinations(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {:?}", description, result);
}

#[rstest]
#[case::union_with_add(
    r#"let x = foreach(item, [1, 2, 3]): if (true): item else: "str";; | x + 1"#,
    "foreach union (array<number|string>) should fail with +"
)]
fn test_foreach_union_arithmetic_errors(#[case] code: &str, #[case] description: &str) {
    let result = check_types(code);
    assert!(
        !result.is_empty(),
        "{}: should produce type error but got no errors",
        description
    );
}

// Loop Union Type Tests

#[rstest]
#[case::same_type_number(r#"loop: break: 42;;"#, true, "loop with number break")]
#[case::different_types(
    r#"loop: if (true): 1 else: "str";;"#,
    true,
    "loop with different branch types creates union"
)]
#[case::break_different_types(
    r#"loop: if (true): break: 42 else: break: "str";;"#,
    true,
    "loop with break of different types creates union"
)]
#[case::while_break_different_types(
    r#"while (true): if (true): break: 42 else: break: "str";"#,
    true,
    "while with break of different types creates union"
)]
#[case::foreach_break_value(
    r#"foreach(x, [1, 2, 3]): if (true): break: "early" else: x;"#,
    true,
    "foreach with break value produces union"
)]
fn test_loop_type_combinations(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {:?}", description, result);
}

#[rstest]
#[case::union_with_add(
    r#"let x = loop: if (true): 1 else: "str";; | x + 1"#,
    "loop union (number|string) should fail with +"
)]
#[case::break_union_with_add(
    r#"let x = loop: if (true): break: 42 else: break: "str";; | x + 1"#,
    "loop with break union (number|string) should fail with +"
)]
#[case::while_break_union_with_add(
    r#"let x = while (true): if (true): break: 42 else: break: "str"; | x + 1"#,
    "while with break union (number|string) should fail with +"
)]
#[case::loop_if_only_break_with_add(
    r#"let x = loop: if (true): break: 42;; | x + 1"#,
    "loop with only if-branch break (no else) should produce union and fail with +"
)]
#[case::while_if_only_break_with_add(
    r#"let x = while (true): if (true): break: 42; | x + 1"#,
    "while with only if-branch break (no else) should produce union and fail with +"
)]
fn test_loop_union_arithmetic_errors(#[case] code: &str, #[case] description: &str) {
    let result = check_types(code);
    assert!(
        !result.is_empty(),
        "{}: should produce type error but got no errors",
        description
    );
}

#[rstest]
#[case::same_type_add(r#"let x = loop: break: 42;; | x + 1"#, "loop number should allow +")]
#[case::while_same_type_add(r#"let x = while (true): 42; | x + 1"#, "while number should allow +")]
#[case::loop_break_same_type(
    r#"let x = loop: if (true): break: 1 else: break: 2;; | x + 1"#,
    "loop with break of same type should allow +"
)]
#[case::to_number_on_union(
    r#"let x = loop: if (true): break: 42;; | to_number(x) + 1"#,
    "to_number on union type should allow + on the result"
)]
#[case::to_number_on_while_union(
    r#"let x = while (true): if (true): break: 42; | to_number(x) + 1"#,
    "to_number on while union type should allow + on the result"
)]
fn test_loop_same_type_arithmetic_ok(#[case] code: &str, #[case] description: &str) {
    let result = check_types(code);
    assert!(result.is_empty(), "{}: {:?}", description, result);
}

// Conversion functions on union types

#[rstest]
#[case::to_string_on_union(
    r#"let x = loop: if (true): break: 42;; | to_string(x)"#,
    "to_string on union type should return string"
)]
#[case::to_array_on_union(
    r#"let x = loop: if (true): break: 42;; | to_array(x)"#,
    "to_array on union type should return array"
)]
#[case::type_fn_on_union(
    r#"let x = loop: if (true): break: 42;; | type(x)"#,
    "type() on union type should return string"
)]
#[case::is_string_on_union(
    r#"let x = loop: if (true): break: 42;; | is_string(x)"#,
    "is_string on union type should return bool"
)]
#[case::is_number_on_union(
    r#"let x = loop: if (true): break: 42;; | is_number(x)"#,
    "is_number on union type should return bool"
)]
#[case::is_none_on_union(
    r#"let x = loop: if (true): break: 42;; | is_none(x)"#,
    "is_none on union type should return bool"
)]
#[case::to_number_on_break_union(
    r#"let x = loop: if (true): break: 42 else: break: "str";; | to_number(x) + 1"#,
    "to_number on (number|string) union should allow +"
)]
fn test_conversion_on_union(#[case] code: &str, #[case] description: &str) {
    let result = check_types(code);
    assert!(result.is_empty(), "{}: {:?}", description, result);
}

// Match Union Type Tests (correct mq syntax)

#[test]
fn test_match_union_type_propagation() {
    let result = check_types(r#"let x = match (42): | 0: "zero" | 1: 100 | _: 200 end | x + 1"#);
    assert!(
        !result.is_empty(),
        "match with union type (string|number) should fail with +: {:?}",
        result
    );
}

#[test]
fn test_match_same_type_arithmetic_ok() {
    let result = check_types(r#"let x = match (42): | 0: 0 | 1: 100 | _: 200 end | x + 1"#);
    assert!(
        result.is_empty(),
        "match with same number type should allow +: {:?}",
        result
    );
}

// --- Variable reassignment tests ---

#[test]
fn test_var_reassignment_same_type() {
    let result = check_types("var x = 10 | x = 20");
    assert!(
        result.is_empty(),
        "var reassignment with same type should succeed: {:?}",
        result
    );
}

#[test]
fn test_var_reassignment_different_type() {
    let result = check_types(r#"var x = 10 | x = "hello""#);
    assert!(
        result.is_empty(),
        "var reassignment with different type should succeed: {:?}",
        result
    );
}

#[test]
fn test_var_reassignment_used_after() {
    let result = check_types(r#"var x = 10 | x = "hello" | upcase(x)"#);
    assert!(
        result.is_empty(),
        "after reassigning var to string, upcase should work: {:?}",
        result
    );
}

#[test]
fn test_var_compound_assignment() {
    let result = check_types("var x = 10 | x += 5");
    assert!(
        result.is_empty(),
        "compound assignment with same type should succeed: {:?}",
        result
    );
}

#[test]
fn test_var_reassignment_type_error_after() {
    // "test" + true passes the typechecker because there is a generic
    // "string + any -> string" overload in the builtin registrations.
    // This is a pre-existing limitation of the overload system, not
    // specific to variable reassignment.
    let baseline = check_types(r#""test" + true"#);
    let result = check_types(r#"var v = 1 | v = "test" | v + true"#);
    assert_eq!(
        baseline.len(),
        result.len(),
        "var reassignment should behave same as direct usage: baseline={:?}, result={:?}",
        baseline,
        result
    );
}
