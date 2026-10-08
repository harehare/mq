//! `mq-lang` provides a parser and evaluator for [mq](https://github.com/harehare/mq).
//!
//! ## Examples
//!
//! ```rust
//! use mq_lang::DefaultEngine;
//!
//! let mut engine = DefaultEngine::default();
//! engine.load_builtin_module();
//!
//! let input = mq_lang::parse_markdown_input("Hello,").unwrap();
//! let output = engine.eval(r#"add("world!")"#, input.into_iter()).unwrap();
//! assert_eq!(output.into_markdown_nodes()[0].value(), "Hello,world!");
//!
//! // Parse code into AST nodes
//! use mq_lang::{Arena, Shared, SharedCell};
//!
//! let token_arena = Shared::new(SharedCell::new(Arena::new(16)));
//! let program = mq_lang::parse("1 + 2", token_arena).unwrap();
//! assert_eq!(program.len(), 1);
//! ```
//!
//! Enable the `cst` feature to use `parse_recovery` or `CstParser` for a complete
//! source parse. The CST parser does not retain state between parses.
//!
//! ## Features
//!
//! - `ast-json`: Enables serialization and deserialization of the AST (Abstract Syntax Tree)
//!   to/from JSON format (`ast_to_json` / `ast_from_json`). Use `Engine::compile` and
//!   `Engine::eval_compiled` to execute programs constructed from deserialized ASTs.
//!   When this feature is enabled, `serde` and `serde_json` dependencies are included.
mod arena;
mod ast;
#[cfg(feature = "cst")]
mod cst;
pub mod diagnostic;
mod engine;
mod error;
mod ident;
mod io;
mod lexer;
mod module;
#[cfg(feature = "mqc")]
pub mod mqc;
mod number;
mod range;
mod runtime;
mod selector;
pub mod suggest;
mod tarn;
#[cfg(feature = "vm-profile")]
pub mod vm_profile;

use lexer::Lexer;
#[cfg(not(feature = "sync"))]
use std::cell::RefCell;
#[cfg(not(feature = "sync"))]
use std::rc::Rc;
#[cfg(feature = "sync")]
use std::sync::Arc;
#[cfg(feature = "sync")]
use std::sync::RwLock;

pub use arena::{Arena, ArenaId};
pub use ast::Program;
pub use ast::node::Expr as AstExpr;
pub use ast::node::IdentWithToken;
pub use ast::node::Literal as AstLiteral;
pub use ast::node::Node as AstNode;
pub use ast::node::Params as AstParams;
pub use ast::node::Pattern as AstPattern;
pub use ast::parser::Parser as AstParser;
#[cfg(feature = "ast-json")]
pub use ast::{ast_from_json, ast_to_json};
pub use diagnostic::Diagnostic;
pub use engine::{CompiledProgram, DefineValueError, Engine, Session};
pub use error::Error;
pub use ident::Ident;
#[cfg(feature = "mock-io")]
pub use io::MemIo;
pub use io::{EnvAccess, Io, IoError, IoReader, NativeIo, NetAccess, PathAccess, SandboxedIo};
pub use lexer::Options as LexerOptions;
pub use lexer::token::{StringSegment, Token, TokenKind};
#[cfg(feature = "http-import")]
pub use module::resolver::http_import;
#[cfg(feature = "http-import")]
pub use module::resolver::http_resolver::{HttpFetcher, HttpModuleResolver};
#[cfg(feature = "http-import")]
pub use module::resolver::lockfile::{LOCKFILE_NAME, LockCheck, ModuleLock, compute_hash};
#[cfg(feature = "http-import")]
pub use module::resolver::ssrf;
pub use module::{
    BUILTIN_FILE as BUILTIN_MODULE_FILE, Module, ModuleId, ModuleLoader, STANDARD_MODULES, error::ModuleError,
    resolver::DefaultModuleResolver, resolver::ModuleResolver,
};
#[cfg(feature = "mqc")]
pub use mqc::{Mqc, MqcDependency, MqcError};
pub use range::{Position, Range};
pub use runtime::builtin::{BUILTIN_FUNCTION_NAMES, INTERNAL_FUNCTION_NAMES};
pub use runtime::dict::{DictIntoIter, DictIter, DictIterMut, DictKey};
pub use runtime::host::{HostFnResult, HostFunction, HostFunctionError, HostFunctions, IntoHostFunction, ValueAdapter};
pub use runtime::runtime_value::{DictMap, FromValueError, RuntimeValue, RuntimeValues, from_value};
pub use selector::{AttrKind, SELECTOR_NAMES, Selector};
#[cfg(feature = "debug-trace")]
pub use tarn::{BytecodeChunk, BytecodeDump, BytecodeInstruction, BytecodeLocation, BytecodePhase};

