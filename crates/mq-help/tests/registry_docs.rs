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
fn selector_docs_cover_exactly_the_accepted_names() {
    let documented: BTreeSet<&str> = BUILTIN_DOC.selectors().flat_map(|doc| doc.names()).collect();
    let accepted: BTreeSet<&str> = SELECTOR_NAMES.iter().copied().collect();
    let undocumented: Vec<_> = accepted.difference(&documented).collect();
    let unknown: Vec<_> = documented.difference(&accepted).collect();
    assert!(undocumented.is_empty(), "accepted but undocumented: {undocumented:?}");
    assert!(unknown.is_empty(), "documented but not accepted: {unknown:?}");
}

#[test]
fn catalog_lists_feature_gated_functions_only_when_enabled() {
    let listed: BTreeSet<String> = mq_help::top_level_entries()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    for doc in BUILTIN_DOC.functions().filter(|doc| doc.capability.is_some()) {
        assert_eq!(
            listed.contains(doc.name),
            mq_lang::is_builtin_function(doc.name),
            "{} is listed iff its feature is on",
            doc.name
        );
    }
}

#[test]
fn lookup_finds_a_selector_by_alias_and_by_attribute_name() {
    let by_alias = mq_help::lookup(".p");
    assert_eq!(by_alias.len(), 1);
    assert_eq!(by_alias[0].name, ".text");
    assert!(by_alias[0].aliases.contains(&".p".to_string()));

    assert_eq!(mq_help::lookup(".url").len(), 1);
}
