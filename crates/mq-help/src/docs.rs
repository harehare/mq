//! Static documentation tables for native builtins and selectors.

mod function;
mod internal;
mod selector;

pub use function::BUILTIN_FUNCTION_DOC;
pub use internal::INTERNAL_FUNCTION_DOC;
pub use selector::BUILTIN_SELECTOR_DOC;

/// A single runnable, verified example shown by `mq help`.
///
/// `expected` is checked against the real evaluation result of `code` by a test
/// (see `doc_examples` tests), so examples cannot silently rot.
#[derive(Clone, Debug)]
pub struct BuiltinExample {
    pub code: &'static str,
    pub expected: &'static str,
}

/// Documentation for a native builtin function or selector.
#[derive(Clone, Debug)]
pub struct BuiltinDoc {
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [&'static str],
    /// Parallel to `params`; a type name (e.g. "string", "number") or "dynamic" per param.
    pub param_types: &'static [&'static str],
    /// Type name of the returned value (e.g. "array", "bool", "dynamic").
    pub returns: &'static str,
    pub examples: &'static [BuiltinExample],
    /// Cargo feature flag required to use this item, if any (e.g. "file-io").
    pub capability: Option<&'static str>,
}

/// Docs sorted by name, so lookups are a binary search.
#[derive(Clone, Copy, Debug, Default)]
pub struct DocTable(&'static [BuiltinDoc]);

impl DocTable {
    /// Wraps `docs`, which must be sorted by `name`.
    pub const fn new(docs: &'static [BuiltinDoc]) -> Self {
        Self(docs)
    }

    /// Returns the doc named `name`.
    pub fn get(&self, name: &str) -> Option<&'static BuiltinDoc> {
        self.0
            .binary_search_by(|doc| doc.name.cmp(name))
            .ok()
            .map(|index| &self.0[index])
    }

    /// Returns true if a doc named `name` exists.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Iterates the docs in name order.
    pub fn iter(&self) -> impl Iterator<Item = &'static BuiltinDoc> + use<> {
        self.0.iter()
    }

    /// Returns the number of docs.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if there are no docs.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tables_are_sorted_by_name() {
        for (label, table) in [
            ("BUILTIN_FUNCTION_DOC", BUILTIN_FUNCTION_DOC),
            ("BUILTIN_SELECTOR_DOC", BUILTIN_SELECTOR_DOC),
            ("INTERNAL_FUNCTION_DOC", INTERNAL_FUNCTION_DOC),
        ] {
            let names: Vec<_> = table.iter().map(|doc| doc.name).collect();
            assert!(
                names.windows(2).all(|pair| pair[0] < pair[1]),
                "{label} must be sorted by name without duplicates"
            );
        }
    }
}
