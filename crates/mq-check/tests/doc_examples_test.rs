//! The examples in the documentation of the builtins are valid programs, so none of them may
//! have a type error. A failure means a builtin signature is narrower than the real function.

use mq_check::TypeChecker;
use mq_hir::Hir;

fn type_errors(code: &str) -> Vec<String> {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    if !hir.errors().is_empty() {
        // Needs a builtin the checker was built without (features), not a type problem.
        return Vec::new();
    }
    TypeChecker::new().check(&hir).iter().map(|e| e.to_string()).collect()
}

#[test]
fn documented_builtin_examples_have_no_type_errors() {
    let mut failures: Vec<String> = Vec::new();
    let docs = mq_lang::BUILTIN_FUNCTION_DOC
        .iter()
        .chain(mq_lang::INTERNAL_FUNCTION_DOC.iter());
    for (name, doc) in docs {
        for example in doc.examples {
            let errors = type_errors(example.code);
            if !errors.is_empty() {
                failures.push(format!("{name}: {}\n    {}", example.code, errors.join("\n    ")));
            }
        }
    }
    failures.sort();
    assert!(
        failures.is_empty(),
        "{} examples fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
