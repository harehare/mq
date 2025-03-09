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
//! let code = "add(1, 2)";
//! let token_arena = Rc::new(RefCell::new(Arena::new()));
//! let parser = mq_lang::parse(code, token_arena).unwrap();
//!
//! assert_eq!(ast.nodes.len(), 1);
//!
//! // Parse code into CST nodes
//! use mq_lang::{tokenize, LexerOptions, CstParser};
//! use std::sync::Arc;
//!
//! let code = "add(1, 2)";
//! let (cst_nodes, errors) = mq_lang::parse_recovery(code);
//!
//! assert!(errors.errors().is_empty());
//! assert!(!cst_nodes.is_empty());
//! ```
mod arena;
mod ast;
mod cst;
mod engine;
mod error;
mod eval;
mod lexer;
mod number;
mod optimizer;
mod range;
mod value;

use error::InnerError;
use lexer::Lexer;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub use arena::Arena;
pub use ast::IdentName as AstIdentName;
pub use ast::Params as AstParams;
pub use ast::Program;
pub use ast::node::Expr as AstExpr;
pub use ast::node::Ident as AstIdent;
pub use ast::node::Literal as AstLiteral;
pub use ast::node::Node as AstNode;
pub use ast::parser::Parser as AstParser;
pub use cst::node::Node as CstNode;
pub use cst::node::NodeKind as CstNodeKind;
pub use cst::node::Trivia as CstTrivia;
pub use cst::parser::ErrorReporter as CstErrorReporter;
pub use cst::parser::Parser as CstParser;
pub use engine::Engine;
pub use error::Error;
pub use eval::builtin::{
    BUILTIN_FUNCTION_DOC, BUILTIN_SELECTOR_DOC, BuiltinFunctionDoc, BuiltinSelectorDoc,
};
pub use eval::module::Module;
pub use eval::module::ModuleLoader;
pub use lexer::Options as LexerOptions;
pub use lexer::token::{Token, TokenKind};
pub use range::{Position, Range};
pub use value::{Value, Values};

pub type MqResult = Result<Values, Error>;

pub fn parse_recovery(code: &str) -> (Vec<Arc<CstNode>>, CstErrorReporter) {
    let tokens = tokenize(
        code,
        lexer::Options {
            ignore_errors: true,
            include_spaces: true,
        },
    )
    .unwrap();
    let (cst_nodes, errors) =
        CstParser::new(tokens.into_iter().map(Arc::new).collect::<Vec<_>>().iter()).parse();

    (cst_nodes, errors)
}

#[allow(clippy::result_large_err)]
pub fn parse(
    code: &str,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
) -> Result<Program, error::Error> {
    AstParser::new(
        tokenize(code, lexer::Options::default())?
            .into_iter()
            .map(Rc::new)
            .collect::<Vec<_>>()
            .iter(),
        token_arena,
        Module::TOP_LEVEL_MODULE_ID,
    )
    .parse()
    .map_err(|e| error::Error::from_error(code, InnerError::Parse(e), ModuleLoader::new(None)))
}

#[allow(clippy::result_large_err)]
pub fn tokenize(
    code: &str,
    options: lexer::Options,
) -> Result<Vec<lexer::token::Token>, error::Error> {
    Lexer::new(options)
        .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
        .map_err(|e| error::Error::from_error(code, InnerError::Lexer(e), ModuleLoader::new(None)))
}
