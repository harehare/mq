//! Command-line interface for the mq markdown processing tool.
//!
//! This crate provides the CLI implementation for mq, a jq-like tool for processing
//! markdown files. It handles command-line argument parsing, input/output processing,
//! and integration with the mq language engine.
//!
//! # Features
//!
//! - Process markdown, MDX, HTML, and plain text inputs
//! - Support for file and stdin input
//! - Multiple output formats
//! - Optional debugger integration (with `debugger` feature)
//! - Configuration file support
//! - Interactive REPL mode
//!
//! # Usage
//!
//! The CLI is typically used through the `mq` binary, but can be embedded in other applications:
//!
//! ```rust,no_run
//! use mq_cli::Cli;
//! use clap::Parser;
//!
//! let cli = Cli::parse();
//! cli.run().expect("CLI execution failed");
//! ```
//!
//! # Command-line Examples
//!
//! Process markdown from a file:
//! ```bash
//! mq '.h' input.md
//! ```
//!
//! Filter headings from stdin:
//! ```bash
//! echo "# Title\nContent" | mq '.h | select(level == 1)'
//! ```
//!
//! Use the REPL:
//! ```bash
//! mq --repl
//! ```

pub mod cli;

#[cfg(feature = "debugger")]
pub mod debugger;

pub use cli::Cli;
