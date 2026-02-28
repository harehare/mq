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

#[rstest]
#[case::downcase("downcase(\"HELLO\")", true)]
#[case::upcase("upcase(\"hello\")", true)]
#[case::trim("trim(\"  hello  \")", true)]
#[case::downcase_number("downcase(42)", false)] // Should fail: wrong type
#[case::rtrim("rtrim(\"  hello  \")", true)]
#[case::rindex("rindex(\"hello world hello\", \"hello\")", true)]
#[case::capture("capture(\"hello 42\", \"(?P<word>\\\\w+)\")", true)]
#[case::is_regex_match("is_regex_match(\"hello123\", \"[0-9]+\")", true)]
#[case::base64url("base64url(\"hello\")", true)]
#[case::base64urld("base64urld(\"aGVsbG8=\")", true)]
#[case::ltrim_number("ltrim(42)", false)]
#[case::rtrim_number("rtrim(42)", false)]
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
#[case::get_array("get([1, 2, 3], 0)", true)]
#[case::get_generic_binary("let d = dict() | get(d, \"key\")", true)]
#[case::get_generic_ternary("let d = dict() | get(d, \"key\")[\"name\"]", true)]
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

// Collection Functions

#[rstest]
#[case::first_array("first([1, 2, 3])", true)]
#[case::last_array("last([1, 2, 3])", true)]
#[case::first_string("first(\"hello\")", true)]
#[case::last_string("last(\"hello\")", true)]
#[case::contains_string("contains(\"hello world\", \"world\")", true)]
#[case::contains_array("contains([1, 2, 3], 2)", true)]
#[case::contains_dict("contains({\"a\": 1, \"b\": 2}, \"a\")", true)]
#[case::in_array("in([1, 2, 3], 1)", true)]
#[case::in_string("in(\"hello world\", \"world\")", true)]
#[case::in_sub_array("in([1, 2, 3, 4], [2, 3])", true)]
fn test_collection_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Higher-Order Collection Functions

#[rstest]
#[case::map_array("map([1, 2, 3], fn(x): x + 1;)", true)]
#[case::filter_array("filter([1, 2, 3], fn(x): x > 1;)", true)]
#[case::fold_array("fold([1, 2, 3], 0, fn(acc, x): acc + x;)", true)]
#[case::sort_by_array("sort_by([1, 2, 3], fn(x): x;)", true)]
#[case::flat_map_array("flat_map([1, 2, 3], fn(x): [x];)", true)]
fn test_higher_order_collection_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// None Propagation for Higher-Order Functions

