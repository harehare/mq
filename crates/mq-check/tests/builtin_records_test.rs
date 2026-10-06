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
