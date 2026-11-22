//! Lexer, AST parser and CST parser for the mq query language.
//!
//! ```rust
//! use mq_syntax::{Arena, Lexer, LexerOptions, TOP_LEVEL_MODULE_ID, ast::parser::Parser};
//!
//! let tokens = Lexer::new(LexerOptions::default())
//!     .tokenize("1 + 2", TOP_LEVEL_MODULE_ID)
//!     .unwrap();
//! let mut arena = Arena::new(16);
//! let program = Parser::new(tokens.iter(), &mut arena, TOP_LEVEL_MODULE_ID)
//!     .parse()
//!     .unwrap();
//! assert_eq!(program.len(), 1);
//! ```
//!
//! ## Features
//!
//! - `cst`: the error-recovering CST parser ([`parse_recovery`], [`cst`]).
//! - `ast-json`: JSON (de)serialization of the AST and tokens.
//! - `sync`: `Arc`/`RwLock` instead of `Rc`/`RefCell` for [`Shared`] and [`SharedCell`].
//! - `debugger`: items used by the debugger.
/// Id-indexed token and module-name storage.
pub mod arena;
/// The AST and its parser.
pub mod ast;
/// The lossless CST and its error-recovering parser.
#[cfg(feature = "cst")]
pub mod cst;
/// Source-agnostic diagnostics.
pub mod diagnostic;
/// Syntax errors.
pub mod error;
/// Interned identifiers.
pub mod ident;
/// The lexer and tokens.
pub mod lexer;
/// Numbers.
pub mod number;
/// Source positions and ranges.
pub mod range;
/// Markdown selectors.
pub mod selector;
/// "Did you mean" suggestions.
pub mod suggest;

#[cfg(not(feature = "sync"))]
use std::cell::RefCell;
#[cfg(not(feature = "sync"))]
use std::rc::Rc;
#[cfg(feature = "sync")]
use std::sync::Arc;
#[cfg(feature = "sync")]
use std::sync::RwLock;

use smol_str::SmolStr;

pub use arena::{Arena, ArenaId};
pub use ast::Program;
pub use ast::constants::identifiers::IMPLICIT_FN_ARG;
pub use diagnostic::Diagnostic;
pub use error::SyntaxError;
pub use ident::Ident;
pub use lexer::token::{StringSegment, Token, TokenKind};
pub use lexer::{BOOLEAN_LITERALS, KEYWORDS, Lexer, Options as LexerOptions};
pub use number::Number;
pub use range::{Position, Range};
pub use selector::{AttrKind, SELECTOR_NAMES, Selector};

#[cfg(feature = "cst")]
pub use cst::node::{
    BinaryOp as CstBinaryOp, Blank as CstBlank, Node as CstNode, NodeKind as CstNodeKind, Trivia as CstTrivia,
    TriviaList as CstTriviaList, UnaryOp as CstUnaryOp,
};
#[cfg(feature = "cst")]
pub use cst::parser::{ErrorReporter as CstErrorReporter, Parser as CstParser};

/// Reference-counted pointer: `Rc`, or `Arc` with the `sync` feature.
#[cfg(not(feature = "sync"))]
pub type Shared<T> = Rc<T>;
/// Reference-counted pointer: `Rc`, or `Arc` with the `sync` feature.
#[cfg(feature = "sync")]
pub type Shared<T> = Arc<T>;

/// Interior-mutability cell: `RefCell`, or `RwLock` with the `sync` feature.
#[cfg(not(feature = "sync"))]
pub type SharedCell<T> = RefCell<T>;
/// Interior-mutability cell: `RefCell`, or `RwLock` with the `sync` feature.
#[cfg(feature = "sync")]
pub type SharedCell<T> = RwLock<T>;

/// The arena the AST parser allocates tokens in.
pub type TokenArena = Shared<SharedCell<Arena<Token>>>;

/// The name of a module, as stored in a module-name arena.
pub type ModuleName = SmolStr;

/// Id of a module name in the module-name arena.
pub type ModuleId = ArenaId<ModuleName>;

/// The id of the top-level module, the query being run.
pub const TOP_LEVEL_MODULE_ID: ModuleId = ArenaId::new(0);

/// Parses `code` into CST nodes, collecting errors instead of stopping at the first one.
///
/// Broken input still yields a lossless tree: unparsable tokens are kept in `Error` nodes
/// and absent required tokens appear as zero-width `Missing` nodes.
///
/// ```rust
/// let (cst_nodes, errors) = mq_syntax::parse_recovery("1 + 2");
/// assert!(!errors.has_errors());
/// assert!(!cst_nodes.is_empty());
/// ```
#[cfg(feature = "cst")]
pub fn parse_recovery(code: &str) -> (Vec<Shared<CstNode>>, CstErrorReporter) {
    let tokens = Lexer::new(LexerOptions {
        ignore_errors: true,
        include_spaces: true,
    })
    .tokenize(code, TOP_LEVEL_MODULE_ID);

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

    CstParser::new(&tokens).parse()
}
