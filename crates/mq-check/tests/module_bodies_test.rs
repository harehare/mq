//! Included and imported module definitions are type-checked when user code reaches them.
//! Their diagnostics are never reported.

use std::path::PathBuf;

use mq_check::{TypeChecker, TypeError};
use mq_hir::Hir;
use rstest::rstest;

const MODULE: &str = r#"
def one(): 1;
def outer(): one();
def broken_but_used(): upcase(1);
def broken_unused(): upcase(2);
"#;

fn module_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mq_check_module_bodies_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mymod.mq"), MODULE).unwrap();
    dir
}

fn check(code: &str) -> Vec<TypeError> {
    let mut loader = mq_lang::ModuleLoader::default();
    loader.set_search_paths(vec![module_dir()]);
    let mut hir = Hir::new(loader);
    hir.add_code(None, code);
    TypeChecker::new().check(&hir)
}

#[rstest]
#[case::direct(r#"include "mymod" | one() | upcase()"#)]
#[case::transitive(r#"include "mymod" | outer() | upcase()"#)]
fn reached_module_function_has_its_inferred_return_type(#[case] code: &str) {
    assert!(!check(code).is_empty(), "{code}");
}

#[rstest]
#[case::reached_function(r#"include "mymod" | broken_but_used()"#)]
#[case::unreached_function(r#"include "mymod" | one()"#)]
#[case::import_only(r#"import "mymod" | 1"#)]
fn module_diagnostics_are_not_reported(#[case] code: &str) {
    let errors = check(code);
    assert!(errors.is_empty(), "{code}: {errors:?}");
}

#[rstest]
#[case::root(r#""a" | upcase() == "A""#)]
#[case::root_unary(r#""a" | !is_empty()"#)]
#[case::root_nested(r#""a" | len() > 0"#)]
#[case::function_body(r#"def f(t): fn(s): s | to_text() == t; end"#)]
fn piped_input_reaches_operands_of_operators(#[case] code: &str) {
    let errors = check(code);
    assert!(errors.is_empty(), "{code}: {errors:?}");
}

#[rstest]
#[case::root(r#"1 | upcase() == "A""#)]
#[case::nested(r#"1 | upcase() + upcase()"#)]
fn piped_input_type_is_checked_in_operands(#[case] code: &str) {
    assert!(!check(code).is_empty(), "{code}");
}

#[rstest]
#[case::array_of_strings(r#"index(["a", "b"], "b")"#)]
#[case::array_of_numbers(r#"index([1, 2], 2)"#)]
#[case::piped(r#"["a"] | index("a")"#)]
#[case::rindex(r#"rindex(["a"], "a")"#)]
fn index_accepts_arrays(#[case] code: &str) {
    let errors = check(code);
    assert!(errors.is_empty(), "{code}: {errors:?}");
}

#[rstest]
#[case::array(r#"["a"] | len() | upcase()"#)]
#[case::dict(r#"{"a": 1} | len() | upcase()"#)]
#[case::through_operator(r#"["a"] | len() + 1 | upcase()"#)]
fn root_stage_after_an_unresolved_input_is_checked(#[case] code: &str) {
    assert_eq!(check(code).len(), 1, "{code}");
}

#[test]
fn root_stage_after_a_converting_call_is_valid() {
    let code = r#"["a"] | len() | to_string() | upcase()"#;
    assert!(check(code).is_empty(), "{code}");
}
