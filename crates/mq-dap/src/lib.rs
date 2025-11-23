//! Debug Adapter Protocol (DAP) implementation for mq.
//!
//! This crate provides a DAP server for debugging mq scripts, enabling integration
//! with IDEs and editors that support the Debug Adapter Protocol.
//!
//! # Features
//!
//! - Full DAP protocol support for mq debugging
//! - Breakpoint management
//! - Step-through execution (step in, step out, step over)
//! - Variable inspection
//! - Stack trace visualization
//! - Expression evaluation in debug context
//!
//! # Usage
//!
//! The DAP server can be started programmatically or as part of an editor integration:
//!
//! ```rust,ignore
//! use mq_dap::start;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     start().await?;
//!     Ok(())
//! }
//! ```
//!
//! # Protocol Support
//!
//! This implementation supports the Debug Adapter Protocol as specified by Microsoft.
//! The server communicates over stdin/stdout using JSON-RPC messages.
//!
//! # Integration
//!
//! This DAP implementation is designed to work with:
//! - Visual Studio Code
//! - Neovim with DAP support
//! - Other editors supporting the Debug Adapter Protocol
//!
//! # Architecture
//!
//! The crate is organized into:
//! - `adapter`: DAP adapter implementation
//! - `protocol`: DAP message types and protocol handling
//! - `executor`: Debug execution engine
//! - `handler`: Request and event handlers
//! - `server`: DAP server implementation

pub mod adapter;
pub mod error;
pub mod executor;
pub mod handler;
pub mod log;
pub mod protocol;
pub mod server;

pub use server::start;
