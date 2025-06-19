//! `mq-lang` is provides a parser and evaluator for a [mq](https://github.com/harehare/mq).
//!
//! ## Examples
//!
//! ```rs
//! use mq_lang::Engine;
//!
//! let code = "add(\"world!\")";
//! let input = vec![mq_lang::Value::Markdown(
//!   mq_markdown::Markdown::from_str("Hello,").unwrap()
//! )].into_iter();
//! let mut engine = mq_lang::Engine::default();
//!
//! assert!(matches!(engine.eval(&code, input).unwrap(), mq_lang::Value::String("Hello,world!".to_string())));
//!
//! // Parse code into AST nodes
//! use mq_lang::{tokenize, LexerOptions, AstParser, Arena};
//! use std::rc::Rc;
//! use std::cell::RefCell;
//!
//! let code = "1 + 2";
//! let token_arena = Rc::new(RefCell::new(Arena::new()));
//! let parser = mq_lang::parse(code, token_arena).unwrap();
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
//! assert!(errors.errors().is_empty());
//! assert!(!cst_nodes.is_empty());
//! ```
mod arena;
mod ast;
#[cfg(feature = "cst")]
mod cst;
mod engine;
mod error;
mod eval;
mod lexer;
mod number;
mod optimizer;
mod range;
mod value;
mod value_macros;

use error::InnerError;
use lexer::Lexer;
use std::cell::RefCell;
use std::rc::Rc;
#[cfg(feature = "cst")]
use std::sync::Arc;

pub use arena::Arena;
pub use ast::IdentName as AstIdentName;
pub use ast::Program;
pub use ast::node::Expr as AstExpr;
pub use ast::node::Ident as AstIdent;
pub use ast::node::Literal as AstLiteral;
pub use ast::node::Node as AstNode;
pub use ast::node::Params as AstParams;
pub use ast::parser::Parser as AstParser;
pub use engine::Engine;
pub use error::Error;
pub use eval::builtin::{
    BUILTIN_FUNCTION_DOC, BUILTIN_SELECTOR_DOC, BuiltinFunctionDoc, BuiltinSelectorDoc,
    INTERNAL_FUNCTION_DOC,
};
pub use eval::module::Module;
pub use eval::module::ModuleLoader;
pub use lexer::Options as LexerOptions;
pub use lexer::token::{StringSegment, Token, TokenKind};
pub use range::{Position, Range};
pub use value::{Value, Values};

#[cfg(feature = "cst")]
pub use cst::node::BinaryOp as CstBinaryOp;
#[cfg(feature = "cst")]
pub use cst::node::Node as CstNode;
#[cfg(feature = "cst")]
pub use cst::node::NodeKind as CstNodeKind;
#[cfg(feature = "cst")]
pub use cst::node::Trivia as CstTrivia;
#[cfg(feature = "cst")]
pub use cst::parser::ErrorReporter as CstErrorReporter;
#[cfg(feature = "cst")]
pub use cst::parser::Parser as CstParser;

pub type MqResult = Result<Values, Box<Error>>;

#[cfg(feature = "cst")]
pub fn parse_recovery(code: &str) -> (Vec<Arc<CstNode>>, CstErrorReporter) {
    let tokens = Lexer::new(lexer::Options {
        ignore_errors: true,
        include_spaces: true,
    })
    .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
    .map_err(|e| {
        Box::new(error::Error::from_error(
            code,
            InnerError::Lexer(e),
            ModuleLoader::new(None),
        ))
    })
    .unwrap();

    let (cst_nodes, errors) =
        CstParser::new(tokens.into_iter().map(Arc::new).collect::<Vec<_>>().iter()).parse();

    (cst_nodes, errors)
}

pub fn parse(
    code: &str,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
) -> Result<Program, Box<error::Error>> {
    let tokens = Lexer::new(lexer::Options::default())
        .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
        .map_err(|e| {
            Box::new(error::Error::from_error(
                code,
                InnerError::Lexer(e),
                ModuleLoader::new(None),
            ))
        })?;

    AstParser::new(
        tokens.into_iter().map(Rc::new).collect::<Vec<_>>().iter(),
        token_arena,
        Module::TOP_LEVEL_MODULE_ID,
    )
    .parse()
    .map_err(|e| {
        Box::new(error::Error::from_error(
            code,
            InnerError::Parse(e),
            ModuleLoader::new(None),
        ))
    })
}

/// Parses an MDX string and returns an iterator over `Value` nodes.
pub fn parse_mdx_input(input: &str) -> miette::Result<Vec<Value>> {
    let mdx = mq_markdown::Markdown::from_mdx_str(input)?;
    Ok(mdx.nodes.into_iter().map(Value::from).collect())
}

/// Parses a Markdown string and returns an iterator over `Value` nodes.
pub fn parse_markdown_input(input: &str) -> miette::Result<Vec<Value>> {
    let md = mq_markdown::Markdown::from_str(input)?;
    Ok(md.nodes.into_iter().map(Value::from).collect())
}

/// Parses a plain text string and returns an iterator over a single `Value` node.
pub fn parse_text_input(input: &str) -> miette::Result<Vec<Value>> {
    Ok(input.lines().map(|line| line.to_string().into()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_basic() {
        let code = "add(\"world!\")";
        let input = mq_markdown::Markdown::from_str("Hello,").unwrap();
        let mut engine = Engine::default();

        assert_eq!(
            engine
                .eval(
                    code,
                    input
                        .nodes
                        .into_iter()
                        .map(Value::from)
                        .collect::<Vec<_>>()
                        .into_iter()
                )
                .unwrap(),
            vec![Value::Markdown(mq_markdown::Node::Text(
                mq_markdown::Text {
                    value: "Hello,world!".to_string(),
                    position: None
                }
            ))]
            .into()
        );
    }

    #[test]
    fn test_parse_error_syntax() {
        let code = "add(1,";
        let token_arena = Rc::new(RefCell::new(Arena::new(10)));
        let result = parse(code, token_arena);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_error_lexer() {
        let code = "add(1, `unclosed string)";
        let token_arena = Rc::new(RefCell::new(Arena::new(10)));
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
        let values: Vec<Value> = result.unwrap();
        assert!(!values.is_empty());
    }

    #[test]
    fn test_parse_mdx_input() {
        let input = "# Heading\n\nSome text.";
        let result = parse_mdx_input(input);
        assert!(result.is_ok());
        let values: Vec<Value> = result.unwrap();
        assert!(!values.is_empty());
    }

    #[test]
    fn test_parse_text_input() {
        let input = "line1\nline2\nline3";
        let result = parse_text_input(input);
        assert!(result.is_ok());
        let values: Vec<Value> = result.unwrap();
        assert_eq!(values.len(), 3);
    }
}
