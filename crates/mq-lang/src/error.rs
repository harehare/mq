pub mod runtime;
pub mod syntax;

use miette::{Diagnostic, SourceOffset, SourceSpan};
use std::borrow::Cow;

use crate::{
    Module, ModuleLoader, ModuleResolver, Token,
    error::{runtime::RuntimeError, syntax::SyntaxError},
    module::{self, error::ModuleError},
};

#[allow(clippy::useless_conversion)]
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum InnerError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Syntax(#[from] SyntaxError),
    #[error(transparent)]
    Module(#[from] ModuleError),
}

impl InnerError {
    #[cold]
    pub fn token(&self) -> Option<&Token> {
        match self {
            InnerError::Syntax(err) => err.token(),
            InnerError::Runtime(err) => err.token(),
            InnerError::Module(err) => err.token(),
        }
    }
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
    #[cold]
    pub fn from_error(
        top_level_source_code: impl Into<String>,
        cause: InnerError,
        module_loader: ModuleLoader<impl ModuleResolver>,
    ) -> Self {
        let source_code = top_level_source_code.into();
        let token = cause.token();

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
                    InnerError::Syntax(SyntaxError::UnexpectedEOFDetected(module_id)) => (Some(module_id), true),
                    InnerError::Runtime(_) => (None, false),
                    InnerError::Module(ModuleError::SyntaxError(SyntaxError::UnexpectedEOFDetected(module_id))) => {
                        (Some(module_id), true)
                    }
                    _ => (None, false),
                };

                let source_code = module_id
                    .map(|module_id| match module_loader.module_name(*module_id) {
                        Cow::Borrowed(Module::TOP_LEVEL_MODULE) => source_code.clone(),
                        Cow::Borrowed(Module::BUILTIN_MODULE) => module::BUILTIN_FILE.to_string(),
                        Cow::Borrowed(module_name) => module_loader.clone().resolve(module_name).unwrap_or_default(),
                        Cow::Owned(module_name) => module_loader.clone().resolve(&module_name).unwrap_or_default(),
                    })
                    .unwrap_or(source_code);

                let location = if is_eof {
                    let lines = source_code.lines();
                    let loc_line = lines.clone().count().saturating_sub(1);
                    let loc_col = lines.last().map(|lines| lines.len()).unwrap_or(0);
                    SourceSpan::new(SourceOffset::from_location(&source_code, loc_line, loc_col), 1)
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

fn type_name<T>(_: &T) -> &'static str {
    std::any::type_name::<T>()
}

impl Diagnostic for Error {
    #[cold]
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(
            match &self.cause {
                InnerError::Runtime(e) => type_name(&e),
                InnerError::Syntax(e) => type_name(&e),
                InnerError::Module(e) => type_name(&e),
            }
            .replace("&mq_lang::", ""),
        ) as Box<dyn std::fmt::Display>)
    }

    #[cold]
    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        match &self.cause {
            InnerError::Runtime(RuntimeError::InvalidDefinition(_, _))
            | InnerError::Runtime(RuntimeError::InvalidTypes { .. }) => {
                Some(Box::new("https://mqlang.org/book") as Box<dyn std::fmt::Display>)
            }
            _ => None,
        }
    }

    #[cold]
    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        let msg: Option<Cow<'static, str>> = match &self.cause {
            InnerError::Syntax(SyntaxError::EnvNotFound(_, env)) => Some(Cow::Owned(format!(
                "Environment variable '{env}' not found. Did you forget to set it?"
            ))),
            InnerError::Syntax(SyntaxError::UnexpectedToken(_)) => {
                Some(Cow::Borrowed("Check for syntax errors or misplaced tokens."))
            }
            InnerError::Syntax(SyntaxError::UnexpectedEOFDetected(_)) => Some(Cow::Borrowed(
                "Input ended unexpectedly. Check for missing closing brackets or incomplete expressions.",
            )),
            InnerError::Syntax(SyntaxError::InsufficientTokens(_)) => Some(Cow::Borrowed(
                "Not enough tokens to complete parsing. Check for missing arguments or delimiters.",
            )),
            InnerError::Syntax(SyntaxError::UnknownSelector(_)) => Some(Cow::Borrowed(
                "Unknown selector used. Verify that the selector is valid.",
            )),
            InnerError::Syntax(SyntaxError::ExpectedClosingParen(_)) => Some(Cow::Borrowed(
                "Expected a closing parenthesis ')'. Check your parentheses for balance.",
            )),
            InnerError::Syntax(SyntaxError::ExpectedClosingBrace(_)) => Some(Cow::Borrowed(
                "Expected a closing brace '}'. Check your braces for balance.",
            )),
            InnerError::Syntax(SyntaxError::ExpectedClosingBracket(_)) => Some(Cow::Borrowed(
                "Expected a closing bracket ']'. Check your brackets for balance.",
            )),
            InnerError::Syntax(SyntaxError::InvalidAssignmentTarget(_)) => Some(Cow::Borrowed(
                "Invalid assignment target. Ensure you're assigning to a valid variable or property.",
            )),
            InnerError::Syntax(SyntaxError::MacroParamsMustBeIdents(_)) => Some(Cow::Borrowed(
                "Macro parameters must be identifiers. Check your macro definition.",
            )),
            InnerError::Syntax(SyntaxError::ParameterWithoutDefaultAfterDefault(_)) => Some(Cow::Borrowed(
                "Parameters with default values must come after all parameters without defaults.",
            )),
            InnerError::Syntax(SyntaxError::MacroParametersCannotHaveDefaults(_)) => {
                Some(Cow::Borrowed("Macro parameters cannot have default values."))
            }
            InnerError::Runtime(RuntimeError::UserDefined { .. }) => {
                Some(Cow::Borrowed("A user-defined error occurred during evaluation."))
            }
            InnerError::Runtime(RuntimeError::InvalidBase64String(_, _)) => Some(Cow::Borrowed(
                "The provided string is not valid Base64. Check your input.",
            )),
            InnerError::Runtime(RuntimeError::NotDefined(_, name)) => Some(Cow::Owned(format!(
                "'{name}' is not defined. Did you forget to declare it?"
            ))),
            InnerError::Runtime(RuntimeError::DateTimeFormatError(_, _)) => Some(Cow::Borrowed(
                "Invalid date/time format. Please check your format string.",
            )),
            InnerError::Runtime(RuntimeError::IndexOutOfBounds(_, _)) => Some(Cow::Borrowed(
                "Index out of bounds. Check your array or string indices.",
            )),
            InnerError::Runtime(RuntimeError::InvalidDefinition(_, _)) => Some(Cow::Borrowed(
                "Invalid definition. Please check your function or variable declaration.",
            )),
            InnerError::Runtime(RuntimeError::AssignToImmutable(_, name)) => Some(Cow::Owned(format!(
                "Cannot assign to immutable variable '{name}'. Consider declaring it as mutable."
            ))),
            InnerError::Runtime(RuntimeError::UndefinedVariable(_, name)) => Some(Cow::Owned(format!(
                "Variable '{name}' is undefined. Did you forget to declare it?"
            ))),
            InnerError::Runtime(RuntimeError::InvalidTypes { .. }) => {
                Some(Cow::Borrowed("Type mismatch. Check the types of your operands."))
            }
            InnerError::Runtime(RuntimeError::InvalidNumberOfArguments {
                token: _,
                name: _,
                expected,
                actual,
            }) => Some(Cow::Owned(format!(
                "Invalid number of arguments: expected {expected}, got {actual}."
            ))),
            InnerError::Runtime(RuntimeError::InvalidNumberOfArgumentsWithDefaults {
                token: _,
                name: _,
                min,
                max,
                actual,
            }) => Some(Cow::Owned(format!(
                "Invalid number of arguments: expected {min} to {max}, got {actual}."
            ))),
            InnerError::Runtime(RuntimeError::InvalidRegularExpression(_, _)) => Some(Cow::Borrowed(
                "Invalid regular expression. Please check your regex syntax.",
            )),
            InnerError::Runtime(RuntimeError::InternalError(_)) => Some(Cow::Borrowed(
                "An internal error occurred. Please report this if it persists.",
            )),
            InnerError::Runtime(RuntimeError::Runtime(_, _)) => {
                Some(Cow::Borrowed("A runtime error occurred during evaluation."))
            }
            InnerError::Runtime(RuntimeError::ZeroDivision(_)) => {
                Some(Cow::Borrowed("Division by zero is not allowed."))
            }
            InnerError::Runtime(RuntimeError::RecursionError(_)) => {
                Some(Cow::Borrowed("Maximum recursion depth exceeded."))
            }
            InnerError::Runtime(RuntimeError::ModuleLoadError(_)) => {
                Some(Cow::Borrowed("Failed to load module. Check module paths and names."))
            }
            InnerError::Runtime(RuntimeError::Break) => None,
            InnerError::Runtime(RuntimeError::Continue) => None,
            InnerError::Runtime(RuntimeError::EnvNotFound(..)) => {
                Some(Cow::Borrowed("Environment variable not found during evaluation."))
            }
            InnerError::Runtime(RuntimeError::QuoteNotAllowedInRuntimeContext(_)) => Some(Cow::Borrowed(
                "quote() is not allowed in runtime context. It should only appear inside macros.",
            )),
            InnerError::Runtime(RuntimeError::UnquoteNotAllowedOutsideQuote(_)) => {
                Some(Cow::Borrowed("unquote() can only be used inside quote()."))
            }
            InnerError::Module(ModuleError::NotFound(name)) => Some(Cow::Owned(format!(
                "Module '{name}' not found. Check the module name or path."
            ))),
            InnerError::Module(ModuleError::AlreadyLoaded(name)) => {
                Some(Cow::Owned(format!("Module '{name}' is already loaded.")))
            }
            InnerError::Module(ModuleError::IOError(_)) => Some(Cow::Borrowed(
                "An I/O error occurred while loading a module. Check file permissions and paths.",
            )),
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::EnvNotFound(_, env))) => {
                Some(Cow::Owned(format!("Environment variable '{env}' not found in module.")))
            }
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::UnexpectedToken(_))) => {
                Some(Cow::Borrowed("Parse error in module: unexpected token."))
            }
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::UnexpectedEOFDetected(_))) => {
                Some(Cow::Borrowed("Parse error in module: unexpected end of file."))
            }
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::InsufficientTokens(_))) => {
                Some(Cow::Borrowed("Parse error in module: insufficient tokens."))
            }
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::ExpectedClosingBracket(_))) => {
                Some(Cow::Borrowed("Parse error in module: expected closing bracket ']'."))
            }
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::ExpectedClosingBrace(_))) => {
                Some(Cow::Borrowed("Parse error in module: expected closing brace '}'."))
            }
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::ExpectedClosingParen(_))) => Some(Cow::Borrowed(
                "Parse error in module: expected closing parenthesis ')'.",
            )),
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::InvalidAssignmentTarget(_))) => {
                Some(Cow::Borrowed("Parse error in module: invalid assignment target."))
            }
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::UnknownSelector(_))) => {
                Some(Cow::Borrowed("Parse error in module: unknown selector used."))
            }
            InnerError::Module(ModuleError::InvalidModule) => Some(Cow::Borrowed("Invalid module format or content.")),
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::MacroParamsMustBeIdents(_))) => Some(
                Cow::Borrowed("Parse error in module: macro parameters must be identifiers."),
            ),
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::ParameterWithoutDefaultAfterDefault(_))) => Some(
                Cow::Borrowed("Parse error in module: parameters with defaults must come after parameters without."),
            ),
            InnerError::Module(ModuleError::SyntaxError(SyntaxError::MacroParametersCannotHaveDefaults(_))) => Some(
                Cow::Borrowed("Parse error in module: macro parameters cannot have default values."),
            ),
            InnerError::Runtime(RuntimeError::UndefinedMacro(_)) => {
                Some(Cow::Borrowed("Macro expansion error: undefined macro used."))
            }
            InnerError::Runtime(RuntimeError::ArityMismatch { .. }) => {
                Some(Cow::Borrowed("Macro expansion error: macro arity mismatch."))
            }
            InnerError::Runtime(RuntimeError::RecursionLimit) => {
                Some(Cow::Borrowed("Macro expansion error: recursion limit exceeded."))
            }
            InnerError::Runtime(RuntimeError::InvalidMacroResultAst(_)) => {
                Some(Cow::Borrowed("Invalid macro result AST during macro expansion."))
            }
            InnerError::Runtime(RuntimeError::InvalidMacroResult(_)) => Some(Cow::Borrowed(
                "Invalid macro result: expected AST value during macro body evaluation.",
            )),
        };

        msg.map(|m| Box::new(m) as Box<dyn std::fmt::Display>)
    }

    #[cold]
    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(miette::LabeledSpan::new_with_span(
            Some(format!("{}", self.cause)),
            self.location,
        ))))
    }

    #[cold]
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source_code)
    }
}

