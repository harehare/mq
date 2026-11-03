//! Builtins that return records have precise field types, not dynamic ones.

use mq_check::TypeChecker;
use mq_hir::{Hir, SymbolKind};
use rstest::rstest;

fn variable_type(code: &str, name: &str) -> String {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    let mut checker = TypeChecker::new();
    let errors = checker.check(&hir);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let (id, _) = hir
        .symbols()
        .find(|(_, symbol)| {
            !hir.is_builtin_symbol(symbol)
                && symbol.kind == SymbolKind::Variable
                && symbol.value.as_deref() == Some(name)
        })
        .unwrap_or_else(|| panic!("no variable {name}"));
    checker.type_of(id).unwrap().ty.display_renumbered()
}

#[rstest]
#[case::split_records(
    r#"let r = split_records("a", "b") | r"#,
    "[{end_byte: number, index: number, start_byte: number, terminator: (string | none), text: string}]"
)]
#[case::extract_urls(
    r#"let r = extract_urls("a") | r"#,
    "[{end_byte: number, kind: string, start_byte: number, url: string}]"
)]
#[case::split_records_node(
    r#"let r = split_records(to_h("a", 1), "b") | r"#,
    "[{end_byte: number, index: number, start_byte: number, terminator: (string | none), text: string}]"
)]
#[case::extract_urls_node(
    r#"let r = extract_urls(to_h("a", 1)) | r"#,
    "[{end_byte: number, kind: string, start_byte: number, url: string}]"
)]
fn test_record_results(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(variable_type(code, "r"), expected);
}

#[test]
fn test_get_indexes_a_markdown_node() {
    assert_eq!(variable_type(r#"let r = get(to_h("a", 1), 0) | r"#, "r"), "markdown");
}

#[rstest]
#[case::nested(r#"def f(): {"a": {"b": 1}}; | let r = f()["a"]["b"] | r"#, "number")]
#[case::outer(r#"def f(): {"a": {"b": 1}}; | let r = f()["a"] | r"#, "{b: number | 'a}")]
#[case::three_levels(r#"def f(): {"a": {"b": {"c": "s"}}}; | let r = f()["a"]["b"]["c"] | r"#, "string")]
fn test_chained_bracket_keys_on_a_call_result(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(variable_type(code, "r"), expected);
}

#[rstest]
#[case::extract_urls("extract_urls(none)")]
#[case::split_records(r#"split_records(none, ",")"#)]
fn test_a_none_input_is_accepted(#[case] code: &str) {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    let errors = TypeChecker::new().check(&hir);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}

#[rstest]
#[case::file_info(
    r#"let r = file_info("a") | r"#,
    "{kind: string, modified: (number | none), path: string, size: number}"
)]
fn test_new_record_results(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(variable_type(code, "r"), expected);
}

#[rstest]
#[case::omitted(r#"http_all([{url: "http://a"}])"#)]
#[case::all_fields(r#"http_all([{url: "http://a", method: :post, body: "x", headers: {"a": "b"}}])"#)]
#[case::explicit_none(r#"http_all([{url: "http://a", method: None, body: None, headers: None}])"#)]
fn test_http_all_request_may_omit_optional_fields(#[case] code: &str) {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    assert!(TypeChecker::new().check(&hir).is_empty());
}

#[rstest]
#[case::wrong_url_type(r#"http_all([{url: 1}])"#)]
#[case::unknown_key(r#"http_all([{url: "a", bogus: 1}])"#)]
#[case::missing_required(r#"http_all([{method: "get"}])"#)]
#[case::non_string_header(r#"http_all([{url: "a", headers: {"X-Test": 1}}])"#)]
fn test_http_all_request_rejects_bad_fields(#[case] code: &str) {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    assert!(!TypeChecker::new().check(&hir).is_empty());
}
