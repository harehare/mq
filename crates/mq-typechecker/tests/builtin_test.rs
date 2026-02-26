//! Tests for builtin function type checking using rstest parameterized tests

use mq_hir::Hir;
use mq_typechecker::{TypeChecker, TypeError};
use rstest::rstest;

/// Helper function to create HIR from code
fn create_hir(code: &str) -> Hir {
    let mut hir = Hir::default();
    // Enable builtins to test builtin function types
    hir.builtin.disabled = false;
    hir.add_builtin(); // Add builtin functions to HIR
    hir.add_code(None, code);
    hir
}

/// Helper function to run type checker
fn check_types(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::new();
    checker.check(&hir)
}

// Mathematical Functions

#[rstest]
#[case::abs("abs(42)", true)]
#[case::abs_negative("abs(-10)", true)]
#[case::ceil("ceil(3.14)", true)]
#[case::floor("floor(3.14)", true)]
#[case::round("round(3.14)", true)]
#[case::trunc("trunc(3.14)", true)]
#[case::abs_string("abs(\"hello\")", false)] // Should fail: wrong type
fn test_unary_math_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::add_numbers("1 + 2", true)]
#[case::add_strings("\"hello\" + \"world\"", true)]
#[case::add_string_number("\"value: \" + 42", true)] // String + number coercion
#[case::add_arrays("[1, 2] + [3, 4]", true)] // Array concatenation
#[case::add_mixed("1 + \"world\"", false)] // Should fail: type mismatch
#[case::sub("10 - 5", true)]
#[case::mul("3 * 4", true)]
#[case::mul_array_repeat("[1, 2] * 3", true)] // Array repetition
#[case::div("10 / 2", true)]
#[case::mod_op("10 % 3", true)]
#[case::pow("2 ^ 8", true)]
fn test_arithmetic_operators(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::lt("5 < 10", true)]
#[case::gt("10 > 5", true)]
#[case::lte("5 <= 10", true)]
#[case::gte("10 >= 5", true)]
#[case::lt_string("\"a\" < \"b\"", true)] // String comparison
#[case::gt_string("\"z\" > \"a\"", true)] // String comparison
#[case::lt_bool("true < false", true)] // Bool comparison (false < true)
#[case::lt_mixed("\"a\" < 1", false)] // Should fail: mixed types
fn test_comparison_operators(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::eq_numbers("1 == 1", true)]
#[case::eq_strings("\"a\" == \"b\"", true)]
#[case::ne_numbers("1 != 2", true)]
#[case::ne_strings("\"a\" != \"b\"", true)]
fn test_equality_operators(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::min_numbers("min(1, 2)", true)]
#[case::max_numbers("max(1, 2)", true)]
#[case::min_strings("min(\"a\", \"b\")", true)]
#[case::max_strings("max(\"a\", \"b\")", true)]
fn test_min_max(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::and_op("true && false", true)]
#[case::or_op("true || false", true)]
#[case::not_op("!true", true)]
#[case::bang_op("!false", true)]
#[case::and_number("1 && 2", true)] // mq supports truthy/falsy semantics
fn test_logical_operators(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::nan("nan()", true)]
#[case::infinite("infinite()", true)]
#[case::is_nan("is_nan(1.0)", true)]
fn test_special_number_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// ============================================================================
// STRING FUNCTIONS
// ============================================================================

