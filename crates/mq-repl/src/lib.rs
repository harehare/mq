//! This crate provides a REPL (Read-Eval-Print Loop) environment for the [mq](https://github.com/harehare/mq), allowing for interactive execution of mq code.
//!
//! The REPL supports:
//! - Interactive command evaluation
//! - History navigation
//! - Code execution in a persistent environment
//!
//! ## Example
//!
//! ```rust
//! use mq_repl::Repl;
//!
//! let repl = mq_repl::Repl::new(vec![mq_lang::Value::String("".into())]);
//! repl.run().unwrap();
//! ```
mod command_context;
mod repl;

pub use repl::Repl;
