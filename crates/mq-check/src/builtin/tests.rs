use mq_hir::Hir;
use rstest::rstest;

use crate::{TypeChecker, TypeError, types::Type};

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

#[rstest]
#[case::flat(Type::array(Type::Number), Type::Number)]
#[case::nested(Type::array(Type::array(Type::array(Type::String))), Type::String)]
#[case::mixed_tuple(
        Type::tuple(vec![Type::array(Type::String), Type::String]),
        Type::String
    )]
#[case::union_members(
        Type::array(Type::union(vec![Type::Number, Type::array(Type::String)])),
        Type::union(vec![Type::Number, Type::String])
    )]
fn test_flatten_result_type(#[case] arg: Type, #[case] leaf: Type) {
    let signature = super::refine_signature("flatten", std::slice::from_ref(&arg));
    assert_eq!(signature, Some(Type::function(vec![arg], Type::array(leaf))));
}

#[test]
fn test_refine_signature_ignores_unresolved_and_other_builtins() {
    assert_eq!(super::refine_signature("flatten", &[Type::Number]), None);
    assert_eq!(super::refine_signature("reverse", &[Type::array(Type::Number)]), None);
}

#[test]
fn test_registered_builtins_are_known_to_hir() {
    let mut ctx = crate::infer::InferenceContext::new();
    super::register_all(&mut ctx);
    let hir = create_hir("");
    let known: std::collections::HashSet<&str> = hir
        .symbols()
        .filter(|(_, symbol)| hir.is_builtin_symbol(symbol))
        .filter_map(|(_, symbol)| symbol.value.as_deref())
        .collect();
    let mut missing: Vec<&str> = ctx
        .builtin_names()
        .filter(|name| name.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .filter(|name| !known.contains(name))
        .collect();
    missing.sort_unstable();
    assert!(
        missing.is_empty(),
        "type signatures without a builtin definition: {missing:?}"
    );
}

#[test]
fn test_public_native_builtins_have_type_signatures() {
    let mut ctx = crate::infer::InferenceContext::new();
    super::register_all(&mut ctx);
    let registered: std::collections::HashSet<&str> = ctx.builtin_names().collect();
    let mut missing: Vec<&str> = mq_lang::BUILTIN_FUNCTION_NAMES
        .iter()
        .copied()
        .filter(|name| !registered.contains(name))
        .collect();
    missing.sort_unstable();
    assert!(
        missing.is_empty(),
        "public native builtins without a type signature: {missing:?}"
    );
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
#[case::add_numbers("1 + 2")]
#[case::add_number_string("1 + \"string\"")]
#[case::add_strings("\"hello\" + \"world\"")]
#[case::add_string_number("\"value: \" + 42")] // String + number coercion
#[case::add_arrays("[1, 2] + [3, 4]")] // Array concatenation
#[case::add_mixed("1 + \"world\"")] // Number + string coercion
#[case::sub("10 - 5")]
#[case::mul("3 * 4")]
#[case::mul_array_repeat("[1, 2] * 3")] // Array repetition
#[case::div("10 / 2")]
#[case::mod_op("10 % 3")]
#[case::pow("2 ^ 8")]
fn test_arithmetic_operators(#[case] code: &str) {
    let result = check_types(code);
    assert!(result.is_empty(), "Code: {}\nResult: {:?}", code, result);
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
#[case::rand("rand()", true)]
#[case::rand_seeded("rand(42)", true)]
#[case::rand_int("rand_int(1, 10)", true)]
#[case::rand_int_seeded("rand_int(1, 10, 42)", true)]
#[case::ln("ln(2.0)", true)]
#[case::log10("log10(100)", true)]
#[case::log("log(2.0)", true)]
#[case::sqrt("sqrt(4)", true)]
#[case::exp("exp(1)", true)]
#[case::sin("sin(0)", true)]
#[case::cos("cos(0)", true)]
#[case::tan("tan(0)", true)]
#[case::ln_string("ln(\"x\")", false)] // Should fail: wrong type
#[case::sqrt_string("sqrt(\"x\")", false)] // Should fail: wrong type
#[case::sin_string("sin(\"x\")", false)] // Should fail: wrong type
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
#[case::ascii_downcase("ascii_downcase(\"HELLO\")", true)]
#[case::ascii_upcase("ascii_upcase(\"hello\")", true)]
#[case::trim("trim(\"  hello  \")", true)]
#[case::downcase_number("downcase(42)", false)] // Should fail: wrong type
#[case::ascii_downcase_number("ascii_downcase(42)", false)] // Should fail: wrong type
#[case::rtrim("rtrim(\"  hello  \")", true)]
#[case::rindex("rindex(\"hello world hello\", \"hello\")", true)]
#[case::token_count("token_count(\"hello world\", \"gpt-4\")", true)]
#[case::token_count_no_model("token_count(\"hello world\")", true)]
#[case::token_count_number("token_count(42, \"gpt-4\")", false)] // Should fail: wrong type
#[case::token_compress("to_markdown(\"# Title\\n\\nBody\") | token_compress(100)", true)]
#[case::token_compress_with_model("to_markdown(\"# Title\\n\\nBody\") | token_compress(100, \"gpt-4\")", true)]
#[case::token_compress_string("token_compress(\"not an array\", 100)", false)] // Should fail: wrong type
#[case::capture("capture(\"hello 42\", \"(?P<word>\\\\w+)\")", true)]
#[case::is_regex_match("is_regex_match(\"hello123\", \"[0-9]+\")", true)]
#[case::is_not_regex_match("is_not_regex_match(\"hello123\", \"[0-9]+\")", true)]
#[case::is_not_regex_match_number("is_not_regex_match(42, \"[0-9]+\")", false)] // Should fail: wrong type
#[case::scan("scan(\"2024-06\", \"(\\\\d{4})-(\\\\d{2})\")", true)]
#[case::scan_number("scan(42, \"[0-9]+\")", false)] // Should fail: wrong type
#[case::base64url("base64url(\"hello\")", true)]
#[case::base64urld("base64urld(\"aGVsbG8=\")", true)]
#[case::ltrim_number("ltrim(42)", false)]
#[case::rtrim_number("rtrim(42)", false)]
#[case::html_escape("html_escape(\"<b>hi</b>\")", true)]
#[case::html_escape_number("html_escape(42)", false)] // Should fail: wrong type
#[case::html_unescape("html_unescape(\"&lt;b&gt;hi&lt;/b&gt;\")", true)]
#[case::html_unescape_number("html_unescape(42)", false)] // Should fail: wrong type
#[case::strip_tags("strip_tags(\"<b>hi</b>\")", true)]
#[case::strip_tags_number("strip_tags(42)", false)] // Should fail: wrong type
#[case::sanitize_html("sanitize_html(\"<script>alert(1)</script>\")", true)]
#[case::sanitize_html_number("sanitize_html(42)", false)] // Should fail: wrong type
#[case::markdown_escape("markdown_escape(\"**bold**\", \"text\")", true)]
#[case::markdown_escape_number("markdown_escape(42, \"text\")", false)] // Should fail: wrong type
#[case::markdown_escape_context_number("markdown_escape(\"a\", 42)", false)] // Should fail: wrong type
#[case::markdown_escape_markdown_input("to_code(\"value\", \"text\") | markdown_escape(\"text\")", true)]
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
#[case::indices("indices(\"hello world hello\", \"hello\")", true)]
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
#[case::lines("lines(\"a\\nb\\nc\")", true)]
#[case::lines_wrong_type("lines(42)", false)]
#[case::unlines("unlines([\"a\", \"b\", \"c\"])", true)]
fn test_lines_unlines_functions(#[case] code: &str, #[case] should_succeed: bool) {
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
#[case::regex_escape("regex_escape(\"a.b*c?\")", true)]
#[case::regex_escape_number("regex_escape(42)", false)] // Should fail: wrong type
#[case::base64("base64(\"hello\")", true)]
#[case::base64d("base64d(\"aGVsbG8=\")", true)]
#[case::base64d_bytes("base64d_bytes(\"aGVsbG8=\")", true)]
#[case::base64urld_bytes("base64urld_bytes(\"aGVsbG8\")", true)]
#[case::url_encode("url_encode(\"hello world\")", true)]
#[case::url_decode("url_decode(\"hello%20world\")", true)]
#[case::uuid("uuid()", true)]
#[case::uuid_v4("uuid_v4()", true)]
#[case::uuid_v7("uuid_v7()", true)]
#[case::random_string("random_string(10, \"abc\")", true)]
#[case::random_string_seeded("random_string(10, \"abc\", 42)", true)]
// Should fail: wrong types. Uses non-numeric/non-string args so it can't accidentally
// satisfy the seeded ternary overload via the implicit-piped-input fallback (a bare
// `(String, Number)` pair would coincidentally match that overload's charset/seed slots).
#[case::random_string_wrong_types("random_string(true, false)", false)]
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
#[case::shuffle("shuffle([1, 2, 3])", true)]
#[case::shuffle_seeded("shuffle([1, 2, 3], 42)", true)]
#[case::sample("sample([1, 2, 3], 2)", true)]
#[case::sample_seeded("sample([1, 2, 3], 2, 42)", true)]
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
#[case::has_dict("has({\"a\": 1}, \"a\")", true)]
#[case::has_array("has([1, 2, 3], 1)", true)]
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
#[case::pick("pick({\"a\": 1, \"b\": 2, \"c\": 3}, [\"a\", \"b\"])", true)]
#[case::omit("omit({\"a\": 1, \"b\": 2, \"c\": 3}, [\"a\"])", true)]
fn test_dict_pick_omit_functions(#[case] code: &str, #[case] should_succeed: bool) {
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
#[case::from_entries("from_entries([[\"a\", 1], [\"b\", 2]])", true)]
#[case::with_entries("with_entries({\"a\": 1}, fn(pair): [pair[0], pair[1] + 1];)", true)]
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
#[case::to_boolean_string("to_boolean(\"true\")", true)]
#[case::to_boolean_bool("to_boolean(true)", true)]
#[case::to_boolean_number("to_boolean(42)", false)] // Should fail: wrong type
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
#[case::gmtime("gmtime(0)", true)]
#[case::localtime("localtime(0)", true)]
#[case::mktime("mktime([2024, 0, 1, 0, 0, 0, 1, 0])", true)]
#[case::strftime("strftime(0, \"%Y-%m-%d\")", true)]
#[case::strptime("strptime(\"2024-01-01\", \"%Y-%m-%d\")", true)]
#[case::date_add("date_add([2024, 0, 1, 0, 0, 0, 1, 0], 1, \"days\")", true)]
#[case::date_diff(
    "date_diff([2024, 0, 1, 0, 0, 0, 1, 0], [2024, 0, 2, 0, 0, 0, 2, 1], \"days\")",
    true
)]
#[case::gmtime_string("gmtime(\"x\")", false)] // Should fail: wrong type
#[case::mktime_string("mktime(\"x\")", false)] // Should fail: wrong type
#[case::strftime_swapped("strftime(\"x\", 1)", false)] // Should fail: wrong type
#[case::strptime_swapped("strptime(1, \"%Y-%m-%d\")", false)] // Should fail: wrong type
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

// File I/O And Path Functions

#[rstest]
#[case::read_file("read_file(\"a.md\")", true)]
#[case::read_file_bytes("read_file_bytes(\"a.md\")", true)]
#[case::open_file("open_file(\"a.md\")", true)]
#[case::open_file_number("open_file(42)", false)]
#[case::file_exists("file_exists(\"a.md\")", true)]
#[case::file_size("file_size(\"a.md\")", true)]
#[case::file_info("file_info(\"a.md\")", true)]
#[case::collection("collection(\"docs\")", true)]
#[case::collection_respect_gitignore("collection(\"docs\", true)", true)]
#[case::collection_len("len(collection(\"docs\"))", true)]
#[case::basename("basename(\"a/b.md\")", true)]
#[case::dirname("dirname(\"a/b.md\")", true)]
#[case::extname("extname(\"a/b.md\")", true)]
#[case::stem("stem(\"a/b.md\")", true)]
#[case::path_join("path_join(\"a\", \"b.md\")", true)]
#[case::glob_match("glob_match(\"*.md\", \"a.md\")", true)]
#[case::write_file_string("write_file(\"a.md\", \"content\")", true)]
#[case::write_file_bytes("write_file(\"a.md\", to_bytes(\"content\"))", true)]
#[case::http_get("http(\"get\", \"https://example.invalid\")", true)]
#[case::http_get_symbol("http(:get, \"https://example.invalid\")", true)]
#[case::http_post("http(\"post\", \"https://example.invalid\", \"{}\")", true)]
#[case::http_post_symbol("http(:post, \"https://example.invalid\", \"{}\")", true)]
#[case::http_headers(
    "http(\"get\", \"https://example.invalid\", {\"Accept\": \"application/json\"})",
    true
)]
#[case::http_headers_symbol("http(:get, \"https://example.invalid\", {\"Accept\": \"application/json\"})", true)]
#[case::http_body_and_headers(
    "http(\"post\", \"https://example.invalid\", \"{}\", {\"Content-Type\": \"application/json\"})",
    true
)]
#[case::http_body_and_headers_symbol(
    "http(:post, \"https://example.invalid\", \"{}\", {\"Content-Type\": \"application/json\"})",
    true
)]
#[case::read_file_number("read_file(42)", false)] // Should fail: wrong type
#[case::file_exists_number("file_exists(42)", false)] // Should fail: wrong type
#[case::file_size_number("file_size(42)", false)] // Should fail: wrong type
#[case::file_info_number("file_info(42)", false)] // Should fail: wrong type
#[case::collection_number("collection(42)", false)] // Should fail: wrong type
#[case::collection_respect_gitignore_number("collection(\"docs\", 42)", false)] // Should fail: wrong type
#[case::basename_number("basename(42)", false)] // Should fail: wrong type
#[case::path_join_number("path_join(42, \"b\")", false)] // Should fail: wrong type
#[case::glob_match_number("glob_match(42, \"a.md\")", false)] // Should fail: wrong type
#[case::write_file_number("write_file(42, \"content\")", false)] // Should fail: wrong type
#[case::http_get_number("http(\"get\", 42)", false)] // Should fail: wrong type
#[case::http_headers_number("http(\"get\", \"https://example.invalid\", 42)", false)] // Should fail: wrong type
fn test_file_io_functions(#[case] code: &str, #[case] should_succeed: bool) {
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
#[case::string_to_ascii_upcase("\"hello\" | ascii_upcase", true)]
#[case::string_to_trim("\"  hello  \" | trim", true)]
#[case::number_to_abs("-42 | abs", true)]
#[case::string_to_len("\"hello\" | len", true)]
#[case::chained_pipes("\"  hello  \" | trim | upcase", true)]
#[case::chained_string_to_len("\"hello\" | upcase | len", true)]
#[case::chained_split_to_len("\"hello\" | split(\",\") | len", true)]
#[case::chained_array_reverse_first("[1,2,3] | reverse | first", true)]
#[case::number_to_upcase("42 | upcase", false)] // Number piped to string function
#[case::number_to_ascii_upcase("42 | ascii_upcase", false)] // Number piped to string function
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
#[case::add_bool_number("true + 1", false)] // bool + number is invalid
#[case::add_bool_string("true + \"s\"", false)] // bool + string is invalid
#[case::sub_strings("\"a\" - \"b\"", false)] // strings cannot be subtracted
#[case::sub_string_number("\"a\" - 1", false)] // string - number is invalid
#[case::mul_string_number("\"a\" * 3", true)] // string * number is invalid
#[case::div_strings("\"a\" / \"b\"", false)] // strings cannot be divided
#[case::mod_strings("\"a\" % \"b\"", false)] // strings cannot use modulo
#[case::sub_bool_bool("true - false", false)] // booleans cannot be subtracted
#[case::mul_bool_number("true * 2", false)] // bool * number is invalid
#[case::div_bool_number("true / 2", false)] // bool / number is invalid
#[case::mod_bool_number("true % 2", false)] // bool % number is invalid
#[case::mod_number_string("10 % \"2\"", false)] // number % string is invalid
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
#[case::lt_number_bool("1 < true", false)] // mixed types
#[case::gt_bool_string("true > \"x\"", false)] // mixed types
#[case::lte_string_number("\"a\" <= 1", false)] // mixed types
#[case::gte_number_bool("1 >= false", false)] // mixed types
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
#[case::not_number("!42", true)] // number is truthy/falsy → bool
#[case::not_string("!\"hello\"", true)] // string is truthy/falsy → bool
#[case::not_array("![1, 2, 3]", true)] // array is truthy/falsy → bool
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
#[case::ascii_downcase_number("ascii_downcase(42)", false)] // wrong argument type
#[case::ceil_string("ceil(\"hello\")", false)] // wrong argument type
#[case::floor_bool("floor(true)", false)] // wrong argument type
#[case::len_no_args("len()", true)] // valid: root-level dynamic piped input satisfies the arg
#[case::split_numbers("split(1, 2)", false)] // wrong argument types
#[case::join_numbers("join(1, 2)", false)] // wrong argument types
#[case::starts_with_numbers("starts_with(1, 2)", false)] // wrong argument types
#[case::round_string("round(\"hello\")", false)] // wrong argument type
#[case::trunc_string("trunc(\"hello\")", false)] // wrong argument type
#[case::abs_bool("abs(true)", false)] // wrong argument type
#[case::floor_string("floor(\"hello\")", false)] // wrong argument type
#[case::ceil_bool("ceil(false)", false)] // wrong argument type
#[case::upcase_number("upcase(42)", false)] // wrong argument type
#[case::ascii_upcase_number("ascii_upcase(42)", false)] // wrong argument type
#[case::trim_number("trim(42)", false)] // wrong argument type
#[case::ltrim_bool("ltrim(true)", false)] // wrong argument type
#[case::rtrim_bool("rtrim(true)", false)] // wrong argument type
#[case::explode_number("explode(42)", false)] // expects string
#[case::implode_string("implode(\"hello\")", false)] // expects array of numbers
#[case::split_wrong_sep("split(\"hello\", 42)", false)] // separator must be string
#[case::join_wrong_sep("join([\"a\", \"b\"], 42)", false)] // separator must be string
#[case::replace_wrong_sep("replace(\"hello\", 42, \"r\")", false)] // wrong separator type
#[case::replace_too_few_args("replace(\"l\")", false)] // 1 explicit + 1 piped = 2 args, needs 3
#[case::replace_too_many_args("replace(\"hello\", \"l\", \"r\", \"extra\")", false)] // too many args
#[case::gsub_wrong_first("gsub(42, \"l\", \"r\")", false)] // expects string first arg
#[case::starts_with_num_sep("starts_with(\"hello\", 42)", false)] // expects string prefix
#[case::ends_with_num_sep("ends_with(\"hello\", 42)", false)] // expects string suffix
#[case::flatten_string("flatten(\"hello\")", false)] // expects array
#[case::sort_string("sort(\"hello\")", false)] // expects array
#[case::uniq_string("uniq(\"hello\")", false)] // expects array
#[case::compact_string("compact(\"hello\")", false)] // expects array
#[case::reverse_number("reverse(42)", false)] // expects array or string
#[case::values_number("values(42)", false)] // expects dict
#[case::values_string("values(\"hello\")", false)] // expects dict
#[case::entries_string("entries(\"hello\")", false)] // expects dict
#[case::range_string("range(\"a\", true)", false)] // expects number or string
#[case::repeat_bool("repeat(\"x\", true)", false)] // second arg must be number
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
#[case::number_to_downcase("42 | downcase", false)] // number piped to string function
#[case::number_to_ascii_downcase("42 | ascii_downcase", false)] // number piped to string function
#[case::bool_to_upcase("true | upcase", false)] // bool piped to string function
#[case::bool_to_ascii_upcase("true | ascii_upcase", false)] // bool piped to string function
#[case::bool_to_trim("true | trim", false)] // bool piped to string function
#[case::bool_to_abs("true | abs", false)] // bool piped to number function
#[case::string_to_floor("\"hello\" | floor", false)] // string piped to number function
#[case::string_to_round("\"hello\" | round", false)] // string piped to number function
#[case::string_to_trunc("\"hello\" | trunc", false)] // string piped to number function
#[case::number_to_len("42 | len", false)] // number piped to len (expects array or string)
#[case::bool_to_len("true | len", false)] // bool piped to len
#[case::number_to_sort("42 | sort", false)] // number piped to sort (expects array)
#[case::number_to_reverse("42 | reverse", false)] // number piped to reverse
#[case::number_to_flatten("42 | flatten", false)] // number piped to flatten
#[case::number_to_uniq("42 | uniq", false)] // number piped to uniq
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
#[case::piped_gsub_variable_arg("def slugify(s, separator = \"-\"): s | gsub(\"[^a-z0-9]+\", separator) end", true)] // regression: default param used in gsub should not produce false error
#[case::replace_sstring_args("def f(x, v): x | replace(s\"old${v}\", s\"new${v}\") end", true)] // s-string args should not expand segments as separate arguments
#[case::replace_chained_sstring(
    "def f(line, prev, ver): line | replace(s\"version = \\\"${prev}\\\"\", s\"version = \\\"${ver}\\\"\") | replace(s\"old${prev}\", s\"new${ver}\") end",
    true
)] // chained replace with s-string args
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
#[case::in_return_plus_number("in([1, 2, 3], 1) + 1", false)] // in returns bool; bool + number is invalid
#[case::contains_return_plus_number("contains(\"hello\", \"he\") + 1", false)] // contains returns bool; bool + number is invalid
#[case::bsearch("bsearch([1, 2, 3, 4], 2)", true)]
#[case::bsearch_other_type("bsearch([1, 2, 3, 4], \"2\")", true)] // the target is never found
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

