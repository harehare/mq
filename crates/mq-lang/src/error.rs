use miette::{Diagnostic, SourceOffset, SourceSpan};

use crate::{
    Module, ModuleLoader,
    ast::error::ParseError,
    eval::{error::EvalError, module::ModuleError},
    lexer::error::LexerError,
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

/// Represents a high-level error with diagnostic information for the user.
#[derive(PartialEq, Debug, thiserror::Error)]
#[error("{cause}")]
pub struct Error {
    /// The underlying cause of the error.
    pub cause: InnerError,
    /// The source code related to the error.
    pub source_code: String,
    /// The location in the source code for diagnostics.
    pub location: SourceSpan,
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
                ParseError::ExpectedClosingParen(token) => Some(token),
                ParseError::ExpectedClosingBrace(token) => Some(token),
                ParseError::ExpectedClosingBracket(token) => Some(token),
            },
            InnerError::Eval(err) => match err {
                EvalError::UserDefined { token, .. } => Some(token),
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
                EvalError::Break(token) => Some(token),
                EvalError::Continue(token) => Some(token),
                EvalError::RecursionError(_) => None,
                EvalError::ModuleLoadError(_) => None,
                EvalError::EnvNotFound(_, _) => None,
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
                    ParseError::ExpectedClosingParen(token) => Some(token),
                    ParseError::ExpectedClosingBrace(token) => Some(token),
                    ParseError::ExpectedClosingBracket(token) => Some(token),
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

                let location = SourceSpan::new(
                    SourceOffset::from_location(
                        &source_code,
                        token.range.start.line as usize,
                        token.range.start.column,
                    ),
                    std::cmp::max(
                        SourceOffset::from_location(
                            &source_code,
                            token.range.end.line as usize,
                            token.range.end.column,
                        )
                        .offset()
                        .saturating_sub(
                            SourceOffset::from_location(
                                &source_code,
                                token.range.start.line as usize,
                                token.range.start.column,
                            )
                            .offset(),
                        ),
                        1,
                    ),
                );

                Self {
                    cause,
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
                    let loc_line = lines.clone().count().saturating_sub(1);
                    let loc_col = lines.last().map(|lines| lines.len()).unwrap_or(0);
                    SourceSpan::new(
                        SourceOffset::from_location(&source_code, loc_line, loc_col),
                        1,
                    )
                } else {
                    SourceSpan::new(SourceOffset::from_location(&source_code, 0, 0), 1)
                };

                Self {
                    cause,
                    source_code,
                    location,
                }
            }
        }
    }
}

