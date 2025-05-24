use compact_str::CompactString;
use thiserror::Error;

use crate::{Token, number::Number};

use super::module::ModuleError;

type FunctionName = String;
type ArgType = Vec<CompactString>;
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
    #[error("Invalid types for \"{}\", got {}", name, args.join(", "))]
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
    #[error("Type \"{received_type}\" is not hashable and cannot be used as a map key.")]
    UnhashableType { token: ErrorToken, received_type: String },
}
