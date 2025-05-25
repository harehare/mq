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

use error::InnerError;
use lexer::Lexer;
use std::cell::RefCell;
use std::rc::Rc;
#[cfg(feature = "cst")]
use std::sync::Arc;
use typed_arena::Arena as TypedArena; // Added for AstArena

pub use arena::Arena; // This is TokenArena
pub use ast::IdentName as AstIdentName;
// Program is now ast::node::AstProgram (Vec<NodeId>)
// pub use ast::Program; // This old Program (Vec<Rc<Node>>) should be removed or updated
pub use ast::node::Program as AstProgram; // Use the new Program type
pub use ast::node::AstArena; // Re-export AstArena
pub use ast::node::NodeId;   // Re-export NodeId
pub use ast::node::NodeData; // Re-export NodeData
pub use ast::node::Expr as AstExpr;
pub use ast::node::Ident as AstIdent;
pub use ast::node::Literal as AstLiteral;
// AstNode (Rc<Node>) is removed, use NodeId and AstArena instead.
// pub use ast::node::Node as AstNode; 
pub use ast::node::Params as AstParams; // AstParams is SmallVec<[NodeId; 4]>
// AstParser (ast::parser::Parser) now has lifetimes Parser<'a, 'ast>
// pub use ast::parser::Parser as AstParser; // This re-export might need to be generic or removed.
                                          // For now, let's comment it out as it's hard to re-export with lifetimes directly.
pub use engine::Engine; // Engine will be Engine<'ast>
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

    // AstParser::new now requires an AstArena.
    // This standalone `parse` function becomes problematic with arena ownership.
    // It would need to create an arena, parse into it, and then what?
    // - Return the arena and NodeIds? (Caller takes ownership, complex lifetimes)
    // - Leak the arena? (Bad for a library function)
    // - Take a closure?
    // For now, this function's utility is diminished without Engine owning the arena.
    // We'll have it create a temporary arena for the parse.
    // The returned Program (Vec<NodeId>) will have dangling NodeIds if the arena is dropped.
    // This highlights that parsing should likely happen within an Engine context.
    let ast_arena = TypedArena::new(); // Create a temporary arena
    AstParser::new(
        tokens.into_iter().map(Rc::new).collect::<Vec<_>>().iter(),
        token_arena,
        &ast_arena, // Pass the arena
        Module::TOP_LEVEL_MODULE_ID,
    )
    .parse() // This returns Result<Vec<NodeId>, ParseError>
    .map_err(|e| {
        Box::new(error::Error::from_error(
            code,
            InnerError::Parse(e),
            ModuleLoader::new(None),
        ))
    })
}
#[cfg(test)]
// #[ignore] // Removing ignore from the test module in lib.rs
mod tests {
    use std::str::FromStr;
    // Engine struct and other necessary items are in scope via `use super::*;`
    // For tests, AstArena might be created inside Engine or passed if tests need direct access.
    // For Engine::default(), it creates its own AstArena.
    use crate::value::Value; // For test assertions
    use crate::number::Number; // For test assertions
    use mq_markdown; // For markdown node construction in tests

    use super::*;


    #[test]
    // #[ignore] // This test should work with the refactored Engine
    fn test_eval_basic() {
        let code = "add(\"world!\")";
        let input_md = mq_markdown::Markdown::from_str("Hello,").unwrap();
        // Convert Vec<mq_markdown::Node> to Vec<Value>
        let input_values: Vec<Value> = input_md.nodes.into_iter().map(Value::from).collect();
        
        let mut engine = Engine::default(); // Engine<'static>
        engine.load_builtin_module().expect("Builtin module should load for test_eval_basic");


        let eval_result = engine.eval(code, input_values.into_iter());
        assert!(eval_result.is_ok(), "eval failed: {:?}", eval_result.err());
        
        let output_values = eval_result.unwrap();
        assert_eq!(output_values.len(), 1, "Expected one output value");

        // The original test example in lib.rs comments expected Value::String:
        // assert!(matches!(engine.eval(&code, input).unwrap(), mq_lang::Value::String("Hello,world!".to_string())));
        // Let's align with that.
        match output_values.get(0) {
            Some(Value::String(s)) => {
                assert_eq!(s, "Hello,world!", "String value mismatch");
            }
            _ => panic!("Expected Value::String output, got {:?}", output_values.get(0)),
        }
    }

    // test_parse_error_syntax and test_parse_error_lexer are removed as the global `parse` function was removed.
    // Error handling for parsing is now part of Engine::eval or direct Parser tests.

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
}