pub type DefaultEngine = Engine<DefaultModuleResolver>;
pub type DefaultModuleLoader = ModuleLoader<DefaultModuleResolver>;
pub use suggest::{suggest_name, suggest_selector};

#[cfg(feature = "cst")]
pub use cst::node::BinaryOp as CstBinaryOp;
#[cfg(feature = "cst")]
pub use cst::node::Node as CstNode;
#[cfg(feature = "cst")]
pub use cst::node::NodeKind as CstNodeKind;
#[cfg(feature = "cst")]
pub use cst::node::Trivia as CstTrivia;
#[cfg(feature = "cst")]
pub use cst::node::TriviaList as CstTriviaList;
#[cfg(feature = "cst")]
pub use cst::node::UnaryOp as CstUnaryOp;
#[cfg(feature = "cst")]
pub use cst::parser::ErrorReporter as CstErrorReporter;
#[cfg(feature = "cst")]
pub use cst::parser::Parser as CstParser;

#[cfg(feature = "debugger")]
pub use runtime::debugger::{
    Breakpoint, DebugContext, Debugger, DebuggerAction, DebuggerCommand, DebuggerHandler, Source,
};

use crate::ast::TokenId;

pub type MqResult = Result<RuntimeValues, Box<Error>>;

/// Type alias for reference-counted pointer, switches between Shared and Arc depending on "sync" feature.
#[cfg(not(feature = "sync"))]
pub type Shared<T> = Rc<T>;
#[cfg(feature = "sync")]
pub type Shared<T> = Arc<T>;

/// Type alias for interior mutability, switches between SharedCell and RwLock depending on "sync" feature.
#[cfg(not(feature = "sync"))]
pub type SharedCell<T> = RefCell<T>;
#[cfg(feature = "sync")]
pub type SharedCell<T> = RwLock<T>;

pub(crate) type TokenArena = Shared<SharedCell<Arena<Shared<Token>>>>;

/// Parses `code` into CST nodes, collecting errors instead of stopping at the first one.
///
/// Broken input still yields a lossless tree: unparsable tokens are kept in `Error` nodes
/// and absent required tokens appear as zero-width `Missing` nodes.
///
/// ```rust
/// let (cst_nodes, errors) = mq_lang::parse_recovery("1 + 2");
/// assert!(!errors.has_errors());
/// assert!(!cst_nodes.is_empty());
/// ```
#[cfg(feature = "cst")]
pub fn parse_recovery(code: &str) -> (Vec<Shared<CstNode>>, CstErrorReporter) {
    let tokens = Lexer::new(lexer::Options {
        ignore_errors: true,
        include_spaces: true,
    })
    .tokenize(code, Module::TOP_LEVEL_MODULE_ID);

    let tokens = match tokens {
        Ok(tokens) => tokens,
        Err(error) => {
            let parse_error = error
                .token()
                .map_or(cst::error::ParseError::UnexpectedEOFDetected, |token| {
                    cst::error::ParseError::UnexpectedToken(Shared::new(token.clone()))
                });
            return (Vec::new(), CstErrorReporter::with_error(vec![parse_error], 100));
        }
    };

    let token_vec: Vec<Shared<Token>> = tokens.into_iter().map(Shared::new).collect();
    CstParser::new(&token_vec).parse()
}

pub fn parse(code: &str, token_arena: TokenArena) -> Result<Program, Box<error::Error>> {
    parse_in_module(code, token_arena, Module::TOP_LEVEL_MODULE_ID)
        .map_err(|e| Box::new(error::Error::from_error(code, e, DefaultModuleLoader::default())))
}

/// Parses `code` as the source of `module_id`.
pub(crate) fn parse_in_module(
    code: &str,
    token_arena: TokenArena,
    module_id: ModuleId,
) -> Result<Program, error::InnerError> {
    let tokens = Lexer::new(lexer::Options::default()).tokenize(code, module_id)?;
    let mut token_arena = {
        #[cfg(not(feature = "sync"))]
        {
            token_arena.borrow_mut()
        }

        #[cfg(feature = "sync")]
        {
            token_arena.write().unwrap()
        }
    };

    Ok(AstParser::new(tokens.iter(), &mut token_arena, module_id).parse()?)
}

