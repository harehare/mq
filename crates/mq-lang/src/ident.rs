use std::sync::{LazyLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use string_interner::{DefaultBackend, DefaultSymbol, StringInterner};

static STRING_INTERNER: LazyLock<RwLock<StringInterner<DefaultBackend>>> =
    LazyLock::new(|| RwLock::new(StringInterner::default()));

// The interner is append-only, so a panic in an unrelated writer must not take down every later lookup.
fn interner_read() -> RwLockReadGuard<'static, StringInterner<DefaultBackend>> {
    STRING_INTERNER.read().unwrap_or_else(PoisonError::into_inner)
}

fn interner_write() -> RwLockWriteGuard<'static, StringInterner<DefaultBackend>> {
    STRING_INTERNER.write().unwrap_or_else(PoisonError::into_inner)
}

/// An interned string identifier for efficient storage and comparison.
///
/// Identifiers are stored in a global string interner, allowing fast equality
/// checks and reduced memory usage for frequently used strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ident(DefaultSymbol);

impl Ident {
    /// Creates a new interned identifier from a string slice.
    ///
    /// If the string already exists in the interner, returns the existing identifier.
    pub fn new(s: &str) -> Self {
        // Read-lock fast path for the common repeat-lookup case, avoiding the write lock
        // `get_or_intern` always takes.
        if let Some(sym) = interner_read().get(s) {
            return Self(sym);
        }
        Self(interner_write().get_or_intern(s))
    }

    /// Returns the identifier for `s` only if it is already interned.
    pub(crate) fn lookup(s: &str) -> Option<Self> {
        interner_read().get(s).map(Self)
    }

    /// Resolves the identifier and passes it to a callback function.
    ///
    /// Use this instead of `to_string()` when the string does not need to be owned.
    pub fn resolve_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&str) -> R,
    {
        let interner = interner_read();
        let resolved = interner
            .resolve(self.0)
            .expect("identifier symbols come from the global interner");
        f(resolved)
    }
}

impl Default for Ident {
    fn default() -> Self {
        Ident::new("")
    }
}

impl From<&str> for Ident {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Ident {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.resolve_with(|s| write!(f, "{}", s))
    }
}

#[cfg(feature = "ast-json")]
impl serde::Serialize for Ident {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Own the string so the interner lock is released before running external serializer code.
        self.to_string().serialize(serializer)
    }
}

#[cfg(feature = "ast-json")]
impl<'de> serde::Deserialize<'de> for Ident {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Ident::new(&s))
    }
}

/// Returns all interned strings currently in the global string interner.
pub fn all_symbols() -> Vec<String> {
    interner_read().iter().map(|(_, s)| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ident_new_and_to_string() {
        let ident = Ident::new("hello");
        assert_eq!(ident.to_string(), "hello");
    }

    #[test]
    fn test_ident_from_str_and_string() {
        let ident1: Ident = "world".into();
        let ident2: Ident = String::from("world").into();
        assert_eq!(ident1, ident2);
        assert_eq!(ident1.to_string(), "world");
    }

    #[test]
    fn test_ident_display_trait() {
        let ident = Ident::new("display_test");
        let s = format!("{}", ident);
        assert_eq!(s, "display_test");
    }

    #[test]
    fn test_ident_resolve_with() {
        let ident = Ident::new("resolve");
        let len = ident.resolve_with(|s| s.len());
        assert_eq!(len, "resolve".len());
    }

    #[cfg(feature = "ast-json")]
    #[test]
    fn test_ident_serde() {
        let ident = Ident::new("serde_test");
        let serialized = serde_json::to_string(&ident).unwrap();
        assert_eq!(serialized, "\"serde_test\"");
        let deserialized: Ident = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, ident);
    }
}
