//! The doc tables and `mq-lang`'s native registry describe the same builtins.
#![cfg(feature = "catalog")]

use std::collections::BTreeSet;

use mq_help::{BUILTIN_FUNCTION_DOC, BUILTIN_SELECTOR_DOC, DocTable, INTERNAL_FUNCTION_DOC};
use mq_lang::{BUILTIN_FUNCTION_NAMES, INTERNAL_FUNCTION_NAMES, SELECTOR_NAMES};

// Callable names the compiler handles itself, so they are absent from the native registry.
const COMPILER_BUILTINS: [&str; 3] = ["breakpoint", "next", "send"];

fn undocumented(names: &[&'static str], docs: DocTable) -> Vec<&'static str> {
    names.iter().copied().filter(|name| !docs.contains(name)).collect()
}

// Docs for features that are off in this build have no registry entry, but name their feature.
fn unregistered(names: impl IntoIterator<Item = &'static str>, docs: DocTable) -> Vec<&'static str> {
    let registered: BTreeSet<&str> = names.into_iter().collect();
    docs.iter()
        .filter(|doc| !registered.contains(doc.name) && doc.capability.is_none())
        .map(|doc| doc.name)
        .collect()
}

#[test]
fn every_native_builtin_is_documented() {
    let missing = undocumented(BUILTIN_FUNCTION_NAMES, BUILTIN_FUNCTION_DOC);
    assert!(missing.is_empty(), "registered but undocumented: {missing:?}");
}

#[test]
fn every_internal_builtin_is_documented() {
    let missing = undocumented(INTERNAL_FUNCTION_NAMES, INTERNAL_FUNCTION_DOC);
    assert!(missing.is_empty(), "internal but undocumented: {missing:?}");
}

#[test]
fn documented_functions_are_registered_or_feature_gated() {
    let names = BUILTIN_FUNCTION_NAMES.iter().copied().chain(COMPILER_BUILTINS);
    let unexpected = unregistered(names, BUILTIN_FUNCTION_DOC);
    assert!(
        unexpected.is_empty(),
        "documented but neither registered nor feature-gated: {unexpected:?}"
    );
}

#[test]
fn documented_internal_functions_are_registered_or_feature_gated() {
    let unexpected = unregistered(INTERNAL_FUNCTION_NAMES.iter().copied(), INTERNAL_FUNCTION_DOC);
    assert!(
        unexpected.is_empty(),
        "documented as internal but not registered as internal: {unexpected:?}"
    );
}

#[test]
fn internal_functions_are_not_in_the_public_table() {
    let leaked: Vec<_> = INTERNAL_FUNCTION_NAMES
        .iter()
        .filter(|name| BUILTIN_FUNCTION_DOC.contains(name))
        .collect();
    assert!(leaked.is_empty(), "internal but documented as public: {leaked:?}");
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