#[rstest]
#[case::filter_none_identity("filter(None, fn(x): x;)", true)]
#[case::filter_none_array_return("filter(None, fn(x): [x[0], x[1]];)", true)]
#[case::flat_map_none("flat_map(None, fn(x): [x];)", true)]
#[case::map_none_identity("map(None, fn(x): x;)", true)]
fn test_none_propagation_higher_order(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Type Check Functions

#[rstest]
#[case::is_none("is_none(42)", true)]
#[case::is_array("is_array([1,2,3])", true)]
#[case::is_dict("is_dict({\"a\": 1})", true)]
#[case::is_string("is_string(\"hello\")", true)]
#[case::is_number("is_number(42)", true)]
#[case::is_bool("is_bool(true)", true)]
#[case::is_empty_string("is_empty(\"\")", true)]
#[case::is_empty_array("is_empty([])", true)]
fn test_type_check_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Utility Functions

#[rstest]
#[case::coalesce("coalesce(1, 2)", true)]
#[case::error_func("error(\"message\")", true)]
#[case::halt_func("halt(1)", true)]
#[case::all_symbols("all_symbols()", true)]
#[case::get_variable("get_variable(\"key\")", true)]
#[case::set_variable("set_variable(\"key\", \"value\")", true)]
#[case::intern("intern(\"symbol\")", true)]
#[case::is_debug_mode("is_debug_mode()", true)]
#[case::breakpoint("breakpoint()", true)]
#[case::assert_func("assert(42)", true)]
#[case::assert_func_bool("assert(true)", true)]
#[case::negate_func("negate(5)", true)]
#[case::pow_func("pow(2, 8)", true)]
#[case::convert_at_operator("\"hello\" @ :text", true)]
#[case::convert_func("convert(\"hello\", :text)", true)]
fn test_utility_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Numeric Bit-Shift Functions

#[rstest]
#[case::shift_left_numbers("shift_left(3, 1)", true)]
#[case::shift_right_numbers("shift_right(8, 2)", true)]
#[case::shift_left_wrong_second_arg("shift_left(\"hello\", \"str\")", false)]
#[case::shift_right_wrong_second_arg("shift_right(\"hello\", \"str\")", false)]
fn test_bitshift_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Markdown Type Check Functions

#[rstest]
#[case::is_h("to_markdown(\"# hello\") | is_h", true)]
#[case::is_p("to_markdown(\"hello\") | is_p", true)]
#[case::is_code("to_markdown(\"hello\") | is_code", true)]
#[case::is_code_block("to_markdown(\"hello\") | is_code_block", true)]
#[case::is_code_inline("to_markdown(\"hello\") | is_code_inline", true)]
#[case::is_em("to_markdown(\"hello\") | is_em", true)]
#[case::is_strong("to_markdown(\"hello\") | is_strong", true)]
#[case::is_link("to_markdown(\"hello\") | is_link", true)]
#[case::is_image("to_markdown(\"hello\") | is_image", true)]
#[case::is_list("to_markdown(\"hello\") | is_list", true)]
#[case::is_list_item("to_markdown(\"hello\") | is_list_item", true)]
#[case::is_table("to_markdown(\"hello\") | is_table", true)]
#[case::is_table_row("to_markdown(\"hello\") | is_table_row", true)]
#[case::is_table_cell("to_markdown(\"hello\") | is_table_cell", true)]
#[case::is_blockquote("to_markdown(\"hello\") | is_blockquote", true)]
#[case::is_hr("to_markdown(\"hello\") | is_hr", true)]
#[case::is_html("to_markdown(\"hello\") | is_html", true)]
#[case::is_text("to_markdown(\"hello\") | is_text", true)]
#[case::is_softbreak("to_markdown(\"hello\") | is_softbreak", true)]
#[case::is_hardbreak("to_markdown(\"hello\") | is_hardbreak", true)]
#[case::is_task_list_item("to_markdown(\"hello\") | is_task_list_item", true)]
#[case::is_footnote("to_markdown(\"hello\") | is_footnote", true)]
#[case::is_footnote_ref("to_markdown(\"hello\") | is_footnote_ref", true)]
#[case::is_strikethrough("to_markdown(\"hello\") | is_strikethrough", true)]
#[case::is_math("to_markdown(\"hello\") | is_math", true)]
#[case::is_math_inline("to_markdown(\"hello\") | is_math_inline", true)]
#[case::is_toml("to_markdown(\"hello\") | is_toml", true)]
#[case::is_yaml("to_markdown(\"hello\") | is_yaml", true)]
#[case::is_h_level("to_markdown(\"# hello\") | is_h_level(1)", true)]
#[case::is_h_level_wrong_type("is_h_level(42, \"str\")", false)]
fn test_markdown_type_check_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Markdown Conversion Functions

#[rstest]
#[case::to_markdown("to_markdown(\"hello\")", true)]
#[case::to_text("to_markdown(\"hello\") | to_text", true)]
#[case::to_html("to_markdown(\"hello\") | to_html", true)]
#[case::to_markdown_string("to_markdown(\"hello\") | to_markdown_string", true)]
#[case::to_code("to_markdown(\"hello\") | to_code(\"rust\")", true)]
#[case::to_code_inline("to_markdown(\"hello\") | to_code_inline", true)]
#[case::to_h("to_markdown(\"hello\") | to_h(1)", true)]
#[case::to_hr("to_hr()", true)]
#[case::to_image("to_image(\"alt\", \"url\", \"title\")", true)]
#[case::to_link("to_link(\"text\", \"url\", \"title\")", true)]
#[case::to_md_list("to_markdown(\"hello\") | to_md_list(1)", true)]
#[case::to_md_name("to_markdown(\"hello\") | to_md_name", true)]
#[case::to_md_table_cell("to_md_table_cell(\"hello\", 0, 0)", true)]
#[case::to_md_table_row("to_markdown(\"hello\") | to_md_table_row", true)]
#[case::to_md_text("to_markdown(\"hello\") | to_md_text", true)]
#[case::to_math("to_markdown(\"hello\") | to_math", true)]
#[case::to_math_inline("to_markdown(\"hello\") | to_math_inline", true)]
#[case::to_mdx("to_mdx(\"hello\")", true)]
#[case::to_strong("to_markdown(\"hello\") | to_strong", true)]
#[case::to_em("to_markdown(\"hello\") | to_em", true)]
fn test_markdown_conversion_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Markdown Manipulation Functions

#[rstest]
#[case::attr("to_markdown(\"[link](url)\") | attr(\"href\")", true)]
#[case::set_attr("to_markdown(\"[link](url)\") | set_attr(\"href\", \"new\")", true)]
#[case::get_title("to_markdown(\"[link](url)\") | get_title", true)]
#[case::get_url("to_markdown(\"[link](url)\") | get_url", true)]
#[case::set_check("to_markdown(\"- [ ] task\") | set_check(true)", true)]
#[case::set_list_ordered("to_markdown(\"- item\") | set_list_ordered(false)", true)]
#[case::set_code_block_lang("to_markdown(\"```\\ncode\\n```\") | set_code_block_lang(\"rust\")", true)]
#[case::set_ref("to_markdown(\"[link][ref]\") | set_ref(\"ref\")", true)]
fn test_markdown_manipulation_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}

// Dict/Array Constructor Functions

#[rstest]
#[case::dict_nullary("dict()", true)]
#[case::array_func("array(42)", true)]
fn test_constructor_functions(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "Code: {}\nResult: {:?}",
        code,
        result
    );
}
