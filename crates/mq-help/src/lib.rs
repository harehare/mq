//! Documentation catalog for `mq`.
//!
//! This crate owns the static doc tables for native builtins and selectors ([`docs`]), and
//! builds the single documentation catalog shared by the `mq help` CLI command and the
//! `mq-web-api` documentation endpoints, so the two can't drift apart. The catalog is behind
//! the `catalog` feature (on by default); without it only the doc tables are built. The
//! catalog combines two things:
//!
//! - [`reference`]: extracts `MqFnDoc`/`MqExample` from the CST of any mq source, by parsing
//!   the Markdown-ish doc-comment convention above each `def` (used for `builtin.mq`
//!   and every standard module).
//! - [`catalog`]: unifies that CST-extracted documentation with the native builtin
//!   and selector doc tables into a single [`HelpEntry`] shape, with lookup, "did you mean"
//!   suggestions, and human-readable rendering.

#[cfg(feature = "catalog")]
pub mod catalog;
pub mod docs;
#[cfg(feature = "catalog")]
pub mod reference;

#[cfg(feature = "catalog")]
pub use catalog::{
    HelpEntry, HelpExample, HelpModule, HelpParam, all_entries, all_modules, all_names, lookup, lookup_module,
    render_human, render_markdown, render_module_human, render_module_markdown, suggest, top_level_entries,
};
pub use docs::{BUILTIN_DOC, BuiltinDoc, BuiltinExample, DocKind, DocTable};
#[cfg(feature = "catalog")]
pub use reference::{ModuleDoc, MqExample, MqFnDoc, extract_functions_from_cst, extract_module, extract_module_doc};
