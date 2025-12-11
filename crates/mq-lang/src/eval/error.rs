use smol_str::SmolStr;
use thiserror::Error;

use crate::{Token, number::Number};

use super::module::error::ModuleError;

type FunctionName = String;
type ArgType = Vec<SmolStr>;
type ErrorToken = Token;

#[derive(Error, Debug, PartialEq)]
pub enum EvalError {
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
    #[error("Invalid regular expression \"{1}\"")]
    InvalidRegularExpression(ErrorToken, String),
    #[error("Internal error")]
    InternalError(ErrorToken),
    #[error("Failed to load module \"{0}\"")]
    ModuleLoadError(#[from] ModuleError),
    #[error("Runtime error: {1}")]
    RuntimeError(ErrorToken, String),
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
}

impl EvalError {
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            EvalError::UserDefined { token, .. } => Some(token),
            EvalError::InvalidBase64String(token, _) => Some(token),
            EvalError::NotDefined(token, _) => Some(token),
            EvalError::DateTimeFormatError(token, _) => Some(token),
            EvalError::IndexOutOfBounds(token, _) => Some(token),
            EvalError::InvalidDefinition(token, _) => Some(token),
            EvalError::RecursionError(_) => None,
            EvalError::InvalidTypes { token, .. } => Some(token),
            EvalError::InvalidNumberOfArguments(token, _, _, _) => Some(token),
            EvalError::InvalidRegularExpression(token, _) => Some(token),
            EvalError::InternalError(token) => Some(token),
            EvalError::ModuleLoadError(err) => err.token(),
            EvalError::RuntimeError(token, _) => Some(token),
            EvalError::ZeroDivision(token) => Some(token),
            EvalError::Break => None,
            EvalError::Continue => None,
            EvalError::EnvNotFound(token, _) => Some(token),
            EvalError::AssignToImmutable(token, _) => Some(token),
            EvalError::UndefinedVariable(token, _) => Some(token),
        }
    }
}
