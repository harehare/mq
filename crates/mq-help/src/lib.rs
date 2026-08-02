//! Documentation catalog for `mq`.
//!
//! This crate builds the single documentation catalog shared by the `mq help` CLI command
//! and the `mq-web-api` documentation endpoints, so the two can't drift apart. It combines
//! two things:
//!
//! - [`reference`]: extracts `MqFnDoc`/`MqExample` from the CST of any mq source, by parsing
//!   the Markdown-ish doc-comment convention above each `def`/`macro` (used for `builtin.mq`
//!   and every standard module).
//! - [`catalog`]: unifies that CST-extracted documentation with `mq-lang`'s native builtin
//!   and selector doc tables into a single [`HelpEntry`] shape, with lookup, "did you mean"
//!   suggestions, and human-readable rendering.

pub mod catalog;
pub mod reference;

pub use catalog::{HelpEntry, HelpExample, HelpParam, all_entries, all_names, lookup, render_human, suggest};
pub use reference::{MqExample, MqFnDoc, extract_functions_from_cst};