// Type filter functions (`arrays`, `booleans`, etc.) are `def`-based builtins from
// builtin.mq, not registered in this file: hir.add_builtin() loads builtin.mq's
// source into the HIR, so their types are inferred from the function bodies just
// like any other user-defined function.
#[rstest]
#[case::arrays("arrays([1, 2])", true)]
#[case::markdowns("markdowns(to_hr())", true)]
#[case::booleans("booleans(true)", true)]
#[case::numbers("numbers(42)", true)]
#[case::strings("strings(\"a\")", true)]
#[case::dicts("dicts({\"a\": 1})", true)]
#[case::nones("nones(None)", true)]
#[case::bytes_filter("bytes(to_bytes(\"a\"))", true)]
#[case::iterables_array("iterables([1, 2])", true)]
#[case::iterables_dict("iterables({\"a\": 1})", true)]
#[case::scalars("scalars(1)", true)]
fn test_type_filter_functions(#[case] code: &str, #[case] should_succeed: bool) {
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
#[case::is_h("to_markdown(\"# hello\") | first() | is_h()", true)]
#[case::is_p("to_markdown(\"hello\") | first() | is_p()", true)]
#[case::is_code("to_markdown(\"hello\") | first() | is_code()", true)]
#[case::is_code_block("to_markdown(\"hello\") | first() | is_code_block()", true)]
#[case::is_code_inline("to_markdown(\"hello\") | first() | is_code_inline()", true)]
#[case::is_em("to_markdown(\"hello\") | first() | is_em()", true)]
#[case::is_strong("to_markdown(\"hello\") | first() | is_strong()", true)]
#[case::is_link("to_markdown(\"hello\") | first() | is_link()", true)]
#[case::is_image("to_markdown(\"hello\") | first() | is_image()", true)]
#[case::is_list("to_markdown(\"hello\") | first() | is_list()", true)]
#[case::is_list_item("to_markdown(\"hello\") | first() | is_list_item()", true)]
#[case::is_table("to_markdown(\"hello\") | first() | is_table()", true)]
#[case::is_table_row("to_markdown(\"hello\") | first() | is_table_row()", true)]
#[case::is_table_cell("to_markdown(\"hello\") | first() | is_table_cell()", true)]
#[case::is_blockquote("to_markdown(\"hello\") | first() | is_blockquote()", true)]
#[case::is_hr("to_markdown(\"hello\") | first() | is_hr()", true)]
#[case::is_html("to_markdown(\"hello\") | first() | is_html()", true)]
#[case::is_text("to_markdown(\"hello\") | first() | is_text()", true)]
#[case::is_softbreak("to_markdown(\"hello\") | first() | is_softbreak()", true)]
#[case::is_hardbreak("to_markdown(\"hello\") | first() | is_hardbreak()", true)]
#[case::is_task_list_item("to_markdown(\"hello\") | first() | is_task_list_item()", true)]
#[case::is_footnote("to_markdown(\"hello\") | first() | is_footnote()", true)]
#[case::is_footnote_ref("to_markdown(\"hello\") | first() | is_footnote_ref()", true)]
#[case::is_strikethrough("to_markdown(\"hello\") | first() | is_strikethrough()", true)]
#[case::is_math("to_markdown(\"hello\") | first() | is_math()", true)]
#[case::is_math_inline("to_markdown(\"hello\") | first() | is_math_inline()", true)]
#[case::is_toml("to_markdown(\"hello\") | first() | is_toml()", true)]
#[case::is_yaml("to_markdown(\"hello\") | first() | is_yaml()", true)]
#[case::is_h_level("to_markdown(\"# hello\") | first() | is_h_level(1)", true)]
#[case::is_h_level_wrong_type("is_h_level(42, \"str\")", false)]
#[case::is_callout("to_markdown(\"hello\") | first() | is_callout()", true)]
#[case::is_table_align("to_markdown(\"hello\") | first() | is_table_align()", true)]
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
#[case::from_html("\"<h1>Hello</h1>\" | from_html()", true)]
#[case::to_markdown("to_markdown(\"hello\")", true)]
#[case::to_text("to_markdown(\"hello\") | first() | to_text()", true)]
#[case::to_html("to_markdown(\"hello\") | first() | to_html()", true)]
#[case::to_markdown_string("to_markdown(\"hello\") | first()  | to_markdown_string()", true)]
#[case::to_code("to_markdown(\"hello\") | first() | to_code(\"rust\")", true)]
#[case::to_code_inline("to_markdown(\"hello\") | first() | to_code_inline()", true)]
#[case::to_h("to_markdown(\"hello\") | first() | to_h(1)", true)]
#[case::to_hr("to_hr()", true)]
#[case::to_image("to_image(\"alt\", \"url\", \"title\")", true)]
#[case::to_link("to_link(\"text\", \"url\", \"title\")", true)]
#[case::to_md_list("to_markdown(\"hello\") | first() | to_md_list(1)", true)]
#[case::to_md_name("to_markdown(\"hello\") | first() | to_md_name()", true)]
#[case::to_md_table_cell("to_md_table_cell(\"hello\", 0, 0)", true)]
#[case::to_md_table_row("to_markdown(\"hello\") | first() | to_md_table_row()", true)]
#[case::to_md_text("to_markdown(\"hello\") | first() | to_md_text()", true)]
#[case::to_math("to_markdown(\"hello\") | first() | to_math()", true)]
#[case::to_math_inline("to_markdown(\"hello\") | first() | to_math_inline()", true)]
#[case::to_mdx("to_mdx(\"hello\")", true)]
#[case::to_strong("to_markdown(\"hello\") | first() | to_strong()", true)]
#[case::to_em("to_markdown(\"hello\") | first() | to_em()", true)]
#[case::to_blockquote("to_markdown(\"hello\") | first() | to_blockquote()", true)]
#[case::to_delete("to_markdown(\"hello\") | first() | to_delete()", true)]
#[case::to_callout("to_markdown(\"hello\") | first() | to_callout(\"note\", \"Note\")", true)]
#[case::to_md_fragment_markdown("to_markdown(\"hello\") | first() | to_md_fragment()", true)]
#[case::to_md_fragment_array("to_md_fragment([\"a\", \"b\"])", true)]
#[case::to_md_table_align("to_md_table_align([\"left\", \"right\"])", true)]
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
#[case::attr("to_markdown(\"[link](url)\") | first() | attr(\"url\")", true)]
#[case::set_attr("to_markdown(\"[link](url)\") | first() | set_attr(\"href\", \"new\")", true)]
#[case::set_children("to_markdown(\"# heading\") | first() | set_children([\"new\"])", true)]
#[case::get_title("to_markdown(\"[link](url)\") | first() | get_title", true)]
#[case::get_url("to_markdown(\"[link](url)\") | first() | get_url", true)]
#[case::get_location("to_markdown(\"# heading\") | first() | get_location", true)]
#[case::set_check("to_markdown(\"- [ ] task\") | first() | set_check(true)", true)]
#[case::set_list_ordered("to_markdown(\"- item\") | first() | set_list_ordered(false)", true)]
#[case::set_code_block_lang("to_markdown(\"```\\ncode\\n```\") | first() | set_code_block_lang(\"rust\")", true)]
#[case::set_ref("to_markdown(\"[link][ref]\") | first() | set_ref(\"ref\")", true)]
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

// Higher-Order Function Lambda Type Errors

#[rstest]
#[case::map_lambda_sub_error("map([1, 2, 3], fn(x): x - \"str\";)", false, "map lambda body type error")]
#[case::map_lambda_bool_op("map([1, 2, 3], fn(x): x - true;)", false, "map lambda bool operand")]
#[case::filter_lambda_sub_error("filter([1, 2, 3], fn(x): x - \"str\";)", false, "filter lambda body type error")]
#[case::fold_lambda_type_error(
    "fold([1, 2, 3], 0, fn(acc, x): acc - \"str\";)",
    false,
    "fold lambda body type error"
)]
#[case::map_lambda_valid("map([1, 2, 3], fn(x): x + 1;)", true, "map lambda valid")]
#[case::filter_lambda_valid("filter([1, 2, 3], fn(x): x > 1;)", true, "filter lambda valid")]
#[case::fold_lambda_valid("fold([1, 2, 3], 0, fn(acc, x): acc + x;)", true, "fold lambda valid")]
fn test_higher_order_lambda_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Result={:?}",
        description,
        code,
        result
    );
}