#[rstest]
#[case::downcase("downcase(\"HELLO\")", true)]
#[case::upcase("upcase(\"hello\")", true)]
#[case::trim("trim(\"  hello  \")", true)]
#[case::downcase_number("downcase(42)", false)] // Should fail: wrong type
fn test_string_case_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::starts_with("starts_with(\"hello\", \"he\")", true)]
#[case::ends_with("ends_with(\"hello\", \"lo\")", true)]
#[case::index("index(\"hello\", \"ll\")", true)]
#[case::rindex("rindex(\"hello\", \"l\")", true)]
fn test_string_search_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::replace("replace(\"hello\", \"l\", \"r\")", true)]
#[case::gsub("gsub(\"hello\", \"l\", \"r\")", true)]
#[case::split("split(\"a,b,c\", \",\")", true)]
#[case::join("join([\"a\", \"b\"], \",\")", true)]
fn test_string_manipulation_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::explode("explode(\"hello\")", true)]
#[case::implode("implode([104, 101, 108, 108, 111])", true)]
#[case::utf8bytelen("utf8bytelen(\"hello\")", true)]
fn test_string_codepoint_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::regex_match("regex_match(\"hello123\", \"[0-9]+\")", true)]
#[case::base64("base64(\"hello\")", true)]
#[case::base64d("base64d(\"aGVsbG8=\")", true)]
#[case::url_encode("url_encode(\"hello world\")", true)]
fn test_string_encoding_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Array Functions

#[rstest]
#[case::flatten("flatten([[1, 2], [3, 4]])", true)]
#[case::reverse("reverse([1, 2, 3])", true)]
#[case::sort("sort([3, 1, 2])", true)]
#[case::uniq("uniq([1, 2, 2, 3])", true)]
#[case::compact("compact([1, none, 2])", true)]
fn test_array_manipulation_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::len_array("len([1, 2, 3])", true)]
#[case::len_string("len(\"hello\")", true)]
#[case::slice("slice([1, 2, 3, 4], 1, 3)", true)]
#[case::insert("insert([1, 3], 1, 2)", true)]
fn test_array_access_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::range_two_args("range(1, 5)", true)]
#[case::range_three_args("range(1, 10, 2)", true)]
#[case::repeat("repeat(\"x\", 3)", true)]
fn test_array_creation_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Dictionary Functions

#[rstest]
#[case::keys("keys({\"a\": 1, \"b\": 2})", true)]
#[case::values("values({\"a\": 1, \"b\": 2})", true)]
#[case::entries("entries({\"a\": 1, \"b\": 2})", true)]
fn test_dict_query_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

#[rstest]
#[case::get("get({\"a\": 1}, \"a\")", true)]
#[case::set("set({\"a\": 1}, \"b\", 2)", true)]
#[case::del("del({\"a\": 1, \"b\": 2}, \"a\")", true)]
#[case::update("update({\"a\": 1}, {\"b\": 2})", true)]
fn test_dict_manipulation_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Type Conversion Functions

#[rstest]
#[case::to_number("to_number(\"42\")", true)]
#[case::to_string("to_string(42)", true)]
#[case::to_array("to_array(42)", true)]
#[case::type_of("type(42)", true)]
fn test_type_conversion_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Date/Time Functions

#[rstest]
#[case::now("now()", true)]
#[case::from_date("from_date(\"2024-01-01\")", true)]
#[case::to_date("to_date(1704067200000, \"%Y-%m-%d\")", true)]
fn test_datetime_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// I/O And Utility Functions

#[rstest]
#[case::print("print(42)", true)]
#[case::stderr("stderr(\"error\")", true)]
#[case::input("input()", true)]
fn test_io_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Complex Expressions With Builtins

#[rstest]
#[case::chained_string_ops("upcase(trim(\"  hello  \"))", true)]
#[case::math_expression("abs(min(-5, -10) + max(3, 7))", true)]
#[case::array_pipeline("len(reverse(sort([3, 1, 2])))", true)]
#[case::mixed_operations("to_string(len(split(\"a,b,c\", \",\")))", true)]
fn test_complex_builtin_expressions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Pipe Type Propagation

