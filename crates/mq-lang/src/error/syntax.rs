use smol_str::SmolStr;
use thiserror::Error;

use crate::{Token, module::ModuleId, selector};

/// Errors that occur during parsing of mq source code.
#[derive(Error, Debug, PartialEq)]
pub enum SyntaxError {
    /// An environment variable was not found.
    #[error("Not found env `{1}`")]
    EnvNotFound(Token, SmolStr),
    /// An unexpected token was encountered during parsing.
    #[error("Unexpected token `{}`", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    UnexpectedToken(Token),
    /// Unexpected end-of-file was encountered.
    #[error("Unexpected EOF detected")]
    UnexpectedEOFDetected(ModuleId),
    /// Insufficient tokens available to complete parsing.
    #[error("Insufficient tokens `{}`", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    InsufficientTokens(Token),
    /// Expected a closing parenthesis `)` but found a different delimiter.
    #[error("Expected a closing parenthesis `)` but got `{}` delimiter", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    ExpectedClosingParen(Token),
    /// Expected a closing brace `}` but found a different delimiter.
    #[error("Expected a closing brace `}}` but got `{}` delimiter", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    ExpectedClosingBrace(Token),
    /// Expected a closing bracket `]` but found a different delimiter.
    #[error("Expected a closing bracket `]` but got `{}` delimiter", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    ExpectedClosingBracket(Token),
    /// An invalid assignment target was encountered (expected an identifier).
    #[error("Invalid assignment target: expected an identifier but got `{}`", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    InvalidAssignmentTarget(Token),
    /// An unknown selector was encountered.
    #[error(transparent)]
    UnknownSelector(selector::UnknownSelector),
    #[error("Macro parameters must be identifiers, but got `{}`", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() })]
    MacroParamsMustBeIdents(Token),
    /// A parameter without a default value was found after a parameter with a default value.
    #[error(
        "Parameter without default value after parameter with default: parameters with defaults must come after all parameters without defaults"
    )]
    ParameterWithoutDefaultAfterDefault(Token),
    /// Macro parameters cannot have default values.
    #[error("Macro parameters cannot have default values")]
    MacroParametersCannotHaveDefaults(Token),
    /// A variadic parameter must be the last parameter.
    #[error("Variadic parameter must be the last parameter")]
    VariadicParameterMustBeLast(Token),
    /// Multiple variadic parameters are not allowed.
    #[error("Multiple variadic parameters are not allowed")]
    MultipleVariadicParameters(Token),
    /// Macro parameters cannot be variadic.
    #[error("Macro parameters cannot be variadic")]
    MacroParametersCannotBeVariadic(Token),
}

impl SyntaxError {
    /// Returns the token associated with this error, if available.
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            SyntaxError::EnvNotFound(token, _) => Some(token),
            SyntaxError::UnexpectedToken(token) => Some(token),
            SyntaxError::UnexpectedEOFDetected(_) => None,
            SyntaxError::InsufficientTokens(token) => Some(token),
            SyntaxError::ExpectedClosingParen(token) => Some(token),
            SyntaxError::ExpectedClosingBrace(token) => Some(token),
            SyntaxError::ExpectedClosingBracket(token) => Some(token),
            SyntaxError::InvalidAssignmentTarget(token) => Some(token),
            SyntaxError::UnknownSelector(selector::UnknownSelector(token)) => Some(token),
            SyntaxError::MacroParamsMustBeIdents(token) => Some(token),
            SyntaxError::ParameterWithoutDefaultAfterDefault(token) => Some(token),
            SyntaxError::MacroParametersCannotHaveDefaults(token) => Some(token),
            SyntaxError::VariadicParameterMustBeLast(token) => Some(token),
            SyntaxError::MultipleVariadicParameters(token) => Some(token),
            SyntaxError::MacroParametersCannotBeVariadic(token) => Some(token),
        }
    }
}