impl Diagnostic for Error {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        let c = match self.cause {
            InnerError::Lexer(LexerError::UnexpectedToken(_)) => {
                "LexerError::UnexpectedToken".to_string()
            }
            InnerError::Lexer(LexerError::UnexpectedEOFDetected(_)) => {
                "LexerError::UnexpectedEOFDetected".to_string()
            }
            InnerError::Parse(ParseError::EnvNotFound(_, _)) => {
                "ParseError::EnvNotFound".to_string()
            }
            InnerError::Parse(ParseError::UnexpectedToken(_)) => {
                "ParseError::UnexpectedToken".to_string()
            }
            InnerError::Parse(ParseError::UnexpectedEOFDetected(_)) => {
                "ParseError::UnexpectedEOFDetected".to_string()
            }
            InnerError::Parse(ParseError::InsufficientTokens(_)) => {
                "ParseError::InsufficientTokens".to_string()
            }
            InnerError::Parse(ParseError::ExpectedClosingParen(_)) => {
                "ParseError::ExpectedClosingParen".to_string()
            }
            InnerError::Parse(ParseError::ExpectedClosingBrace(_)) => {
                "ParseError::ExpectedClosingBrace".to_string()
            }
            InnerError::Parse(ParseError::ExpectedClosingBracket(_)) => {
                "ParseError::ExpectedClosingBracket".to_string()
            }
            InnerError::Eval(EvalError::RecursionError(_)) => {
                "EvalError::RecursionError".to_string()
            }
            InnerError::Eval(EvalError::ModuleLoadError(_)) => {
                "EvalError::ModuleLoadError".to_string()
            }
            InnerError::Eval(EvalError::UserDefined { .. }) => "EvalError::UserDefined".to_string(),
            InnerError::Eval(EvalError::InvalidBase64String(_, _)) => {
                "EvalError::InvalidBase64String".to_string()
            }
            InnerError::Eval(EvalError::NotDefined(_, _)) => "EvalError::NotDefined".to_string(),
            InnerError::Eval(EvalError::DateTimeFormatError(_, _)) => {
                "EvalError::DateTimeFormatError".to_string()
            }
            InnerError::Eval(EvalError::IndexOutOfBounds(_, _)) => {
                "EvalError::IndexOutOfBounds".to_string()
            }
            InnerError::Eval(EvalError::InvalidDefinition(_, _)) => {
                "EvalError::InvalidDefinition".to_string()
            }
            InnerError::Eval(EvalError::InvalidTypes { .. }) => {
                "EvalError::InvalidTypes".to_string()
            }
            InnerError::Eval(EvalError::InvalidNumberOfArguments(_, _, _, _)) => {
                "EvalError::InvalidNumberOfArguments".to_string()
            }
            InnerError::Eval(EvalError::InvalidRegularExpression(_, _)) => {
                "EvalError::InvalidRegularExpression".to_string()
            }
            InnerError::Eval(EvalError::InternalError(_)) => "EvalError::InternalError".to_string(),
            InnerError::Eval(EvalError::RuntimeError(_, _)) => {
                "EvalError::RuntimeError".to_string()
            }
            InnerError::Eval(EvalError::ZeroDivision(_)) => "EvalError::ZeroDivision".to_string(),
            InnerError::Eval(EvalError::Break(_)) => "EvalError::Break".to_string(),
            InnerError::Eval(EvalError::Continue(_)) => "EvalError::Continue".to_string(),
            InnerError::Eval(EvalError::EnvNotFound(_, _)) => "EvalError::EnvNotFound".to_string(),
            InnerError::Module(ModuleError::NotFound(_)) => "ModuleError::NotFound".to_string(),
            InnerError::Module(ModuleError::IOError(_)) => "ModuleError::IOError".to_string(),
            InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedToken(_))) => {
                "ModuleError::LexerError::UnexpectedToken".to_string()
            }
            InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedEOFDetected(_))) => {
                "ModuleError::LexerError::UnexpectedEOFDetected".to_string()
            }
            InnerError::Module(ModuleError::ParseError(ParseError::EnvNotFound(_, _))) => {
                "ModuleError::ParseError::EnvNotFound".to_string()
            }
            InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedToken(_))) => {
                "ModuleError::ParseError::UnexpectedToken".to_string()
            }
            InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedEOFDetected(_))) => {
                "ModuleError::ParseError::UnexpectedEOFDetected".to_string()
            }
            InnerError::Module(ModuleError::ParseError(ParseError::InsufficientTokens(_))) => {
                "ModuleError::ParseError::InsufficientTokens".to_string()
            }
            InnerError::Module(ModuleError::InvalidModule) => {
                "ModuleError::InvalidModule".to_string()
            }
            InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingParen(_))) => {
                "ModuleError::ExpectedClosingParen".to_string()
            }
            InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingBrace(_))) => {
                "ModuleError::ExpectedClosingBrace".to_string()
            }
            InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingBracket(_))) => {
                "ModuleError::ExpectedClosingBracket".to_string()
            }
        };

        Some(Box::new(c))
    }

    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        match &self.cause {
            InnerError::Eval(EvalError::InvalidDefinition(_, _))
            | InnerError::Eval(EvalError::InvalidTypes { .. }) => {
                Some(Box::new("https://mqlang.org/book") as Box<dyn std::fmt::Display>)
            }
            _ => None,
        }
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        let msg = match &self.cause {
            InnerError::Lexer(LexerError::UnexpectedToken(_)) => {
                Some("Check for unexpected or misplaced tokens in your input.".to_string())
            }
            InnerError::Lexer(LexerError::UnexpectedEOFDetected(_)) => {
                Some("Input ended unexpectedly. Make sure all expressions are complete.".to_string())
            }
            InnerError::Parse(ParseError::EnvNotFound(_, env)) => {
                Some(format!("Environment variable '{env}' not found. Did you forget to set it?"))
            }
            InnerError::Parse(ParseError::UnexpectedToken(_)) => {
                Some("Check for syntax errors or misplaced tokens.".to_string())
            }
            InnerError::Parse(ParseError::UnexpectedEOFDetected(_)) => {
                Some("Input ended unexpectedly. Check for missing closing brackets or incomplete expressions.".to_string())
            }
            InnerError::Parse(ParseError::InsufficientTokens(_)) => {
                Some("Not enough tokens to complete parsing. Check for missing arguments or delimiters.".to_string())
            }
            InnerError::Eval(EvalError::UserDefined { .. }) => {
                Some("A user-defined error occurred during evaluation.".to_string())
            }
            InnerError::Eval(EvalError::InvalidBase64String(_, _)) => {
                Some("The provided string is not valid Base64. Check your input.".to_string())
            }
            InnerError::Eval(EvalError::NotDefined(_, name)) => {
                Some(format!("'{name}' is not defined. Did you forget to declare it?"))
            }
            InnerError::Eval(EvalError::DateTimeFormatError(_, _)) => {
                Some("Invalid date/time format. Please check your format string.".to_string())
            }
            InnerError::Eval(EvalError::IndexOutOfBounds(_, _)) => {
                Some("Index out of bounds. Check your array or string indices.".to_string())
            }
            InnerError::Eval(EvalError::InvalidDefinition(_, _)) => {
                Some("Invalid definition. Please check your function or variable declaration.".to_string())
            }
            InnerError::Eval(EvalError::InvalidTypes { .. }) => {
                Some("Type mismatch. Check the types of your operands.".to_string())
            }
            InnerError::Eval(EvalError::InvalidNumberOfArguments(_, _, expected, actual)) => {
                Some(format!(
                    "Invalid number of arguments: expected {expected}, got {actual}."
                ))
            }
            InnerError::Eval(EvalError::InvalidRegularExpression(_, _)) => {
                Some("Invalid regular expression. Please check your regex syntax.".to_string())
            }
            InnerError::Eval(EvalError::InternalError(_)) => {
                Some("An internal error occurred. Please report this if it persists.".to_string())
            }
            InnerError::Eval(EvalError::RuntimeError(_, _)) => {
                Some("A runtime error occurred during evaluation.".to_string())
            }
            InnerError::Eval(EvalError::ZeroDivision(_)) => {
                Some("Division by zero is not allowed.".to_string())
            }
            InnerError::Module(ModuleError::NotFound(name)) => {
                Some(format!("Module '{name}' not found. Check the module name or path."))
            }
            InnerError::Module(ModuleError::IOError(_)) => {
                Some("An I/O error occurred while loading a module. Check file permissions and paths.".to_string())
            }
            InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedToken(_))) => {
                Some("Lexer error in module: unexpected token.".to_string())
            }
            InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedEOFDetected(_))) => {
                Some("Lexer error in module: unexpected end of file.".to_string())
            }
            InnerError::Module(ModuleError::ParseError(ParseError::EnvNotFound(_, env))) => {
                Some(format!("Environment variable '{env}' not found in module."))
            }
            InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedToken(_))) => {
                Some("Parse error in module: unexpected token.".to_string())
            }
            InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedEOFDetected(_))) => {
                Some("Parse error in module: unexpected end of file.".to_string())
            }
            InnerError::Module(ModuleError::ParseError(ParseError::InsufficientTokens(_))) => {
                Some("Parse error in module: insufficient tokens.".to_string())
            }
            InnerError::Module(ModuleError::InvalidModule) => {
                Some("Invalid module format or content.".to_string())
            }
            _ => None,
        };

        msg.map(|m| Box::new(m) as Box<dyn std::fmt::Display>)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(
            miette::LabeledSpan::new_with_span(Some(format!("{}", self.cause)), self.location),
        )))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source_code)
    }
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use mq_test::defer;
    use rstest::{fixture, rstest};

    use super::*;
    use crate::{Arena, Range, Token, TokenKind, arena::ArenaId};

    #[fixture]
    fn module_loader() -> ModuleLoader {
        ModuleLoader::new(None)
    }

    #[test]
    fn test_from_error_with_eof_error() {
        let cause = InnerError::Parse(ParseError::UnexpectedEOFDetected(ArenaId::new(0)));
        let module_loader = ModuleLoader::new(None);
        let error = Error::from_error("line 1\nline 2", cause, module_loader);

        assert_eq!(error.source_code, "line 1\nline 2");
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
    #[case::module_io_error(
        InnerError::Module(ModuleError::IOError("test".to_string())),
        "source code"
    )]
    #[case::module_lexer_error(
        InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedEOFDetected(
            ArenaId::new(0)
        ))),
        "source code"
    )]
    #[case::module_parse_error(
        InnerError::Module(ModuleError::ParseError(ParseError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "test".into()))),
        "source code"
    )]
    #[case::module_parse_error(
        InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))),
        "source code"
    )]
    #[case::module_parse_error(
        InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedEOFDetected(
            ArenaId::new(0)
        ))),
        "source code"
    )]
    #[case::module_parse_error(
        InnerError::Module(ModuleError::ParseError(ParseError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }
        ))),
        "source code"
    )]
    fn test_from_error(
        module_loader: ModuleLoader,
        #[case] cause: InnerError,
        #[case] source_code: &str,
    ) {
        let error = Error::from_error(source_code, cause, module_loader);
        assert_eq!(error.source_code, source_code);
    }

    #[test]
    fn test_from_error_with_module_source() {
        let (temp_dir, temp_file_path) = mq_test::create_file(
            "test_from_error_with_module_source.mq",
            "def func1(): 42; | let val1 = 1",
        );

        defer! {
            if temp_file_path.exists() {
                std::fs::remove_file(&temp_file_path).expect("Failed to delete temp file");
            }
        }

        let token_arena = Rc::new(RefCell::new(Arena::new(10)));
        let mut loader = ModuleLoader::new(Some(vec![temp_dir.clone()]));
        loader
            .load_from_file("test_from_error_with_module_source", token_arena)
            .unwrap();

        let token = Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(1),
        };

        let cause = InnerError::Eval(EvalError::ZeroDivision(token));
        let error = Error::from_error("top level source", cause, loader);

        assert_eq!(error.source_code, "def func1(): 42; | let val1 = 1");
    }

    #[test]
    fn test_from_error_with_builtin_module() {
        let token_arena = Rc::new(RefCell::new(Arena::new(10)));
        let mut loader = ModuleLoader::new(None);
        loader.load_builtin(token_arena).unwrap();
        let token = Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(1),
        };

        let cause = InnerError::Eval(EvalError::ZeroDivision(token));
        let error = Error::from_error("top level source", cause, loader);

        assert_eq!(error.source_code, ModuleLoader::BUILTIN_FILE);
    }
}