// Bit-Shift Operator Type Errors

#[rstest]
#[case::shift_left_bool_rhs("3 << true", false, "shift left with bool right operand")]
#[case::shift_right_bool_rhs("8 >> true", false, "shift right with bool right operand")]
#[case::shift_left_string_rhs("3 << \"1\"", false, "shift left with string right operand")]
#[case::shift_right_string_rhs("8 >> \"2\"", false, "shift right with string right operand")]
#[case::shift_left_valid("3 << 1", true, "shift left with numbers is valid")]
#[case::shift_right_valid("8 >> 2", true, "shift right with numbers is valid")]
fn test_bitshift_op_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Result={:?}",
        description,
        code,
        result
    );
}

// Markdown Function Type Errors

#[rstest]
#[case::to_markdown_number("to_markdown(42)", true, "to_markdown accepts any type")]
#[case::to_markdown_bool("to_markdown(true)", true, "to_markdown accepts any type")]
#[case::to_h_wrong_level("to_markdown(\"hello\") | to_h(\"one\")", false, "to_h expects number level")]
#[case::to_md_list_wrong_type("to_markdown(\"hello\") | to_md_list(\"x\")", false, "to_md_list expects number")]
#[case::to_md_table_cell_wrong_col(
    "to_md_table_cell(\"hello\", \"col\", 0)",
    false,
    "to_md_table_cell expects number col"
)]
#[case::to_md_table_cell_wrong_row(
    "to_md_table_cell(\"hello\", 0, \"row\")",
    false,
    "to_md_table_cell expects number row"
)]
#[case::set_check_wrong_type("to_markdown(\"- [ ] task\") | set_check(42)", false, "set_check expects bool")]
#[case::set_children_wrong_type(
    "to_markdown(\"# heading\") | set_children(\"not_an_array\")",
    false,
    "set_children expects array"
)]
#[case::set_list_ordered_wrong_type(
    "to_markdown(\"- item\") | set_list_ordered(42)",
    false,
    "set_list_ordered expects bool"
)]
#[case::set_code_block_lang_wrong_type(
    "to_markdown(\"hello\") | set_code_block_lang(42)",
    false,
    "set_code_block_lang expects string"
)]
#[case::to_markdown_valid("to_markdown(\"# hello\")", true, "to_markdown with string is valid")]
#[case::to_h_valid("to_markdown(\"hello\") | to_h(1)", true, "to_h with number is valid")]
#[case::to_callout_wrong_kind_type(
    "to_markdown(\"hello\") | first() | to_callout(42, \"Note\")",
    false,
    "to_callout expects string kind"
)]
#[case::to_callout_wrong_title_type(
    "to_markdown(\"hello\") | first() | to_callout(\"note\", 42)",
    false,
    "to_callout expects string title"
)]
#[case::to_md_table_align_wrong_type("to_md_table_align(\"left\")", false, "to_md_table_align expects an array")]
fn test_markdown_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Result={:?}",
        description,
        code,
        result
    );
}

