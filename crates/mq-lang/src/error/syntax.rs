use smol_str::SmolStr;
use thiserror::Error;

use crate::{Token, module::ModuleId, selector};

/// Errors that occur during parsing of mq source code.
#[derive(Error, Debug, PartialEq)]
pub enum SyntaxError {
    /// An environment variable was not found.
    #[error("Not found env `{}` at line {}:{}", .1, .0.range.start.line, .0.range.start.column)]
    EnvNotFound(Token, SmolStr),
    /// An unexpected token was encountered during parsing.
    #[error("Unexpected token `{}` at line {}:{}", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() }, .0.range.start.line, .0.range.start.column)]
    UnexpectedToken(Token),
    /// Unexpected end-of-file was encountered.
    #[error("Unexpected end of input")]
    UnexpectedEOFDetected(ModuleId),
    /// Insufficient tokens available to complete parsing.
    #[error("Insufficient tokens `{}` at line {}:{}", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() }, .0.range.start.line, .0.range.start.column)]
    InsufficientTokens(Token),
    /// Expected a closing parenthesis `)` but found a different delimiter.
    #[error("Expected a closing parenthesis `)` but got `{}` delimiter at line {}:{}", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() }, .0.range.start.line, .0.range.start.column)]
    ExpectedClosingParen(Token),
    /// Expected a closing brace `}` but found a different delimiter.
    #[error("Expected a closing brace `}}` but got `{}` delimiter at line {}:{}", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() }, .0.range.start.line, .0.range.start.column)]
    ExpectedClosingBrace(Token),
    /// Expected a closing bracket `]` but found a different delimiter.
    #[error("Expected a closing bracket `]` but got `{}` delimiter at line {}:{}", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() }, .0.range.start.line, .0.range.start.column)]
    ExpectedClosingBracket(Token),
    /// An invalid assignment target was encountered (expected an identifier).
    #[error("Invalid assignment target: expected an identifier but got `{}` at line {}:{}", if .0.is_eof() { "EOF".to_string() } else { .0.to_string() }, .0.range.start.line, .0.range.start.column)]
    InvalidAssignmentTarget(Token),
    /// An unknown selector was encountered.
    #[error(transparent)]
    UnknownSelector(selector::UnknownSelector),
    /// A parameter without a default value was found after a parameter with a default value.
    #[error(
        "Non-default parameter `{}` cannot follow a parameter with a default value",
        if .0.is_eof() { "EOF".to_string() } else { .0.to_string() }
    )]
    ParameterWithoutDefaultAfterDefault(Token),
    /// Macro parameters cannot have default values.
    #[error("Macro parameters cannot have default values at line {}:{}", .0.range.start.line, .0.range.start.column)]
    MacroParametersCannotHaveDefaults(Token),
    /// A variadic parameter must be the last parameter.
    #[error("Variadic parameter must be the last parameter at line {}:{}", .0.range.start.line, .0.range.start.column)]
    VariadicParameterMustBeLast(Token),
    /// Multiple variadic parameters are not allowed.
    #[error("Multiple variadic parameters are not allowed at line {}:{}", .0.range.start.line, .0.range.start.column)]
    MultipleVariadicParameters(Token),
    /// Macro parameters cannot be variadic.
    #[error("Macro parameters cannot be variadic at line {}:{}", .0.range.start.line, .0.range.start.column)]
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
            SyntaxError::ParameterWithoutDefaultAfterDefault(token) => Some(token),
            SyntaxError::MacroParametersCannotHaveDefaults(token) => Some(token),
            SyntaxError::VariadicParameterMustBeLast(token) => Some(token),
            SyntaxError::MultipleVariadicParameters(token) => Some(token),
            SyntaxError::MacroParametersCannotBeVariadic(token) => Some(token),
        }
    }
}
