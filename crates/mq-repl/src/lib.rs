//! This module contains the main library code for the `mq-repl` crate.
//!
//! ## Example
//!
//! ```rust
//! use mq_repl::Repl;
//!
//! let repl = mq_repl::Repl::new(vec![mq_lang::Value::String("".to_string())]);
//! repl.run().unwrap();
//! ```
mod command_context;
mod repl;

pub use repl::Repl;
