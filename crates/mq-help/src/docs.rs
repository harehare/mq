//! Static documentation table for native builtins and selectors.

mod table;

pub use table::BUILTIN_DOC;

/// A single runnable, verified example shown by `mq help`.
///
/// `expected` is checked against the real evaluation result of `code` by a test
/// (see `doc_examples` tests), so examples cannot silently rot.
#[derive(Clone, Debug)]
pub struct BuiltinExample {
    pub code: &'static str,
    pub expected: &'static str,
}

/// What a [`BuiltinDoc`] describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    /// A native builtin function.
    Function,
    /// A native selector (e.g. `.h`).
    Selector,
    /// An implementation detail of `builtin.mq`, kept out of user-facing listings.
    Internal,
}

/// Documentation for a native builtin function or selector.
#[derive(Clone, Debug)]
pub struct BuiltinDoc {
    pub name: &'static str,
    pub kind: DocKind,
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

/// Docs sorted by name, so lookups are a binary search. Selector names start with `.`, so
/// they never collide with function names.
#[derive(Clone, Copy, Debug, Default)]
pub struct DocTable(&'static [BuiltinDoc]);

impl DocTable {
    /// Wraps `docs`, which must be sorted by `name`.
    pub const fn new(docs: &'static [BuiltinDoc]) -> Self {
        Self(docs)
    }

    /// Returns the doc named `name`, whatever its kind.
    pub fn get(&self, name: &str) -> Option<&'static BuiltinDoc> {
        self.0
            .binary_search_by(|doc| doc.name.cmp(name))
            .ok()
            .map(|index| &self.0[index])
    }

    /// Returns the doc named `name` if it is a function.
    pub fn function(&self, name: &str) -> Option<&'static BuiltinDoc> {
        self.get(name).filter(|doc| doc.kind == DocKind::Function)
    }

    /// Returns the doc named `name` if it is a selector.
    pub fn selector(&self, name: &str) -> Option<&'static BuiltinDoc> {
        self.get(name).filter(|doc| doc.kind == DocKind::Selector)
    }

    /// Iterates every doc in name order.
    pub fn iter(&self) -> impl Iterator<Item = &'static BuiltinDoc> + use<> {
        self.0.iter()
    }

    /// Iterates the docs of the given kind in name order.
    pub fn of_kind(&self, kind: DocKind) -> impl Iterator<Item = &'static BuiltinDoc> + use<> {
        self.0.iter().filter(move |doc| doc.kind == kind)
    }

    /// Iterates the function docs, without internal helpers.
    pub fn functions(&self) -> impl Iterator<Item = &'static BuiltinDoc> + use<> {
        self.of_kind(DocKind::Function)
    }

    /// Iterates the selector docs.
    pub fn selectors(&self) -> impl Iterator<Item = &'static BuiltinDoc> + use<> {
        self.of_kind(DocKind::Selector)
    }

    /// Iterates the internal helper docs.
    pub fn internal_functions(&self) -> impl Iterator<Item = &'static BuiltinDoc> + use<> {
        self.of_kind(DocKind::Internal)
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
    fn test_table_is_sorted_by_name() {
        let names: Vec<_> = BUILTIN_DOC.iter().map(|doc| doc.name).collect();
        assert!(
            names.windows(2).all(|pair| pair[0] < pair[1]),
            "BUILTIN_DOC must be sorted by name without duplicates"
        );
    }

    #[test]
    fn test_selector_kind_matches_dot_prefix() {
        for doc in BUILTIN_DOC.iter() {
            assert_eq!(doc.kind == DocKind::Selector, doc.name.starts_with('.'), "{}", doc.name);
        }
    }
}
