//! Generators: functions that contain `yield` return a coroutine of the yielded values.

use mq_check::{TypeChecker, TypeError};
use mq_hir::{Hir, SymbolKind};
use rstest::rstest;

fn check(code: &str) -> (TypeChecker, Hir, Vec<TypeError>) {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    let mut checker = TypeChecker::new();
    let errors = checker.check(&hir);
    (checker, hir, errors)
}

/// The display type of the first symbol of `kind` named `name`.
fn type_of(code: &str, kind: SymbolKind, name: &str) -> String {
    let (checker, hir, errors) = check(code);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let (id, _) = hir
        .symbols()
        .filter(|(_, symbol)| {
            !hir.is_builtin_symbol(symbol) && symbol.kind == kind && symbol.value.as_deref() == Some(name)
        })
        .min_by_key(|(id, _)| hir.symbol_insertion_order(*id))
        .unwrap_or_else(|| panic!("no {kind:?} named {name}"));
    checker.type_of(id).unwrap().ty.display_renumbered()
}

#[rstest]
#[case::yields_numbers("def g(): yield: 1 | yield: 2;\n| g()", "generator<number>")]
#[case::final_value_is_ignored(r#"def g(): yield: 1 | "done";\n| g()"#, "generator<number>")]
#[case::lambda("let g = fn(): yield: 10 | yield: 20;\n| g()", "generator<number>")]
fn test_a_function_that_yields_returns_a_generator(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(type_of(code, SymbolKind::Call, "g"), expected);
}

#[test]
fn test_a_bare_yield_yields_none() {
    assert_eq!(
        type_of("def g(): yield;\n| g()", SymbolKind::Call, "g"),
        "generator<none>"
    );
}

#[test]
fn test_a_function_without_yield_is_unchanged() {
    assert_eq!(type_of("def f(): 1;\n| f()", SymbolKind::Call, "f"), "number");
}

#[test]
fn test_yield_in_a_nested_function_belongs_to_that_function() {
    let code = "def outer(): fn(): yield: 1; ;\n| outer()";
    assert_eq!(type_of(code, SymbolKind::Call, "outer"), "() -> generator<number>");
}

#[rstest]
#[case::first("first(g())", "(number | none)")]
#[case::map("map(g(), fn(x): to_string(x);)", "generator<string>")]
#[case::filter("filter(g(), fn(x): x > 1;)", "generator<number>")]
#[case::fold("fold(g(), 0, fn(acc, x): acc + x;)", "number")]
fn test_builtins_over_a_generator(#[case] call: &str, #[case] expected: &str) {
    let code = format!("def g(): yield: 1 | yield: 2;\nlet r = {call}\n| r");
    assert_eq!(type_of(&code, SymbolKind::Ref, "r"), expected);
}

#[test]
fn test_next_gives_the_value_and_done_record() {
    let code = "def g(): yield: 1;\nlet r = next(g())\n| r";
    assert_eq!(
        type_of(code, SymbolKind::Ref, "r"),
        "{done: bool, value: (number | none)}"
    );
}

#[rstest]
#[case::len("def g(): yield: 1;\n| len(g())")]
#[case::next_of_a_number("next(1)")]
fn test_a_generator_is_not_an_array_or_a_number(#[case] code: &str) {
    let (_, _, errors) = check(code);
    assert!(!errors.is_empty());
}

#[rstest]
#[case::control("def g(): yield: 1;\n| let s = g() | is_coroutine(s) | next(s) | send(s, 2) | status(s) | close(s)")]
#[case::narrowed("def f(v): if (is_coroutine(v)): next(v) else: v;")]
fn test_generator_operations_type_check(#[case] code: &str) {
    let (_, _, errors) = check(code);
    assert!(errors.is_empty(), "{errors:?}");
}
