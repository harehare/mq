//! WebAssembly bindings for the mq markdown processing library.
//!
//! This crate provides WebAssembly bindings for mq, enabling browser-based
//! and WASM-compatible environments to process markdown, MDX, and HTML
//! using the mq query language.
//!
/// JavaScript-facing API.
pub mod script;

/// HTML to Markdown conversion.
#[cfg(feature = "html")]
pub mod html;
/// Diagnostics, hover, inlay hints, formatter and AST dump.
#[cfg(feature = "lang-service")]
pub mod lang_service;