#[cfg(test)]
mod test {
    use rstest::{fixture, rstest};
    use scopeguard::defer;
    use std::io::Write;
    use std::{fs::File, path::PathBuf};

    use super::*;
    use crate::{Arena, LocalFsModuleResolver, Range, Shared, SharedCell, Token, TokenKind, arena::ArenaId};

    type TempDir = PathBuf;
    type TempFile = PathBuf;

    fn create_file(name: &str, content: &str) -> (TempDir, TempFile) {
        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join(name);
        let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");

        (temp_dir, temp_file_path)
    }

    #[fixture]
    fn module_loader() -> ModuleLoader {
        ModuleLoader::default()
    }

    #[test]
    fn test_from_error_with_eof_error() {
        let cause = InnerError::Syntax(SyntaxError::UnexpectedEOFDetected(ArenaId::new(0)));
        let module_loader: ModuleLoader = ModuleLoader::default();
        let error = Error::from_error("line 1\nline 2", cause, module_loader);

        assert_eq!(error.source_code, "line 1\nline 2");
    }

    #[rstest]
    #[case::parse_unexpected_token(
        InnerError::Syntax(SyntaxError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::parse_unexpected_eof_detected(
        InnerError::Syntax(SyntaxError::UnexpectedEOFDetected(ArenaId::new(0))),
        "source code"
    )]
    #[case::parse_env_not_found(
        InnerError::Syntax(SyntaxError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "ENV_VAR".into())),
        "source code"
    )]
    #[case::parse_env_not_found(
        InnerError::Syntax(SyntaxError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::parse_env_not_found(
        InnerError::Syntax(SyntaxError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::eval_zero_division(
        InnerError::Runtime(RuntimeError::ZeroDivision(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::eval_invalid_base64_string(
        InnerError::Runtime(RuntimeError::InvalidBase64String(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::eval_not_defined(
        InnerError::Runtime(RuntimeError::NotDefined(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::eval_index_out_of_bounds(
        InnerError::Runtime(RuntimeError::IndexOutOfBounds(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, 1.into())),
        "source code"
    )]
    #[case::eval_invalid_definition(
        InnerError::Runtime(RuntimeError::InvalidDefinition(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::eval_invalid_number_of_arguments(
        InnerError::Runtime(RuntimeError::InvalidNumberOfArguments{token: Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, name: "".to_string(), expected: 1, actual:1}),
        "source code"
    )]
    #[case::eval_invalid_regular_expression(
        InnerError::Runtime(RuntimeError::InvalidRegularExpression(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::eval_internal_error(
        InnerError::Runtime(RuntimeError::InternalError(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })),
        "source code"
    )]
    #[case::eval_internal_error(
        InnerError::Runtime(RuntimeError::Runtime(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "".to_string())),
        "source code"
    )]
    #[case::module_not_found(InnerError::Module(ModuleError::NotFound(Cow::Borrowed("test"))), "source code")]
    #[case::module_io_error(InnerError::Module(ModuleError::IOError(Cow::Borrowed("test"))), "source code")]
    #[case::module_parse_error(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "test".into()))),
        "source code"
    )]
    #[case::module_parse_error(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))),
        "source code"
    )]
    #[case::module_parse_error(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::UnexpectedEOFDetected(ArenaId::new(0)))),
        "source code"
    )]
    #[case::module_parse_error(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }
        ))),
        "source code"
    )]
    fn test_from_error(
        module_loader: module::ModuleLoader<impl ModuleResolver>,
        #[case] cause: InnerError,
        #[case] source_code: &str,
    ) {
        let error = Error::from_error(source_code, cause, module_loader);
        assert_eq!(error.source_code, source_code);
    }

    #[test]
    fn test_from_error_with_module_source() {
        let (temp_dir, temp_file_path) = create_file(
            "test_from_error_with_module_source.mq",
            "def func1(): 42; | let val1 = 1",
        );

        defer! {
            if temp_file_path.exists() {
                std::fs::remove_file(&temp_file_path).expect("Failed to delete temp file");
            }
        }

        let token_arena = Shared::new(SharedCell::new(Arena::new(10)));
        let mut loader = ModuleLoader::new(LocalFsModuleResolver::new(Some(vec![temp_dir.clone()])));
        loader
            .load_from_file("test_from_error_with_module_source", token_arena)
            .unwrap();

        let token = Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(1),
        };

        let cause = InnerError::Runtime(RuntimeError::ZeroDivision(token));
        let error = Error::from_error("top level source", cause, loader);

        assert_eq!(error.source_code, "def func1(): 42; | let val1 = 1");
    }

    #[test]
    fn test_from_error_with_builtin_module() {
        let token_arena = Shared::new(SharedCell::new(Arena::new(10)));
        let mut loader: ModuleLoader = ModuleLoader::default();
        loader.load_builtin(token_arena).unwrap();
        let token = Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(1),
        };

        let cause = InnerError::Runtime(RuntimeError::ZeroDivision(token));
        let error = Error::from_error("top level source", cause, loader);

        assert_eq!(error.source_code, module::BUILTIN_FILE);
    }

    #[rstest]
    #[case::parse_env_not_found(
        InnerError::Syntax(SyntaxError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "ENV_VAR".into()))
    )]
    #[case::parse_unexpected_token(
        InnerError::Syntax(SyntaxError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::parse_unexpected_eof_detected(InnerError::Syntax(SyntaxError::UnexpectedEOFDetected(ArenaId::new(0))))]
    #[case::parse_insufficient_tokens(
        InnerError::Syntax(SyntaxError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::parse_expected_closing_paren(
        InnerError::Syntax(SyntaxError::ExpectedClosingParen(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::parse_expected_closing_brace(
        InnerError::Syntax(SyntaxError::ExpectedClosingBrace(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::parse_expected_closing_bracket(
        InnerError::Syntax(SyntaxError::ExpectedClosingBracket(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::eval_recursion_error(InnerError::Runtime(RuntimeError::RecursionError(0)))]
    #[case::eval_module_load_error(
        InnerError::Runtime(RuntimeError::ModuleLoadError(ModuleError::NotFound("mod".into())))
    )]
    #[case::eval_user_defined(
        InnerError::Runtime(RuntimeError::UserDefined {
            token: Token {
                range: Range::default(),
                kind: TokenKind::Eof,
                module_id: ArenaId::new(0),
            },
            message: "msg".to_string(),
        })
    )]
    #[case::eval_invalid_base64_string(
        InnerError::Runtime(RuntimeError::InvalidBase64String(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "bad".to_string()))
    )]
    #[case::eval_not_defined(
        InnerError::Runtime(RuntimeError::NotDefined(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "name".to_string()))
    )]
    #[case::eval_datetime_format_error(
        InnerError::Runtime(RuntimeError::DateTimeFormatError(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "fmt".to_string()))
    )]
    #[case::eval_index_out_of_bounds(
        InnerError::Runtime(RuntimeError::IndexOutOfBounds(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, 1.into()))
    )]
    #[case::eval_invalid_definition(
        InnerError::Runtime(RuntimeError::InvalidDefinition(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "bad".into()))
    )]
    #[case::eval_invalid_types(
        InnerError::Runtime(RuntimeError::InvalidTypes {
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
        InnerError::Runtime(RuntimeError::InvalidNumberOfArguments{token: Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, name: "func".to_string(), expected: 2, actual: 1})
    )]
    #[case::eval_invalid_regular_expression(
        InnerError::Runtime(RuntimeError::InvalidRegularExpression(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "bad".to_string()))
    )]
    #[case::eval_internal_error(
        InnerError::Runtime(RuntimeError::InternalError(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::eval_runtime_error(
        InnerError::Runtime(RuntimeError::Runtime(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "err".to_string()))
    )]
    #[case::eval_zero_division(
        InnerError::Runtime(RuntimeError::ZeroDivision(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }))
    )]
    #[case::eval_break(InnerError::Runtime(RuntimeError::Break))]
    #[case::eval_continue(InnerError::Runtime(RuntimeError::Continue))]
    #[case::eval_env_not_found(
        InnerError::Runtime(RuntimeError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "ENV".into()))
    )]
    #[case::module_not_found(InnerError::Module(ModuleError::NotFound(Cow::Borrowed("mod"))))]
    #[case::module_io_error(InnerError::Module(ModuleError::IOError(Cow::Borrowed("io"))))]
    #[case::module_parse_error_env_not_found(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::EnvNotFound(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        }, "ENV".into())))
    )]
    #[case::module_parse_error_unexpected_token(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::UnexpectedToken(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_parse_error_unexpected_eof(InnerError::Module(ModuleError::SyntaxError(
        SyntaxError::UnexpectedEOFDetected(ArenaId::new(0))
    )))]
    #[case::module_parse_error_insufficient_tokens(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::InsufficientTokens(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_invalid_module(InnerError::Module(ModuleError::InvalidModule))]
    #[case::module_parse_error_expected_closing_paren(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::ExpectedClosingParen(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_parse_error_expected_closing_brace(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::ExpectedClosingBrace(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    #[case::module_parse_error_expected_closing_bracket(
        InnerError::Module(ModuleError::SyntaxError(SyntaxError::ExpectedClosingBracket(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: ArenaId::new(0),
        })))
    )]
    fn test_diagnostic_code_and_help(module_loader: ModuleLoader<impl ModuleResolver>, #[case] cause: InnerError) {
        let error = Error::from_error("source code", cause, module_loader);
        // code() and help() must not panic
        let _ = error.code();
        let _ = error.help();
    }
}