// Piped Wrong-Type Comprehensive Cases

#[rstest]
#[case::number_to_explode("42 | explode", false, "number piped to explode (expects string)")]
#[case::number_to_downcase("42 | downcase", false, "number piped to downcase")]
#[case::number_to_ascii_downcase("42 | ascii_downcase", false, "number piped to ascii_downcase")]
#[case::number_to_ltrim("42 | ltrim", false, "number piped to ltrim")]
#[case::number_to_rtrim("42 | rtrim", false, "number piped to rtrim")]
#[case::bool_to_floor("true | floor", false, "bool piped to floor")]
#[case::bool_to_round("true | round", false, "bool piped to round")]
#[case::bool_to_trunc("true | trunc", false, "bool piped to trunc")]
#[case::bool_to_sort("true | sort", false, "bool piped to sort")]
#[case::string_to_flatten("\"hello\" | flatten", false, "string piped to flatten")]
#[case::string_to_sort("\"hello\" | sort", false, "string piped to sort")]
#[case::string_to_uniq("\"hello\" | uniq", false, "string piped to uniq")]
#[case::string_to_compact("\"hello\" | compact", false, "string piped to compact")]
fn test_pipe_wrong_type_comprehensive(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Result={:?}",
        description,
        code,
        result
    );
}

// Bytes builtin type signatures

