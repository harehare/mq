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
    #[error("\"{1}\" is not defined")]
    NotDefined(ErrorToken, FunctionName),
    #[error("Unable to format date time, {1}")]
    DateTimeFormatError(ErrorToken, String),
    #[error("Index out of bounds {1}")]
    IndexOutOfBounds(ErrorToken, Number),
    #[error("Invalid definition for \"{1}\"")]
    InvalidDefinition(ErrorToken, String),
    #[error("Maximum recursion depth exceeded \"{0}\"")]
    RecursionError(u32),
    #[error(r#"Invalid types for "{}", got {}"#, name, args.join(", "))]
    InvalidTypes {
        token: ErrorToken,
        name: FunctionName,
        args: ArgType,
    },
    #[error("Invalid number of arguments in \"{1}\", expected {2}, got {3}")]
    InvalidNumberOfArguments(ErrorToken, FunctionName, u8, u8),
    #[error("Invalid number of arguments in \"{1}\", expected {2} to {3}, got {4}")]
    InvalidNumberOfArgumentsWithDefaults(ErrorToken, FunctionName, u8, u8, u8),
    #[error("Invalid regular expression \"{1}\"")]
    InvalidRegularExpression(ErrorToken, String),
    #[error("Internal error")]
    InternalError(ErrorToken),
    #[error("Failed to load module \"{0}\"")]
    ModuleLoadError(#[from] ModuleError),
    #[error("Runtime error: {1}")]
    Runtime(ErrorToken, String),
    #[error("Divided by 0")]
    ZeroDivision(ErrorToken),
    #[error("Unexpected token break")]
    Break,
    #[error("Unexpected token continue")]
    Continue,
    #[error("Not found env `{1}`")]
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
}

impl RuntimeError {
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            RuntimeError::UserDefined { token, .. } => Some(token),
            RuntimeError::InvalidBase64String(token, _) => Some(token),
            RuntimeError::NotDefined(token, _) => Some(token),
            RuntimeError::DateTimeFormatError(token, _) => Some(token),
            RuntimeError::IndexOutOfBounds(token, _) => Some(token),
            RuntimeError::InvalidDefinition(token, _) => Some(token),
            RuntimeError::RecursionError(_) => None,
            RuntimeError::InvalidTypes { token, .. } => Some(token),
            RuntimeError::InvalidNumberOfArguments(token, _, _, _) => Some(token),
            RuntimeError::InvalidNumberOfArgumentsWithDefaults(token, _, _, _, _) => Some(token),
            RuntimeError::InvalidRegularExpression(token, _) => Some(token),
            RuntimeError::InternalError(token) => Some(token),
            RuntimeError::ModuleLoadError(err) => err.token(),
            RuntimeError::Runtime(token, _) => Some(token),
            RuntimeError::ZeroDivision(token) => Some(token),
            RuntimeError::Break => None,
            RuntimeError::Continue => None,
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
        }
    }
}
