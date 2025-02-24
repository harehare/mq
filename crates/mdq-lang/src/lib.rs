//! `mdq-lang` is provides a parser and evaluator for a mdq language.
//!
//! ## Modules
//!
//! - `ast`: Abstract Syntax Tree (AST) structures and parser.
//! - `cst`: Concrete Syntax Tree (CST) structures and parser.
//! - `engine`: Execution engine for evaluating mdq code.
//! - `error`: Error handling utilities.
//! - `eval`: Evaluation logic and built-in functions.
//! - `lexer`: Lexical analysis and tokenization.
//! - `optimizer`: Code optimization utilities.
//! - `value`: Value types used in the language.
//!
//! ## Examples
//!
//! ```rs
//! use mdq_lang::Engine;
//!
//! let code = "add(\"world!\")";
//! let input = vec![mdq_lang::Value::Markdown(
//!   mdq_md::Markdown::from_str("Hello,").unwrap()
//! )].into_iter();
//! let mut engine = mdq_lang::Engine::default();
//!
//! assert!(matches!(engine.eval(&code, input).unwrap(), mdq_lang::Value::String("Hello,world!".to_string())));
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
use itertools::Itertools;
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

pub type MdqResult = Result<Values, Error>;

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
        CstParser::new(tokens.into_iter().map(Arc::new).collect_vec().iter()).parse();

    (cst_nodes, errors)
}

pub fn parse(
    code: &str,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
) -> Result<Program, error::Error> {
    AstParser::new(
        tokenize(code, lexer::Options::default())?
            .into_iter()
            .map(Rc::new)
            .collect_vec()
            .iter(),
        token_arena,
        Module::TOP_LEVEL_MODULE_ID,
    )
    .parse()
    .map_err(|e| error::Error::from_error(code, InnerError::Parse(e), ModuleLoader::new(None)))
}

pub fn tokenize(
    code: &str,
    options: lexer::Options,
) -> Result<Vec<lexer::token::Token>, error::Error> {
    Lexer::new(options)
        .tokenize(code, Module::TOP_LEVEL_MODULE_ID)
        .map_err(|e| error::Error::from_error(code, InnerError::Lexer(e), ModuleLoader::new(None)))
}
