//! Output format renderers for the `-F/--output-format` CLI option.
//!
//! Grouped under this module (rather than as top-level `csv`/`toml` modules) so the
//! module names don't collide with the like-named `csv`/`toml` crates in the extern prelude.

pub(crate) mod csv;
pub(crate) mod json;
pub(crate) mod table;
pub(crate) mod toml;
pub(crate) mod xml;
pub(crate) mod yaml;
