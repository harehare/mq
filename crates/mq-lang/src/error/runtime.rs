use std::time::Duration;

use smol_str::SmolStr;
use thiserror::Error;

use crate::{Ident, Token, number::Number};

use super::module::error::ModuleError;

type FunctionName = String;
type ArgType = Vec<SmolStr>;
type ErrorToken = Token;

#[derive(Error, Debug, PartialEq)]
pub enum RuntimeError {
    #[error("{}", message)]
    UserDefined { message: String, token: ErrorToken },
    #[error("Invalid base64 string")]
    InvalidBase64String(ErrorToken, String),
    // The boxed slice is a snapshot of currently-defined names (builtins excluded, since those
    // are looked up separately) used to power "did you mean" suggestions. Boxed rather than
    // `Vec` to keep this rarely-populated field from growing every `RuntimeError` beyond
    // clippy's `result_large_err` threshold.
    #[error("\"{1}\" is not defined")]
    NotDefined(ErrorToken, FunctionName, Box<[FunctionName]>),
    // A bare identifier reference not bound in any scope. Distinct from InvalidDefinition
    // below, which is a resolved-but-not-callable value.
    #[error("\"{1}\" is not defined")]
    UndefinedReference(ErrorToken, FunctionName, Box<[FunctionName]>),
    #[error("Unable to format date time, {1}")]
    DateTimeFormatError(ErrorToken, String),
    #[error("Index out of bounds {1}")]
    IndexOutOfBounds(ErrorToken, Number),
    #[error("Invalid definition for \"{1}\"")]
    InvalidDefinition(ErrorToken, String),
    #[error("Maximum recursion depth exceeded ({0})")]
    RecursionError(u32),
    #[error("Execution timed out after {:.3}s", .0.as_secs_f64())]
    Timeout(Duration),
    #[error(r#"Invalid types for "{}", got {}"#, name, args.join(", "))]
    InvalidTypes {
        token: ErrorToken,
        name: FunctionName,
        args: ArgType,
    },
    #[error("Invalid number of arguments in \"{name}\", expected {expected}, got {actual}")]
    InvalidNumberOfArguments {
        token: ErrorToken,
        name: FunctionName,
        expected: u8,
        actual: u8,
    },
    #[error("Invalid regular expression \"{1}\"")]
    InvalidRegularExpression(ErrorToken, String),
    #[error("Internal error")]
    InternalError(ErrorToken),
    #[error("Failed to load module \"{0}\"")]
    ModuleLoadError(#[from] ModuleError),
    #[error("Runtime error: {1}")]
    Runtime(ErrorToken, String),
    #[error("Division by zero")]
    ZeroDivision(ErrorToken),
    #[error("Unexpected break outside of loop")]
    UnexpectedBreak(ErrorToken),
    #[error("Unexpected continue outside of loop")]
    UnexpectedContinue(ErrorToken),
    #[error("Environment variable `{1}` not found")]
    EnvNotFound(Token, SmolStr),
    #[error("Cannot assign to immutable variable \"{1}\"")]
    AssignToImmutable(Token, String),
    #[error("Undefined variable \"{1}\"")]
    UndefinedVariable(Token, String),
    #[error("quote() is not allowed in runtime context, it should only appear inside macros")]
    QuoteNotAllowedInRuntimeContext(Token),
    #[error("unquote() can only be used inside quote()")]
    UnquoteNotAllowedOutsideQuote(Token),
    #[error("Undefined macro: {0}")]
    UndefinedMacro(Ident),
    #[error("Macro {macro_name} expects {expected} arguments, got {got}")]
    ArityMismatch {
        macro_name: Ident,
        expected: usize,
        got: usize,
    },
    #[error("Maximum macro recursion depth exceeded")]
    RecursionLimit,
    #[error("Invalid macro result AST")]
    InvalidMacroResultAst(Token),
    #[error("Invalid macro result: expected AST value")]
    InvalidMacroResult(Token),
    #[error("Invalid convert: {1}")]
    InvalidConvert(Token, String),
    #[error("Destructuring pattern did not match value")]
    DestructuringFailed(Token),
}

impl RuntimeError {
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            RuntimeError::UserDefined { token, .. } => Some(token),
            RuntimeError::InvalidBase64String(token, _) => Some(token),
            RuntimeError::NotDefined(token, _, _) => Some(token),
            RuntimeError::UndefinedReference(token, _, _) => Some(token),
            RuntimeError::DateTimeFormatError(token, _) => Some(token),
            RuntimeError::IndexOutOfBounds(token, _) => Some(token),
            RuntimeError::InvalidDefinition(token, _) => Some(token),
            RuntimeError::RecursionError(_) => None,
            RuntimeError::Timeout(_) => None,
            RuntimeError::InvalidTypes { token, .. } => Some(token),
            RuntimeError::InvalidNumberOfArguments { token, .. } => Some(token),
            RuntimeError::InvalidRegularExpression(token, _) => Some(token),
            RuntimeError::InternalError(token) => Some(token),
            RuntimeError::ModuleLoadError(err) => err.token(),
            RuntimeError::Runtime(token, _) => Some(token),
            RuntimeError::ZeroDivision(token) => Some(token),
            RuntimeError::UnexpectedBreak(token) => Some(token),
            RuntimeError::UnexpectedContinue(token) => Some(token),
            RuntimeError::EnvNotFound(token, _) => Some(token),
            RuntimeError::AssignToImmutable(token, _) => Some(token),
            RuntimeError::UndefinedVariable(token, _) => Some(token),
            RuntimeError::QuoteNotAllowedInRuntimeContext(token) => Some(token),
            RuntimeError::UnquoteNotAllowedOutsideQuote(token) => Some(token),
            RuntimeError::UndefinedMacro(_) => None,
            RuntimeError::ArityMismatch { .. } => None,
            RuntimeError::RecursionLimit => None,
            RuntimeError::InvalidMacroResultAst(token) => Some(token),
            RuntimeError::InvalidMacroResult(token) => Some(token),
            RuntimeError::InvalidConvert(token, _) => Some(token),
            RuntimeError::DestructuringFailed(token) => Some(token),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Range, TokenKind, arena::ArenaId};
    use rstest::rstest;