#[rstest]
// to_bytes: string -> bytes
#[case::to_bytes_string(r#"to_bytes("hello")"#, true)]
// to_bytes: [number] -> bytes
#[case::to_bytes_array("to_bytes([104, 101, 108, 108, 111])", true)]
// to_bytes: bytes -> bytes (identity)
#[case::to_bytes_bytes(r#"to_bytes("hi") | to_bytes"#, true)]
// _cbor_stringify produces bytes
#[case::cbor_stringify_produces_bytes(r#"{"k": 1} | _cbor_stringify | to_hex"#, true)]
// bytes add/concatenation
#[case::bytes_add(r#"to_bytes("ab") | add(to_bytes("cd"))"#, true)]
#[case::bytes_add_op(r#"to_bytes("ab") + to_bytes("cd")"#, true)]
// slice on bytes
#[case::bytes_slice(r#"to_bytes("hello") | slice(1, 3)"#, true)]
// reverse on bytes
#[case::bytes_reverse(r#"to_bytes("hello") | reverse"#, true)]
// len on bytes
#[case::bytes_len(r#"to_bytes("hello") | len"#, true)]
// base64 on bytes produces string
#[case::bytes_base64(r#"to_bytes("hello") | base64"#, true)]
// to_hex: bytes -> string
#[case::to_hex(r#"to_bytes("hello") | to_hex"#, true)]
// hexdump: bytes -> string
#[case::hexdump(r#"to_bytes("hello") | hexdump"#, true)]
// from_hex: string -> bytes
#[case::from_hex(r#"from_hex("deadbeef")"#, true)]
// utf8: bytes -> string
#[case::utf8(r#"to_bytes("hello") | utf8"#, true)]
// xor: (bytes, bytes) -> bytes
#[case::xor(r#"to_bytes("ab") | xor(to_bytes("cd"))"#, true)]
// sha512: string -> string
#[case::sha512_string(r#"sha512("hello")"#, true)]
// sha512: bytes -> string
#[case::sha512_bytes(r#"to_bytes("hello") | sha512"#, true)]
// bytes comparisons
#[case::bytes_gt(r#"gt(to_bytes("b"), to_bytes("a"))"#, true)]
#[case::bytes_gte(r#"gte(to_bytes("a"), to_bytes("a"))"#, true)]
#[case::bytes_lt(r#"lt(to_bytes("a"), to_bytes("b"))"#, true)]
#[case::bytes_lte(r#"lte(to_bytes("a"), to_bytes("a"))"#, true)]
// is_bytes: (a) -> bool
#[case::is_bytes(r#"to_bytes("hello") | is_bytes"#, true)]
// base64d_bytes: string -> bytes, usable where bytes are expected
#[case::base64d_bytes_xor(r#"xor(base64d_bytes("aGVsbG8="), to_bytes("world"))"#, true)]
#[case::base64urld_bytes_is_bytes(r#"base64urld_bytes("aGVsbG8") | is_bytes"#, true)]
fn test_bytes_builtin_signatures(#[case] code: &str, #[case] should_succeed: bool) {
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
// to_hex requires bytes, not string
#[case::to_hex_string(r#"to_hex("hello")"#, false, "to_hex expects bytes not string")]
// utf8 requires bytes, not string
#[case::utf8_string(r#"utf8("hello")"#, false, "utf8 expects bytes not string")]
// hexdump requires bytes, not string
#[case::hexdump_string(r#"hexdump("hello")"#, false, "hexdump expects bytes not string")]
// from_hex requires string, not bytes
#[case::from_hex_bytes(r#"to_bytes("hello") | from_hex"#, false, "from_hex expects string not bytes")]
fn test_bytes_type_errors(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(
        result.is_empty(),
        should_succeed,
        "{}: Code='{}' Result={:?}",
        description,
        code,
        result
    );
}
