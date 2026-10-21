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
//! - Optional Tarn VM operand-stack tracing (with `debug-trace` feature)
//! - Optional Tarn VM instruction-count profiling (with `vm-profile` feature)
//! - Interactive REPL mode
//!
//! # Usage
//!
//! The CLI is typically used through the `mq` binary, but can be embedded in other applications.
//! External subcommands (`mq-NAME`) are dispatched by [`Cli::run_external_subcommand`] on the raw
//! arguments before parsing; skip that call if the embedding application does not need them.
//!
//! ```rust,no_run
//! use mq_run::Cli;
//! use clap::Parser;
//!
//! fn main() -> std::process::ExitCode {
//!     let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
//!     if let Some(code) = Cli::run_external_subcommand(&args) {
//!         return code;
//!     }
//!
//!     let cli = Cli::parse_from(args);
//!     cli.run().expect("CLI execution failed");
//!     std::process::ExitCode::SUCCESS
//! }
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
//! mq repl
//! ```

pub(crate) mod atomic_output;
/// Command-line interface.
pub mod cli;
pub(crate) mod grep;
pub(crate) mod output;

#[cfg(feature = "debugger")]
/// Interactive debugger.
pub mod debugger;

pub use cli::Cli;
