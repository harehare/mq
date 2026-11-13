//! Output format renderers for the `-F/--output-format` CLI option.
//!
//! Grouped under this module (rather than as top-level `csv`/`toml` modules) so the
//! module names don't collide with the like-named `csv`/`toml` crates in the extern prelude.

pub(crate) mod csv;
pub(crate) mod gron;
pub(crate) mod json;
pub(crate) mod shell;
pub(crate) mod table;
pub(crate) mod toml;
pub(crate) mod toon;
pub(crate) mod xml;
pub(crate) mod yaml;

/// Returns `true` for values a selector produced when nothing matched.
pub(crate) fn is_unmatched(value: &mq_lang::RuntimeValue) -> bool {
    match value {
        mq_lang::RuntimeValue::None => true,
        mq_lang::RuntimeValue::Markdown(node, _) => node.is_empty() || node.is_empty_fragment(),
        _ => false,
    }
}
