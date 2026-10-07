//! The doc table and `mq-lang`'s native registry describe the same builtins.
#![cfg(feature = "catalog")]

use std::collections::BTreeSet;

use mq_help::{BUILTIN_DOC, BuiltinDoc, DocKind};
use mq_lang::{BUILTIN_FUNCTION_NAMES, INTERNAL_FUNCTION_NAMES, SELECTOR_NAMES};

// Callable names the compiler handles itself, so they are absent from the native registry.
const COMPILER_BUILTINS: [&str; 3] = ["breakpoint", "next", "send"];

// Names registered under `kind` that the table lacks or documents as another kind.
fn missing(names: &[&'static str], kind: DocKind) -> Vec<&'static str> {
    names
        .iter()
        .copied()
        .filter(|name| BUILTIN_DOC.get(name).is_none_or(|doc| doc.kind != kind))
        .collect()
}

// Docs of `kind` with no registry entry. Docs for features that are off in this build have
// none, but name their feature.
fn unregistered(kind: DocKind, names: impl IntoIterator<Item = &'static str>) -> Vec<&'static str> {
    let registered: BTreeSet<&str> = names.into_iter().collect();
    BUILTIN_DOC
        .of_kind(kind)
        .filter(|doc: &&BuiltinDoc| !registered.contains(doc.name) && doc.capability.is_none())
        .map(|doc| doc.name)
        .collect()
}

#[test]
fn every_native_builtin_is_documented() {
    let missing = missing(BUILTIN_FUNCTION_NAMES, DocKind::Function);
    assert!(
        missing.is_empty(),
        "registered but not documented as a function: {missing:?}"
    );
}

#[test]
fn every_internal_builtin_is_documented() {
    let missing = missing(INTERNAL_FUNCTION_NAMES, DocKind::Internal);
    assert!(
        missing.is_empty(),
        "internal but not documented as internal: {missing:?}"
    );
}

#[test]
fn documented_functions_are_registered_or_feature_gated() {
    let names = BUILTIN_FUNCTION_NAMES.iter().copied().chain(COMPILER_BUILTINS);
    let unexpected = unregistered(DocKind::Function, names);
    assert!(
        unexpected.is_empty(),
        "documented but neither registered nor feature-gated: {unexpected:?}"
    );
}

#[test]
fn documented_internal_functions_are_registered_or_feature_gated() {
    let unexpected = unregistered(DocKind::Internal, INTERNAL_FUNCTION_NAMES.iter().copied());
    assert!(
        unexpected.is_empty(),
        "documented as internal but not registered as internal: {unexpected:?}"
    );
}

#[test]
fn documented_selectors_are_accepted() {
    let unknown: Vec<_> = BUILTIN_DOC
        .selectors()
        .map(|doc| doc.name)
        .filter(|name| !SELECTOR_NAMES.contains(name))
        .collect();
    assert!(unknown.is_empty(), "documented but not accepted: {unknown:?}");
}
