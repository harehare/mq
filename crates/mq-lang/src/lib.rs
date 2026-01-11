//! `mq-lang` provides a parser and evaluator for a [mq](https://github.com/harehare/mq).
//!
//! ## Examples
//!
//! ```rs
//! let code = "add(\"world!\")";
//! let input = vec![mq_lang::Value::Markdown(
//!   mq_markdown::Markdown::from_str("Hello,").unwrap()
//! )].into_iter();
//! let mut engine = mq_lang::DefaultEngine::default();
//!
//! assert!(matches!(engine.eval(&code, input).unwrap(), mq_lang::Value::String("Hello,world!".to_string())));
//!
//! // Parse code into AST nodes
//! use mq_lang::{tokenize, LexerOptions, AstParser, Arena};
//! use std::rc::Shared;
//! use std::cell::SharedCell;
//!
//! let code = "1 + 2";
//! let token_arena = Shared::new(SharedCell::new(Arena::new()));
//! let ast = mq_lang::parse(code, token_arena).unwrap();
//!
//! assert_eq!(ast.nodes.len(), 1);
//!
//! // Parse code into CST nodes
//! use mq_lang::{tokenize, LexerOptions, CstParser};
//! use std::sync::Arc;
//!
//! let code = "1 + 2";
//! let (cst_nodes, errors) = mq_lang::parse_recovery(code);
//!
//! assert!(!errors.has_errors());
//! assert!(!cst_nodes.is_empty());
//! ```
//!
//! ## Features
//!
//! - `ast-json`: Enables serialization and deserialization of the AST (Abstract Syntax Tree)
//!   to/from JSON format. This also enables the `Engine::eval_ast` method for direct
//!   AST execution. When this feature is enabled, `serde` and `serde_json` dependencies
//!   are included.
mod arena;
mod ast;
#[cfg(feature = "cst")]
mod cst;
mod engine;
mod error;
mod eval;
mod ident;
mod lexer;
mod macro_expand;
mod module;
mod number;
mod range;
mod selector;

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
pub use ast::parser::Parser as AstParser;
#[cfg(feature = "ast-json")]
pub use ast::{ast_from_json, ast_to_json};
pub use engine::Engine;
pub use error::Error;
pub use eval::builtin::{
    BUILTIN_FUNCTION_DOC, BUILTIN_SELECTOR_DOC, BuiltinFunctionDoc, BuiltinSelectorDoc, INTERNAL_FUNCTION_DOC,
};
pub use eval::runtime_value::{RuntimeValue, RuntimeValues};
pub use ident::Ident;
pub use lexer::Options as LexerOptions;
pub use lexer::token::{StringSegment, Token, TokenKind};
pub use module::{
    BUILTIN_FILE as BUILTIN_MODULE_FILE, Module, ModuleId, ModuleLoader, error::ModuleError,
    resolver::LocalFsModuleResolver, resolver::ModuleResolver, resolver::module_name,
};
pub use range::{Position, Range};
pub use selector::{AttrKind, Selector};

pub type DefaultEngine = Engine<LocalFsModuleResolver>;
pub type DefaultModuleLoader = ModuleLoader<LocalFsModuleResolver>;

#[cfg(feature = "cst")]
pub use cst::node::BinaryOp as CstBinaryOp;
#[cfg(feature = "cst")]
pub use cst::node::Node as CstNode;
#[cfg(feature = "cst")]
pub use cst::node::NodeKind as CstNodeKind;
#[cfg(feature = "cst")]
pub use cst::node::Trivia as CstTrivia;
#[cfg(feature = "cst")]
pub use cst::node::UnaryOp as CstUnaryOp;
#[cfg(feature = "cst")]
pub use cst::parser::ErrorReporter as CstErrorReporter;
#[cfg(feature = "cst")]
pub use cst::parser::Parser as CstParser;

#[cfg(feature = "debugger")]
pub use eval::debugger::{
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

#[cfg(feature = "cst")]
pub fn parse_recovery(code: &str) -> (Vec<Shared<CstNode>>, CstErrorReporter) {
    let tokens = Lexer::new(lexer::Options {
        ignore_errors: true,
        include_spaces: true,
    })
    .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
    .map_err(|e| Box::new(error::Error::from_error(code, e.into(), DefaultModuleLoader::default())))
    .unwrap();

    CstParser::new(tokens.into_iter().map(Shared::new).collect::<Vec<_>>().iter()).parse()
}

pub fn parse(code: &str, token_arena: TokenArena) -> Result<Program, Box<error::Error>> {
    let tokens = Lexer::new(lexer::Options::default())
        .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
        .map_err(|e| Box::new(error::Error::from_error(code, e.into(), DefaultModuleLoader::default())))?;
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

    AstParser::new(
        tokens.into_iter().map(Shared::new).collect::<Vec<_>>().iter(),
        &mut token_arena,
        Module::TOP_LEVEL_MODULE_ID,
    )
    .parse()
    .map_err(|e| Box::new(error::Error::from_error(code, e.into(), DefaultModuleLoader::default())))
}

/// Parses an MDX string and returns an iterator over `Value` nodes.
pub fn parse_mdx_input(input: &str) -> miette::Result<Vec<RuntimeValue>> {
    let mdx = mq_markdown::Markdown::from_mdx_str(input)?;
    Ok(mdx.nodes.into_iter().map(RuntimeValue::from).collect())
}

pub fn parse_html_input(input: &str) -> miette::Result<Vec<RuntimeValue>> {
    let html = mq_markdown::Markdown::from_html_str(input)?;
    Ok(html.nodes.into_iter().map(RuntimeValue::from).collect())
}

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

/// Returns a vector containing a single `Value` representing an empty input.
pub fn null_input() -> Vec<RuntimeValue> {
    vec!["".to_string().into()]
}

/// Parses a raw input string and returns a vector containing a single `Value` node.
pub fn raw_input(input: &str) -> Vec<RuntimeValue> {
    vec![input.to_string().into()]
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

#[inline(always)]
pub(crate) fn get_token(arena: TokenArena, token_id: TokenId) -> Shared<Token> {
    #[cfg(not(feature = "sync"))]
    {
        Shared::clone(&arena.borrow()[token_id])
    }

    #[cfg(feature = "sync")]
    {
        Shared::clone(&arena.read().unwrap()[token_id])
    }
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
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "Hello,world!".to_string(),
                    position: None
                },),
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
        assert!(cst_nodes.is_empty());
    }

    #[test]
    #[cfg(feature = "cst")]
    fn test_parse_recovery_with_error_lexer() {
        let code = "add(1, \"";
        let (cst_nodes, errors) = parse_recovery(code);

        assert!(errors.has_errors());
        assert!(cst_nodes.is_empty());
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

    #[test]
    fn test_parse_html_input() {
        let input = "<h1>Heading</h1><p>Some text.</p>";
        let result = parse_html_input(input);
        assert!(result.is_ok());
        let values: Vec<RuntimeValue> = result.unwrap();
        assert!(!values.is_empty());
    }

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
            },
        );
        assert!(result.is_ok());
        assert_eq!(
            mq_markdown::Markdown::new(
                result
                    .unwrap()
                    .iter()
                    .map(|value| match value {
                        RuntimeValue::Markdown(node, _) => node.clone(),
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
}
