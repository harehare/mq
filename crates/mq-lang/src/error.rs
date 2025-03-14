use miette::SourceOffset;

use crate::{
    Module, ModuleLoader,
    ast::error::ParseError,
    eval::{error::EvalError, module::ModuleError},
    lexer::error::LexerError,
    range::Range,
};

#[allow(clippy::useless_conversion)]
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum InnerError {
    #[error(transparent)]
    Eval(#[from] EvalError),
    #[error(transparent)]
    Lexer(#[from] LexerError),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Module(#[from] ModuleError),
}

#[derive(PartialEq, Debug, thiserror::Error, miette::Diagnostic)]
#[error("mq error")]
pub struct Error {
    pub cause: InnerError,
    pub span: Range,
    #[source_code]
    source_code: String,
    #[label("{cause}")]
    location: SourceOffset,
}

impl Error {
    pub fn from_error(
        top_level_source_code: impl Into<String>,
        cause: InnerError,
        module_loader: ModuleLoader,
    ) -> Self {
        let source_code = top_level_source_code.into();
        let token = match &cause {
            InnerError::Lexer(LexerError::UnexpectedToken(token)) => Some(token),
            InnerError::Lexer(LexerError::UnexpectedEOFDetected(_)) => None,
            InnerError::Parse(err) => match err {
                ParseError::EnvNotFound(token, _) => Some(token),
                ParseError::UnexpectedToken(token) => Some(token),
                ParseError::UnexpectedEOFDetected(_) => None,
                ParseError::InsufficientTokens(token) => Some(token),
            },
            InnerError::Eval(err) => match err {
                EvalError::InvalidBase64String(token, _) => Some(token),
                EvalError::NotDefined(token, _) => Some(token),
                EvalError::DateTimeFormatError(token, _) => Some(token),
                EvalError::IndexOutOfBounds(token, _) => Some(token),
                EvalError::InvalidDefinition(token, _) => Some(token),
                EvalError::InvalidTypes { token, .. } => Some(token),
                EvalError::InvalidNumberOfArguments(token, _, _, _) => Some(token),
                EvalError::InvalidRegularExpression(token, _) => Some(token),
                EvalError::InternalError(token) => Some(token),
                EvalError::RuntimeError(token, _) => Some(token),
                EvalError::ZeroDivision(token) => Some(token),
                _ => None,
            },
            InnerError::Module(err) => match err {
                ModuleError::NotFound(_) => None,
                ModuleError::IOError(_) => None,
                ModuleError::LexerError(LexerError::UnexpectedToken(token)) => Some(token),
                ModuleError::LexerError(LexerError::UnexpectedEOFDetected(_)) => None,
                ModuleError::ParseError(err) => match err {
                    ParseError::EnvNotFound(token, _) => Some(token),
                    ParseError::UnexpectedToken(token) => Some(token),
                    ParseError::UnexpectedEOFDetected(_) => None,
                    ParseError::InsufficientTokens(token) => Some(token),
                },
                ModuleError::InvalidModule => None,
            },
        };

        match token {
            Some(token) => {
                let source_code = match module_loader.module_name(token.module_id).as_str() {
                    Module::TOP_LEVEL_MODULE => source_code,
                    Module::BUILTIN_MODULE => ModuleLoader::BUILTIN_FILE.to_string(),
                    module_name => module_loader
                        .clone()
                        .read_file(module_name)
                        .unwrap_or_default()
                        .clone(),
                };

                let location = SourceOffset::from_location(
                    &source_code,
                    token.range.start.line as usize,
                    token.range.start.column,
                );
                let range = token.clone().range;

                Self {
                    cause,
                    span: range,
                    source_code,
                    location,
                }
            }
            None => {
                let (module_id, is_eof) = match &cause {
                    InnerError::Parse(ParseError::UnexpectedEOFDetected(module_id)) => {
                        (Some(module_id), true)
                    }
                    InnerError::Lexer(LexerError::UnexpectedEOFDetected(module_id)) => {
                        (Some(module_id), true)
                    }
                    InnerError::Eval(_) => (None, false),
                    InnerError::Module(ModuleError::ParseError(
                        ParseError::UnexpectedEOFDetected(module_id),
                    )) => (Some(module_id), true),
                    _ => (None, false),
                };

                let source_code = module_id
                    .map(
                        |module_id| match module_loader.module_name(*module_id).as_str() {
                            Module::TOP_LEVEL_MODULE => source_code.to_owned(),
                            Module::BUILTIN_MODULE => ModuleLoader::BUILTIN_FILE.to_string(),
                            module_name => module_loader
                                .clone()
                                .read_file(module_name)
                                .unwrap_or_default()
                                .clone(),
                        },
                    )
                    .unwrap_or(source_code.to_owned());

                let location = if is_eof {
                    let lines = source_code.lines();
                    let loc_line = lines.clone().count() - 1;
                    let loc_col = lines.last().unwrap().len();
                    SourceOffset::from_location(&source_code, loc_line, loc_col)
                } else {
                    SourceOffset::from_location(&source_code, 0, 0)
                };

                Self {
                    cause,
                    span: Range::default(),
                    source_code,
                    location,
                }
            }
        }
    }
}
#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::*;
    use crate::{Token, TokenKind, arena::ArenaId};

    #[test]
    fn test_from_error_with_eof_error() {
        let cause = InnerError::Parse(ParseError::UnexpectedEOFDetected(ArenaId::new(0)));
        let module_loader = ModuleLoader::new(None);
        let error = Error::from_error("line 1\nline 2", cause, module_loader);

        assert_eq!(error.source_code, "line 1\nline 2");
        assert_eq!(error.span, Range::default());
    }

    #[rstest]
    #[case::lexer_unexpected_token(
        InnerError::Lexer(LexerError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::lexer_unexpected_eof(
        InnerError::Lexer(LexerError::UnexpectedEOFDetected(ArenaId::new(0))),
        "line 1\nline 2"
    )]
    #[case::parse_unexpected_token(
        InnerError::Parse(ParseError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::parse_unexpected_eof_detected(
        InnerError::Parse(ParseError::UnexpectedEOFDetected(ArenaId::new(0))),
        "source code"
    )]
    #[case::parse_env_not_found(
        InnerError::Parse(ParseError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "ENV_VAR".into())),
        "source code"
    )]
    #[case::parse_env_not_found(
        InnerError::Parse(ParseError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::parse_env_not_found(
        InnerError::Parse(ParseError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::eval_zero_division(
        InnerError::Eval(EvalError::ZeroDivision(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::eval_invalid_base64_string(
        InnerError::Eval(EvalError::InvalidBase64String(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::eval_not_defined(
        InnerError::Eval(EvalError::NotDefined(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::eval_index_out_of_bounds(
        InnerError::Eval(EvalError::IndexOutOfBounds(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, 1.into())),
        "source code"
    )]
    #[case::eval_invalid_definition(
        InnerError::Eval(EvalError::InvalidDefinition(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::eval_invalid_number_of_arguments(
        InnerError::Eval(EvalError::InvalidNumberOfArguments(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string(), 1, 1)),
        "source code"
    )]
    #[case::eval_invalid_regular_expression(
        InnerError::Eval(EvalError::InvalidRegularExpression(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::eval_internal_error(
        InnerError::Eval(EvalError::InternalError(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::eval_internal_error(
        InnerError::Eval(EvalError::RuntimeError(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::module_not_found(
        InnerError::Module(ModuleError::NotFound("test".to_string())),
        "source code"
    )]
    fn test_from_error(#[case] cause: InnerError, #[case] source_code: &str) {
        let module_loader = ModuleLoader::new(None);
        let error = Error::from_error(source_code, cause, module_loader);

        assert_eq!(error.source_code, source_code);
    }
}
