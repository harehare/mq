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
fn test_record_results(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(variable_type(code, "r"), expected);
}
