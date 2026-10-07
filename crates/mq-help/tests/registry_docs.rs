//! The doc tables and `mq-lang`'s native registry describe the same builtins.
#![cfg(feature = "catalog")]

use std::collections::BTreeSet;

use mq_help::{BUILTIN_FUNCTION_DOC, BUILTIN_SELECTOR_DOC};
use mq_lang::{BUILTIN_FUNCTION_NAMES, SELECTOR_NAMES};

// Callable names the compiler handles itself, so they are absent from the native registry.
const COMPILER_BUILTINS: [&str; 3] = ["breakpoint", "next", "send"];

#[test]
fn every_native_builtin_is_documented() {
    let missing: Vec<_> = BUILTIN_FUNCTION_NAMES
        .iter()
        .filter(|name| !BUILTIN_FUNCTION_DOC.contains(name))
        .collect();
    assert!(missing.is_empty(), "registered but undocumented: {missing:?}");
}

#[test]
fn documented_functions_are_registered_or_feature_gated() {
    let registered: BTreeSet<&str> = BUILTIN_FUNCTION_NAMES
        .iter()
        .copied()
        .chain(COMPILER_BUILTINS)
        .collect();
    let unexpected: Vec<_> = BUILTIN_FUNCTION_DOC
        .iter()
        .filter(|doc| !registered.contains(doc.name) && doc.capability.is_none() && !doc.name.starts_with('_'))
        .map(|doc| doc.name)
        .collect();
    assert!(
        unexpected.is_empty(),
        "documented but neither registered nor feature-gated: {unexpected:?}"
    );
}

#[test]
fn documented_selectors_are_accepted() {
    let unknown: Vec<_> = BUILTIN_SELECTOR_DOC
        .iter()
        .map(|doc| doc.name)
        .filter(|name| !SELECTOR_NAMES.contains(name))
        .collect();
    assert!(unknown.is_empty(), "documented but not accepted: {unknown:?}");
}