    fn eof_token() -> Token {
        Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }
    }

    #[rstest]
    #[case(RuntimeError::UserDefined { message: "msg".to_string(), token: eof_token() }, true)]
    #[case(RuntimeError::InvalidBase64String(eof_token(), "bad".to_string()), true)]
    #[case(RuntimeError::NotDefined(eof_token(), "f".to_string(), Box::new([])), true)]
    #[case(RuntimeError::UndefinedReference(eof_token(), "r".to_string(), Box::new([])), true)]
    #[case(RuntimeError::DateTimeFormatError(eof_token(), "err".to_string()), true)]
    #[case(RuntimeError::IndexOutOfBounds(eof_token(), Number::from(1.0)), true)]
    #[case(RuntimeError::InvalidDefinition(eof_token(), "d".to_string()), true)]
    #[case(RuntimeError::RecursionError(10), false)]
    #[case(RuntimeError::Timeout(Duration::from_secs(1)), false)]
    #[case(RuntimeError::InvalidTypes { token: eof_token(), name: "f".to_string(), args: vec![] }, true)]
    #[case(RuntimeError::InvalidNumberOfArguments { token: eof_token(), name: "f".to_string(), expected: 1, actual: 0 }, true)]
    #[case(RuntimeError::InvalidRegularExpression(eof_token(), "pat".to_string()), true)]
    #[case(RuntimeError::InternalError(eof_token()), true)]
    #[case(RuntimeError::Runtime(eof_token(), "err".to_string()), true)]
    #[case(RuntimeError::ZeroDivision(eof_token()), true)]
    #[case(RuntimeError::UnexpectedBreak(eof_token()), true)]
    #[case(RuntimeError::UnexpectedContinue(eof_token()), true)]
    #[case(RuntimeError::EnvNotFound(eof_token(), "VAR".into()), true)]
    #[case(RuntimeError::AssignToImmutable(eof_token(), "x".to_string()), true)]
    #[case(RuntimeError::UndefinedVariable(eof_token(), "y".to_string()), true)]
    #[case(RuntimeError::QuoteNotAllowedInRuntimeContext(eof_token()), true)]
    #[case(RuntimeError::UnquoteNotAllowedOutsideQuote(eof_token()), true)]
    #[case(RuntimeError::UndefinedMacro(Ident::new("m")), false)]
    #[case(RuntimeError::ArityMismatch { macro_name: Ident::new("m"), expected: 1, got: 0 }, false)]
    #[case(RuntimeError::RecursionLimit, false)]
    #[case(RuntimeError::InvalidMacroResultAst(eof_token()), true)]
    #[case(RuntimeError::InvalidMacroResult(eof_token()), true)]
    #[case(RuntimeError::InvalidConvert(eof_token(), "msg".to_string()), true)]
    #[case(RuntimeError::DestructuringFailed(eof_token()), true)]
    fn test_token_presence(#[case] err: RuntimeError, #[case] has_token: bool) {
        assert_eq!(err.token().is_some(), has_token);
    }

    #[rstest]
    #[case(RuntimeError::RecursionError(42), "Maximum recursion depth exceeded (42)")]
    #[case(
        RuntimeError::Timeout(Duration::from_millis(1500)),
        "Execution timed out after 1.500s"
    )]
    #[case(RuntimeError::RecursionLimit, "Maximum macro recursion depth exceeded")]
    #[case(RuntimeError::UndefinedMacro(Ident::new("foo")), "Undefined macro: foo")]
    #[case(RuntimeError::ArityMismatch { macro_name: Ident::new("bar"), expected: 2, got: 1 }, "Macro bar expects 2 arguments, got 1")]
    fn test_error_display(#[case] err: RuntimeError, #[case] expected: &str) {
        assert_eq!(err.to_string(), expected);
    }
}