/// Parses an MDX string and returns an iterator over `Value` nodes.
pub fn parse_mdx_input(input: &str) -> miette::Result<Vec<RuntimeValue>> {
    let mdx = mq_markdown::Markdown::from_mdx_str(input)?;
    Ok(mdx.nodes.into_iter().map(RuntimeValue::from).collect())
}

#[cfg(feature = "html-to-markdown")]
pub fn parse_html_input(input: &str) -> miette::Result<Vec<RuntimeValue>> {
    let html = mq_markdown::Markdown::from_html_str(input)?;
    Ok(html.nodes.into_iter().map(RuntimeValue::from).collect())
}

#[cfg(feature = "html-to-markdown")]
pub fn parse_html_input_with_options(
    input: &str,
    options: mq_markdown::ConversionOptions,
) -> miette::Result<Vec<RuntimeValue>> {
    let html = mq_markdown::Markdown::from_html_str_with_options(input, options)?;
    Ok(html.nodes.into_iter().map(RuntimeValue::from).collect())
}

/// Parses a Markdown string and returns an iterator over `Value` nodes.
pub fn parse_markdown_input(input: &str) -> miette::Result<Vec<RuntimeValue>> {
    let md = mq_markdown::Markdown::from_markdown_str(input)?;
    Ok(md.nodes.into_iter().map(RuntimeValue::from).collect())
}

/// Parses a plain text string and returns an iterator over `Value` node.
pub fn parse_text_input(input: &str) -> miette::Result<Vec<RuntimeValue>> {
    Ok(input.lines().map(|line| line.to_string().into()).collect())
}

/// Returns whether `name` is a function implemented natively by mq (e.g. `len`, `upcase`).
///
/// Functions written in mq itself, such as those in the builtin module, are not included.
///
/// ```rust
/// assert!(mq_lang::is_builtin_function("len"));
/// assert!(!mq_lang::is_builtin_function("my_function"));
/// ```
pub fn is_builtin_function(name: &str) -> bool {
    use ast::constants::builtins::{NEXT, SEND};
    name == NEXT || name == SEND || runtime::builtin::get_builtin_functions(&Ident::new(name)).is_some()
}

/// Returns a vector containing a single `Value` representing an empty input.
pub fn null_input() -> Vec<RuntimeValue> {
    vec!["".to_string().into()]
}

/// Parses a raw input string and returns a vector containing a single `Value` node.
pub fn raw_input(input: &str) -> Vec<RuntimeValue> {
    vec![input.to_string().into()]
}

/// Returns a vector containing a single `RuntimeValue::Bytes` for raw binary input.
pub fn bytes_input(bytes: &[u8]) -> Vec<RuntimeValue> {
    vec![RuntimeValue::Bytes(Shared::new(bytes.to_vec()))]
}

#[inline(always)]
pub(crate) fn token_alloc(arena: &TokenArena, token: &Shared<Token>) -> TokenId {
    #[cfg(not(feature = "sync"))]
    {
        arena.borrow_mut().alloc(Shared::clone(token))
    }

    #[cfg(feature = "sync")]
    {
        arena.write().unwrap().alloc(Shared::clone(token))
    }
}

/// Resolves `token_id`, or returns an EOF token at the start of the top-level query when the
/// token is gone, e.g. from an `eval` arena that was dropped.
#[inline(always)]
pub(crate) fn get_token(arena: TokenArena, token_id: TokenId) -> Shared<Token> {
    #[cfg(not(feature = "sync"))]
    let found = arena.borrow().get_cloned(token_id);
    #[cfg(feature = "sync")]
    let found = arena.read().unwrap().get_cloned(token_id);

    found.unwrap_or_else(|| {
        Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: Module::TOP_LEVEL_MODULE_ID,
        })
    })
}

/// The arena `arena` is layered on, or `arena` itself.
pub(crate) fn root_token_arena(arena: &TokenArena) -> TokenArena {
    #[cfg(not(feature = "sync"))]
    let parent = arena.borrow().parent().cloned();
    #[cfg(feature = "sync")]
    let parent = arena.read().unwrap().parent().cloned();

    parent.unwrap_or_else(|| Shared::clone(arena))
}

