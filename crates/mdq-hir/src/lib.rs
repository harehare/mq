//! This module provides the core functionality for the `mdq-hir` crate, which includes
//! handling high-level intermediate representation (HIR) for the project. The module is organized into several submodules, each responsible for a specific aspect of the HIR processing and management.
//!
//! - `builtin`: Contains built-in definitions and utilities.
//! - `error`: Defines error types and handling mechanisms.
//! - `find`: Provides functionality to find elements within the HIR.
//! - `hir`: Defines the structure and manipulation of the high-level intermediate representation.
//! - `reference`: Manages references within the HIR.
//! - `resolve`: Handles name resolution within the HIR.
//! - `scope`: Manages scopes and their relationships within the HIR.
//! - `source`: Deals with source code representation and management.
//! - `symbol`: Defines symbols and their properties within the HIR.
//!
//! ## Example
//!
//! ```rust
//! use std::str::FromStr;
//!
//! use itertools::Itertools;
//! use mdq_hir::{Hir, Symbol, SymbolId};
//! use url::Url;
//!
//! // Create a new HIR instance
//! let mut hir = Hir::new();
//!
//! // Add some code to the HIR
//! let code = r#"
//!   def main():
//!     let x = 42; | x;
//!   "#;
//! hir.add_code(Url::from_str("file:///main.rs").unwrap(), code);
//!
//! // Retrieve symbols from the HIR
//! let symbols: Vec<(SymbolId, &Symbol)> = hir.symbols().collect_vec();
//!
//! // Print the symbols
//! for (symbol_id, symbol) in symbols {
//!   println!("{:?}, {:?}, {:?}", symbol_id, symbol.name, symbol.kind);
//! }
//! ```
mod builtin;
mod error;
mod find;
mod hir;
mod reference;
mod resolve;
mod scope;
mod source;
mod symbol;

pub use hir::Hir;
pub use scope::Scope;
pub use scope::ScopeId;
pub use scope::ScopeKind;
pub use source::Source;
pub use source::SourceId;
pub use source::SourceInfo;
pub use symbol::Symbol;
pub use symbol::SymbolId;
pub use symbol::SymbolKind;
