//! This module provides the core functionality for the `mq-hir` crate, which includes the High-level Intermediate Representation (HIR) for the [mq](https://github.com/harehare/mq).
//!
//! ## Example
//!
//! ```rust
//! use std::str::FromStr;
//!
//! use itertools::Itertools;
//! use mq_hir::{Hir, Symbol, SymbolId};
//! use url::Url;
//!
//! // Create a new HIR instance
//! let mut hir = Hir::default();
//!
//! // Add some code to the HIR
//! let code = r#"
//!   def main():
//!     let x = 42; | x;
//!   "#;
//! hir.add_code(Some(Url::from_str("file:///main.rs").unwrap()), code);
//!
//! // Retrieve symbols from the HIR
//! let symbols: Vec<(SymbolId, &Symbol)> = hir.symbols().collect::<Vec<_>>();
//!
//! // Print the symbols
//! for (symbol_id, symbol) in symbols {
//!   println!("{:?}, {:?}, {:?}", symbol_id, symbol.value, symbol.kind);
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

pub use error::HirError;
pub use error::HirWarning;
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