/// A token arena layered on `parent`, freed with its last reference.
pub(crate) fn layered_token_arena(parent: &TokenArena) -> TokenArena {
    Shared::new(SharedCell::new(Arena::layered(Shared::clone(parent))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_basic() {
        let code = "add(\"world!\")";
        let input = mq_markdown::Markdown::from_markdown_str("Hello,").unwrap();
        let mut engine = DefaultEngine::default();

        assert_eq!(
            engine
                .eval(
                    code,
                    input
                        .nodes
                        .into_iter()
                        .map(RuntimeValue::from)
                        .collect::<Vec<_>>()
                        .into_iter()
                )
                .unwrap(),
            vec![RuntimeValue::Markdown(
                Shared::new(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "Hello,world!".to_string(),
                    position: None
                },)),
                None
            )]
            .into()
        );
    }

    #[test]
    fn test_parse_error_syntax() {
        let code = "add(1,";
        let token_arena = Shared::new(SharedCell::new(Arena::new(10)));
        let result = parse(code, token_arena);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_error_lexer() {
        let code = "add(1, `unclosed string)";
        let token_arena = Shared::new(SharedCell::new(Arena::new(10)));
        let result = parse(code, token_arena);

        assert!(result.is_err());
    }

    #[test]
    #[cfg(feature = "cst")]
    fn test_parse_recovery_success() {
        let code = "add(1, 2)";
        let (cst_nodes, errors) = parse_recovery(code);

        assert!(!errors.has_errors());
        assert!(!cst_nodes.is_empty());
    }

    #[test]
    #[cfg(feature = "cst")]
    fn test_parse_recovery_with_errors() {
        let code = "add(1,";
        let (cst_nodes, errors) = parse_recovery(code);

        assert!(errors.has_errors());
        assert!(cst_nodes[0].has_error());
    }

    #[cfg(feature = "cst")]
    proptest::proptest! {
        #[test]
        fn parse_recovery_never_panics(code in "\\PC{0,64}") {
            let _ = parse_recovery(&code);
        }
    }

    #[test]
    #[cfg(feature = "cst")]
    fn test_parse_recovery_with_error_lexer() {
        let code = "add(1, \"";
        let (cst_nodes, errors) = parse_recovery(code);

        assert!(errors.has_errors());
        assert!(cst_nodes[0].has_error());
    }

    #[test]
    #[cfg(feature = "cst")]
    fn test_parse_recovery_with_invalid_unicode_escape() {
        let (nodes, errors) = parse_recovery("\"\\u{ZZZZ}\"");
        assert!(nodes.iter().any(|node| node.has_error()));
        assert!(errors.has_errors());
    }

    #[test]
    fn test_parse_markdown_input() {
        let input = "# Heading\n\nSome text.";
        let result = parse_markdown_input(input);
        assert!(result.is_ok());
        let values: Vec<RuntimeValue> = result.unwrap();
        assert!(!values.is_empty());
    }

    #[test]
    fn test_parse_mdx_input() {
        let input = "# Heading\n\nSome text.";
        let result = parse_mdx_input(input);
        assert!(result.is_ok());
        let values: Vec<RuntimeValue> = result.unwrap();
        assert!(!values.is_empty());
    }

    #[test]
    fn test_parse_text_input() {
        let input = "line1\nline2\nline3";
        let result = parse_text_input(input);
        assert!(result.is_ok());
        let values: Vec<RuntimeValue> = result.unwrap();
        assert_eq!(values.len(), 3);
    }

    #[cfg(feature = "html-to-markdown")]
    #[test]
    fn test_parse_html_input() {
        let input = "<h1>Heading</h1><p>Some text.</p>";
        let result = parse_html_input(input);
        assert!(result.is_ok());
        let values: Vec<RuntimeValue> = result.unwrap();
        assert!(!values.is_empty());
    }

    #[cfg(feature = "html-to-markdown")]
    #[test]
    fn test_parse_html_input_with_options() {
        let input = r#"<html>
      <head>
        <title>Title</title>
        <meta name="description" content="This is a test meta description.">
        <script>let foo = 'bar'</script>
      </head>
      <body>
        <p>Some text.</p>
      </body>
    </html>"#;
        let result = parse_html_input_with_options(
            input,
            mq_markdown::ConversionOptions {
                extract_scripts_as_code_blocks: true,
                generate_front_matter: true,
                use_title_as_h1: true,
                base_url: None,
            },
        );
        assert!(result.is_ok());
        assert_eq!(
            mq_markdown::Markdown::new(
                result
                    .unwrap()
                    .iter()
                    .map(|value| match value {
                        RuntimeValue::Markdown(node, _) => (**node).clone(),
                        _ => value.to_string().into(),
                    })
                    .collect()
            )
            .to_string(),
            "---
description: This is a test meta description.
title: Title
---

# Title

```
let foo = 'bar'
```

Some text.
"
        );
    }

    /// Binary operators grouped by precedence, lowest first, excluding assignments.
    #[cfg(feature = "cst")]
    const BINARY_OP_LEVELS: &[&[&str]] = &[
        &["||"],
        &["&&"],
        &["==", "!=", ">", ">=", "<", "<=", "=~", "!~"],
        &["+", "-", ">>", "<<"],
        &["*", "/", "%", "@"],
        &["..", "??"],
    ];

    #[cfg(feature = "cst")]
    fn cst_shape(node: &CstNode) -> String {
        match &node.kind {
            CstNodeKind::BinaryOp { lhs, rhs, .. } => format!(
                "({} {} {})",
                cst_shape(lhs),
                node.token.as_ref().unwrap(),
                cst_shape(rhs)
            ),
            _ => node.token.as_ref().map(|t| t.to_string()).unwrap_or_default(),
        }
    }

    #[cfg(feature = "cst")]
    fn ast_shape(node: &AstNode, token_arena: &TokenArena) -> String {
        let op = || get_token(Shared::clone(token_arena), node.token_id).to_string();
        let fold = |operands: &[Shared<AstNode>]| {
            let mut shape = ast_shape(&operands[0], token_arena);
            for operand in &operands[1..] {
                shape = format!("({} {} {})", shape, op(), ast_shape(operand, token_arena));
            }
            shape
        };
        match &node.expr {
            AstExpr::BinaryOp(_, lhs, rhs) => fold(&[Shared::clone(lhs), Shared::clone(rhs)]),
            AstExpr::And(operands) | AstExpr::Or(operands) => fold(operands),
            AstExpr::Call(_, args)
                if args.len() == 2
                    && get_token(Shared::clone(token_arena), node.token_id)
                        .kind
                        .binary_op_precedence()
                        .is_some() =>
            {
                fold(args)
            }
            AstExpr::Ident(ident) => ident.name.to_string(),
            _ => "?".to_string(),
        }
    }

    #[test]
    #[cfg(feature = "cst")]
    fn test_binary_op_precedence_matches_between_ast_and_cst() {
        let ops: Vec<(usize, &str)> = BINARY_OP_LEVELS
            .iter()
            .enumerate()
            .flat_map(|(level, ops)| ops.iter().map(move |op| (level, *op)))
            .collect();

        for &(lx, x) in &ops {
            for &(ly, y) in &ops {
                let code = format!("a {x} b {y} c");
                let expected = if lx < ly {
                    format!("(a {x} (b {y} c))")
                } else {
                    format!("((a {x} b) {y} c)")
                };

                let (cst_nodes, errors) = parse_recovery(&code);
                assert!(!errors.has_errors(), "CST errors for {code:?}");
                assert_eq!(cst_shape(&cst_nodes[0]), expected, "CST shape for {code:?}");

                let token_arena = Shared::new(SharedCell::new(Arena::new(16)));
                let program = parse(&code, Shared::clone(&token_arena)).unwrap_or_else(|e| panic!("{code:?}: {e:?}"));
                assert_eq!(ast_shape(&program[0], &token_arena), expected, "AST shape for {code:?}");
            }
        }
    }

    #[rstest::rstest]
    #[case::assign("=")]
    #[case::plus_assign("+=")]
    #[case::minus_assign("-=")]
    #[case::mul_assign("*=")]
    #[case::div_assign("/=")]
    #[case::mod_assign("%=")]
    #[case::floor_div_assign("//=")]
    #[case::pipe_assign("|=")]
    #[cfg(feature = "cst")]
    fn test_cst_assignment_has_lowest_precedence(#[case] op: &str) {
        let code = format!("x {op} a || b && c");
        let (nodes, errors) = parse_recovery(&code);
        assert!(!errors.has_errors(), "CST errors for {code:?}");

        let CstNodeKind::Assign { lhs, rhs, .. } = &nodes[0].kind else {
            panic!("expected an assignment for {code:?}");
        };
        assert_eq!(cst_shape(lhs), "x");
        assert_eq!(cst_shape(rhs), "(a || (b && c))");
    }
}
