//! Every diagnostic code is documented in the README.

use mq_check::TypeError;

const README: &str = include_str!("../README.md");

fn one_of_each() -> Vec<TypeError> {
    vec![
        TypeError::Mismatch {
            expected: String::new(),
            found: String::new(),
            location: None,
            context: None,
        },
        TypeError::UnificationError {
            left: String::new(),
            right: String::new(),
            location: None,
            context: None,
        },
        TypeError::WrongArity {
            expected: 0,
            found: 0,
            location: None,
            context: None,
        },
        TypeError::UndefinedField {
            field: String::new(),
            record_ty: String::new(),
            location: None,
        },
        TypeError::UndefinedAttribute {
            attr: String::new(),
            node_ty: String::new(),
            location: None,
        },
        TypeError::HeterogeneousArray {
            types: String::new(),
            location: None,
        },
        TypeError::NullablePropagation {
            op: String::new(),
            nullable_arg: String::new(),
            location: None,
            context: None,
        },
        TypeError::UnreachableCode {
            reason: String::new(),
            location: None,
        },
        TypeError::NonExhaustiveMatch {
            missing: String::new(),
            location: None,
            context: None,
        },
    ]
}

#[test]
fn test_every_error_code_is_documented() {
    for error in one_of_each() {
        assert!(
            README.contains(&format!("`{}`", error.error_code())),
            "{} is not in the error code table of crates/mq-check/README.md",
            error.error_code()
        );
    }
}

#[test]
fn test_error_code_matches_the_miette_code() {
    for error in one_of_each() {
        let code = miette::Diagnostic::code(&error).map(|code| code.to_string());
        assert_eq!(code.as_deref(), Some(error.error_code()));
    }
}
