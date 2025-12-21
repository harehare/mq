//! Closure-based compiler for mq-lang.
//!
//! This module provides a compiler that transforms AST nodes into dynamically-dispatched
//! closures for faster execution. The compiler is optional and can be enabled via
//! `Engine::set_use_compiler(true)`.
//!
//! ## Design
//!
//! The compiler follows the approach described in Cloudflare's blog post on building
//! fast interpreters in Rust. Each AST expression is compiled to a closure that:
//! - Takes a `RuntimeValue` as input (pipeline style)
//! - Takes a mutable call stack for recursion tracking
//! - Takes an environment for variable resolution
//! - Returns a `Result<RuntimeValue, RuntimeError>`
//!
//! ## Performance
//!
//! Expected 10-15% runtime speedup over tree-walking interpreter for typical workloads,
//! with gains from:
//! - Reduced AST traversal overhead
//! - Compile-time constant folding
//! - Better cache locality
//!
//! ## Example
//!
//! ```rust
//! use mq_lang::DefaultEngine;
//!
//! let mut engine = DefaultEngine::default();
//! engine.load_builtin_module();
//! engine.set_use_compiler(true);
//!
//! let input = mq_lang::parse_text_input("hello").unwrap();
//! let result = engine.eval("add(\" world\")", input.into_iter());
//! assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
//! ```

mod call_stack;
mod compile;
pub(crate) mod compiled;
mod constant_fold;
#[cfg(test)]
mod test_compiler;

pub use compile::Compiler;