#[rstest]
#[case::string_to_upcase("\"hello\" | upcase", true)]
#[case::string_to_trim("\"  hello  \" | trim", true)]
#[case::number_to_abs("-42 | abs", true)]
#[case::string_to_len("\"hello\" | len", true)]
#[case::chained_pipes("\"  hello  \" | trim | upcase", true)]
#[case::chained_string_to_len("\"hello\" | upcase | len", true)]
#[case::chained_split_to_len("\"hello\" | split(\",\") | len", true)]
#[case::chained_array_reverse_first("[1,2,3] | reverse | first", true)]
#[case::number_to_upcase("42 | upcase", false)] // Number piped to string function
fn test_pipe_type_propagation(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Type Error Cases

// --- Arithmetic error cases ---
#[rstest]
#[case::add_number_string("1 + \"string\"", false)] // number + string is invalid
#[case::add_bool_number("true + 1", false)] // bool + number is invalid
#[case::add_bool_string("true + \"s\"", false)] // bool + string is invalid
#[case::sub_strings("\"a\" - \"b\"", false)] // strings cannot be subtracted
#[case::sub_string_number("\"a\" - 1", false)] // string - number is invalid
#[case::mul_string_number("\"a\" * 3", false)] // string * number is invalid
#[case::div_strings("\"a\" / \"b\"", false)] // strings cannot be divided
#[case::mod_strings("\"a\" % \"b\"", false)] // strings cannot use modulo
#[case::sub_bool_bool("true - false", false)] // booleans cannot be subtracted
fn test_arithmetic_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// --- Comparison error cases ---
#[rstest]
#[case::lt_string_number("\"a\" < 1", false)] // mixed types
#[case::gt_number_string("1 > \"a\"", false)] // mixed types
#[case::lte_bool_number("true <= 1", false)] // mixed types
#[case::gte_string_bool("\"a\" >= true", false)] // mixed types
fn test_comparison_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// --- Logical error cases ---
#[rstest]
#[case::not_number("!42", false)] // number is not bool
fn test_logical_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// --- Function arity/type error cases ---
#[rstest]
#[case::abs_string("abs(\"not a number\")", false)] // wrong argument type
#[case::downcase_number("downcase(42)", false)] // wrong argument type
#[case::ceil_string("ceil(\"hello\")", false)] // wrong argument type
#[case::floor_bool("floor(true)", false)] // wrong argument type
#[case::len_no_args("len()", false)] // missing argument
#[case::split_numbers("split(1, 2)", false)] // wrong argument types
#[case::join_numbers("join(1, 2)", false)] // wrong argument types
#[case::starts_with_numbers("starts_with(1, 2)", false)] // wrong argument types
fn test_function_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// --- Pipe type error cases ---
#[rstest]
#[case::number_to_upcase("42 | upcase", false)] // number piped to string function
#[case::number_to_trim("42 | trim", false)] // number piped to string function
#[case::string_to_abs("\"hello\" | abs", false)] // string piped to number function
#[case::string_to_ceil("\"hello\" | ceil", false)] // string piped to number function
fn test_pipe_type_errors(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// --- Piped function calls (pipe value as implicit first argument) ---
#[rstest]
#[case::piped_join("[\"a\", \"b\"] | join(\",\")", true)] // array piped as first arg to join
#[case::piped_split("\"a,b,c\" | split(\",\")", true)] // string piped as first arg to split
#[case::piped_starts_with("\"hello\" | starts_with(\"he\")", true)]
#[case::piped_ends_with("\"hello\" | ends_with(\"lo\")", true)]
#[case::piped_index("\"hello\" | index(\"ll\")", true)]
#[case::piped_replace("\"hello\" | replace(\"l\", \"r\")", true)]
#[case::piped_gsub("\"hello\" | gsub(\"l\", \"r\")", true)]
#[case::piped_slice("[1, 2, 3, 4] | slice(1, 3)", true)]
#[case::piped_repeat("\"x\" | repeat(3)", true)]
#[case::piped_join_wrong_type("42 | join(\",\")", false)] // number piped to join (expects array)
#[case::piped_split_wrong_type("42 | split(\",\")", false)] // number piped to split (expects string)
fn test_piped_function_calls(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}
