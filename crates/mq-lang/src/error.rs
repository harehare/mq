use miette::{Diagnostic, SourceOffset, SourceSpan};
use std::borrow::Cow;

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
                let source_code = module_loader
                    .get_source_code(token.module_id, source_code)
                    .unwrap_or_default();
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
                    .map(|module_id| match module_loader.module_name(*module_id) {
                        Cow::Borrowed(Module::TOP_LEVEL_MODULE) => source_code.clone(),
                        Cow::Borrowed(Module::BUILTIN_MODULE) => {
                            ModuleLoader::BUILTIN_FILE.to_string()
                        }
                        Cow::Borrowed(module_name) => module_loader
                            .clone()
                            .read_file(module_name)
                            .unwrap_or_default(),
                        Cow::Owned(module_name) => module_loader
                            .clone()
                            .read_file(&module_name)
                            .unwrap_or_default(),
                    })
                    .unwrap_or(source_code);

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
        let c: Cow<'static, str> = match self.cause {
            InnerError::Lexer(LexerError::UnexpectedToken(_)) => {
                Cow::Borrowed("LexerError::UnexpectedToken")
            }
            InnerError::Lexer(LexerError::UnexpectedEOFDetected(_)) => {
                Cow::Borrowed("LexerError::UnexpectedEOFDetected")
            }
            InnerError::Parse(ParseError::EnvNotFound(_, _)) => {
                Cow::Borrowed("ParseError::EnvNotFound")
            }
            InnerError::Parse(ParseError::UnexpectedToken(_)) => {
                Cow::Borrowed("ParseError::UnexpectedToken")
            }
            InnerError::Parse(ParseError::UnexpectedEOFDetected(_)) => {
                Cow::Borrowed("ParseError::UnexpectedEOFDetected")
            }
            InnerError::Parse(ParseError::InsufficientTokens(_)) => {
                Cow::Borrowed("ParseError::InsufficientTokens")
            }
            InnerError::Parse(ParseError::ExpectedClosingParen(_)) => {
                Cow::Borrowed("ParseError::ExpectedClosingParen")
            }
            InnerError::Parse(ParseError::ExpectedClosingBrace(_)) => {
                Cow::Borrowed("ParseError::ExpectedClosingBrace")
            }
            InnerError::Parse(ParseError::ExpectedClosingBracket(_)) => {
                Cow::Borrowed("ParseError::ExpectedClosingBracket")
            }
            InnerError::Eval(EvalError::RecursionError(_)) => {
                Cow::Borrowed("EvalError::RecursionError")
            }
            InnerError::Eval(EvalError::ModuleLoadError(_)) => {
                Cow::Borrowed("EvalError::ModuleLoadError")
            }
            InnerError::Eval(EvalError::UserDefined { .. }) => {
                Cow::Borrowed("EvalError::UserDefined")
            }
            InnerError::Eval(EvalError::InvalidBase64String(_, _)) => {
                Cow::Borrowed("EvalError::InvalidBase64String")
            }
            InnerError::Eval(EvalError::NotDefined(_, _)) => Cow::Borrowed("EvalError::NotDefined"),
            InnerError::Eval(EvalError::DateTimeFormatError(_, _)) => {
                Cow::Borrowed("EvalError::DateTimeFormatError")
            }
            InnerError::Eval(EvalError::IndexOutOfBounds(_, _)) => {
                Cow::Borrowed("EvalError::IndexOutOfBounds")
            }
            InnerError::Eval(EvalError::InvalidDefinition(_, _)) => {
                Cow::Borrowed("EvalError::InvalidDefinition")
            }
            InnerError::Eval(EvalError::InvalidTypes { .. }) => {
                Cow::Borrowed("EvalError::InvalidTypes")
            }
            InnerError::Eval(EvalError::InvalidNumberOfArguments(_, _, _, _)) => {
                Cow::Borrowed("EvalError::InvalidNumberOfArguments")
            }
            InnerError::Eval(EvalError::InvalidRegularExpression(_, _)) => {
                Cow::Borrowed("EvalError::InvalidRegularExpression")
            }
            InnerError::Eval(EvalError::InternalError(_)) => {
                Cow::Borrowed("EvalError::InternalError")
            }
            InnerError::Eval(EvalError::RuntimeError(_, _)) => {
                Cow::Borrowed("EvalError::RuntimeError")
            }
            InnerError::Eval(EvalError::ZeroDivision(_)) => {
                Cow::Borrowed("EvalError::ZeroDivision")
            }
            InnerError::Eval(EvalError::Break(_)) => Cow::Borrowed("EvalError::Break"),
            InnerError::Eval(EvalError::Continue(_)) => Cow::Borrowed("EvalError::Continue"),
            InnerError::Eval(EvalError::EnvNotFound(_, _)) => {
                Cow::Borrowed("EvalError::EnvNotFound")
            }
            InnerError::Module(ModuleError::NotFound(_)) => Cow::Borrowed("ModuleError::NotFound"),
            InnerError::Module(ModuleError::IOError(_)) => Cow::Borrowed("ModuleError::IOError"),
            InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedToken(_))) => {
                Cow::Borrowed("ModuleError::LexerError::UnexpectedToken")
            }
            InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedEOFDetected(_))) => {
                Cow::Borrowed("ModuleError::LexerError::UnexpectedEOFDetected")
            }
            InnerError::Module(ModuleError::ParseError(ParseError::EnvNotFound(_, _))) => {
                Cow::Borrowed("ModuleError::ParseError::EnvNotFound")
            }
            InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedToken(_))) => {
                Cow::Borrowed("ModuleError::ParseError::UnexpectedToken")
            }
            InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedEOFDetected(_))) => {
                Cow::Borrowed("ModuleError::ParseError::UnexpectedEOFDetected")
            }
            InnerError::Module(ModuleError::ParseError(ParseError::InsufficientTokens(_))) => {
                Cow::Borrowed("ModuleError::ParseError::InsufficientTokens")
            }
            InnerError::Module(ModuleError::InvalidModule) => {
                Cow::Borrowed("ModuleError::InvalidModule")
            }
            InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingParen(_))) => {
                Cow::Borrowed("ModuleError::ExpectedClosingParen")
            }
            InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingBrace(_))) => {
                Cow::Borrowed("ModuleError::ExpectedClosingBrace")
            }
            InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingBracket(_))) => {
                Cow::Borrowed("ModuleError::ExpectedClosingBracket")
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
        let msg: Option<Cow<'static, str>> = match &self.cause {
            InnerError::Lexer(LexerError::UnexpectedToken(_)) => Some(Cow::Borrowed(
                "Check for unexpected or misplaced tokens in your input.",
            )),
            InnerError::Lexer(LexerError::UnexpectedEOFDetected(_)) => Some(Cow::Borrowed(
                "Input ended unexpectedly. Make sure all expressions are complete.",
            )),
            InnerError::Parse(ParseError::EnvNotFound(_, env)) => Some(Cow::Owned(format!(
                "Environment variable '{env}' not found. Did you forget to set it?"
            ))),
            InnerError::Parse(ParseError::UnexpectedToken(_)) => Some(Cow::Borrowed(
                "Check for syntax errors or misplaced tokens.",
            )),
            InnerError::Parse(ParseError::UnexpectedEOFDetected(_)) => Some(Cow::Borrowed(
                "Input ended unexpectedly. Check for missing closing brackets or incomplete expressions.",
            )),
            InnerError::Parse(ParseError::InsufficientTokens(_)) => Some(Cow::Borrowed(
                "Not enough tokens to complete parsing. Check for missing arguments or delimiters.",
            )),
            InnerError::Eval(EvalError::UserDefined { .. }) => Some(Cow::Borrowed(
                "A user-defined error occurred during evaluation.",
            )),
            InnerError::Eval(EvalError::InvalidBase64String(_, _)) => Some(Cow::Borrowed(
                "The provided string is not valid Base64. Check your input.",
            )),
            InnerError::Eval(EvalError::NotDefined(_, name)) => Some(Cow::Owned(format!(
                "'{name}' is not defined. Did you forget to declare it?"
            ))),
            InnerError::Eval(EvalError::DateTimeFormatError(_, _)) => Some(Cow::Borrowed(
                "Invalid date/time format. Please check your format string.",
            )),
            InnerError::Eval(EvalError::IndexOutOfBounds(_, _)) => Some(Cow::Borrowed(
                "Index out of bounds. Check your array or string indices.",
            )),
            InnerError::Eval(EvalError::InvalidDefinition(_, _)) => Some(Cow::Borrowed(
                "Invalid definition. Please check your function or variable declaration.",
            )),
            InnerError::Eval(EvalError::InvalidTypes { .. }) => Some(Cow::Borrowed(
                "Type mismatch. Check the types of your operands.",
            )),
            InnerError::Eval(EvalError::InvalidNumberOfArguments(_, _, expected, actual)) => {
                Some(Cow::Owned(format!(
                    "Invalid number of arguments: expected {expected}, got {actual}."
                )))
            }
            InnerError::Eval(EvalError::InvalidRegularExpression(_, _)) => Some(Cow::Borrowed(
                "Invalid regular expression. Please check your regex syntax.",
            )),
            InnerError::Eval(EvalError::InternalError(_)) => Some(Cow::Borrowed(
                "An internal error occurred. Please report this if it persists.",
            )),
            InnerError::Eval(EvalError::RuntimeError(_, _)) => {
                Some(Cow::Borrowed("A runtime error occurred during evaluation."))
            }
            InnerError::Eval(EvalError::ZeroDivision(_)) => {
                Some(Cow::Borrowed("Division by zero is not allowed."))
            }
            InnerError::Module(ModuleError::NotFound(name)) => Some(Cow::Owned(format!(
                "Module '{name}' not found. Check the module name or path."
            ))),
            InnerError::Module(ModuleError::IOError(_)) => Some(Cow::Borrowed(
                "An I/O error occurred while loading a module. Check file permissions and paths.",
            )),
            InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedToken(_))) => {
                Some(Cow::Borrowed("Lexer error in module: unexpected token."))
            }
            InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedEOFDetected(_))) => {
                Some(Cow::Borrowed(
                    "Lexer error in module: unexpected end of file.",
                ))
            }
            InnerError::Module(ModuleError::ParseError(ParseError::EnvNotFound(_, env))) => Some(
                Cow::Owned(format!("Environment variable '{env}' not found in module.")),
            ),
            InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedToken(_))) => {
                Some(Cow::Borrowed("Parse error in module: unexpected token."))
            }
            InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedEOFDetected(_))) => {
                Some(Cow::Borrowed(
                    "Parse error in module: unexpected end of file.",
                ))
            }
            InnerError::Module(ModuleError::ParseError(ParseError::InsufficientTokens(_))) => {
                Some(Cow::Borrowed("Parse error in module: insufficient tokens."))
            }
            InnerError::Module(ModuleError::InvalidModule) => {
                Some(Cow::Borrowed("Invalid module format or content."))
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
    use mq_test::defer;
    use rstest::{fixture, rstest};

    use super::*;
    use crate::{Arena, Range, Shared, SharedCell, Token, TokenKind, arena::ArenaId};

    #[fixture]
    fn module_loader() -> ModuleLoader {
        ModuleLoader::default()
    }

    #[test]
    fn test_from_error_with_eof_error() {
        let cause = InnerError::Parse(ParseError::UnexpectedEOFDetected(ArenaId::new(0)));
        let module_loader = ModuleLoader::default();
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

        let token_arena = Shared::new(SharedCell::new(Arena::new(10)));
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
        let token_arena = Shared::new(SharedCell::new(Arena::new(10)));
        let mut loader = ModuleLoader::default();
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

    #[rstest]
    #[case::lexer_unexpected_token(
        InnerError::Lexer(LexerError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::lexer_unexpected_eof(InnerError::Lexer(LexerError::UnexpectedEOFDetected(
        ArenaId::new(0)
    )))]
    #[case::parse_env_not_found(
        InnerError::Parse(ParseError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "ENV_VAR".into()))
    )]
    #[case::parse_unexpected_token(
        InnerError::Parse(ParseError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::parse_unexpected_eof_detected(InnerError::Parse(ParseError::UnexpectedEOFDetected(
        ArenaId::new(0)
    )))]
    #[case::parse_insufficient_tokens(
        InnerError::Parse(ParseError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::parse_expected_closing_paren(
        InnerError::Parse(ParseError::ExpectedClosingParen(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::parse_expected_closing_brace(
        InnerError::Parse(ParseError::ExpectedClosingBrace(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::parse_expected_closing_bracket(
        InnerError::Parse(ParseError::ExpectedClosingBracket(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::eval_recursion_error(InnerError::Eval(EvalError::RecursionError(0)))]
    #[case::eval_module_load_error(
        InnerError::Eval(EvalError::ModuleLoadError(ModuleError::NotFound("mod".into())))
    )]
    #[case::eval_user_defined(
        InnerError::Eval(EvalError::UserDefined {
            token: Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: ArenaId::new(0),
            },
            message: "msg".to_string(),
        })
    )]
    #[case::eval_invalid_base64_string(
        InnerError::Eval(EvalError::InvalidBase64String(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "bad".to_string()))
    )]
    #[case::eval_not_defined(
        InnerError::Eval(EvalError::NotDefined(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "name".to_string()))
    )]
    #[case::eval_datetime_format_error(
        InnerError::Eval(EvalError::DateTimeFormatError(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "fmt".to_string()))
    )]
    #[case::eval_index_out_of_bounds(
        InnerError::Eval(EvalError::IndexOutOfBounds(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, 1.into()))
    )]
    #[case::eval_invalid_definition(
        InnerError::Eval(EvalError::InvalidDefinition(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "bad".into()))
    )]
    #[case::eval_invalid_types(
        InnerError::Eval(EvalError::InvalidTypes {
            token: Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: ArenaId::new(0),
            },
            name: "int".into(),
            args: vec!["str".into()],
        })
    )]
    #[case::eval_invalid_number_of_arguments(
        InnerError::Eval(EvalError::InvalidNumberOfArguments(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "func".to_string(), 2, 1))
    )]
    #[case::eval_invalid_regular_expression(
        InnerError::Eval(EvalError::InvalidRegularExpression(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "bad".to_string()))
    )]
    #[case::eval_internal_error(
        InnerError::Eval(EvalError::InternalError(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::eval_runtime_error(
        InnerError::Eval(EvalError::RuntimeError(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "err".to_string()))
    )]
    #[case::eval_zero_division(
        InnerError::Eval(EvalError::ZeroDivision(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::eval_break(
        InnerError::Eval(EvalError::Break(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::eval_continue(
        InnerError::Eval(EvalError::Continue(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::eval_env_not_found(
        InnerError::Eval(EvalError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "ENV".into()))
    )]
    #[case::module_not_found(
        InnerError::Module(ModuleError::NotFound("mod".to_string()))
    )]
    #[case::module_io_error(
        InnerError::Module(ModuleError::IOError("io".to_string()))
    )]
    #[case::module_lexer_error_unexpected_token(
        InnerError::Module(ModuleError::LexerError(LexerError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_lexer_error_unexpected_eof(InnerError::Module(ModuleError::LexerError(
        LexerError::UnexpectedEOFDetected(ArenaId::new(0))
    )))]
    #[case::module_parse_error_env_not_found(
        InnerError::Module(ModuleError::ParseError(ParseError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "ENV".into())))
    )]
    #[case::module_parse_error_unexpected_token(
        InnerError::Module(ModuleError::ParseError(ParseError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_parse_error_unexpected_eof(InnerError::Module(ModuleError::ParseError(
        ParseError::UnexpectedEOFDetected(ArenaId::new(0))
    )))]
    #[case::module_parse_error_insufficient_tokens(
        InnerError::Module(ModuleError::ParseError(ParseError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_invalid_module(InnerError::Module(ModuleError::InvalidModule))]
    #[case::module_parse_error_expected_closing_paren(
        InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingParen(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_parse_error_expected_closing_brace(
        InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingBrace(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_parse_error_expected_closing_bracket(
        InnerError::Module(ModuleError::ParseError(ParseError::ExpectedClosingBracket(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    fn test_diagnostic_code_and_help(module_loader: ModuleLoader, #[case] cause: InnerError) {
        let error = Error::from_error("source code", cause, module_loader);
        // code() and help() must not panic
        let _ = error.code();
        let _ = error.help();
    }
}
