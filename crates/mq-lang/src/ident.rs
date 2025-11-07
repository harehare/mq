use std::sync::{LazyLock, Mutex};

use string_interner::{DefaultBackend, DefaultSymbol, StringInterner};

static STRING_INTERNER: LazyLock<Mutex<StringInterner<DefaultBackend>>> =
    LazyLock::new(|| Mutex::new(StringInterner::default()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ident(DefaultSymbol);

impl Ident {
    pub fn new(s: &str) -> Self {
        Self(STRING_INTERNER.lock().unwrap().get_or_intern(s))
    }

    pub fn as_str(&self) -> String {
        STRING_INTERNER.lock().unwrap().resolve(self.0).unwrap().to_string()
    }

    pub fn resolve_with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&str) -> R,
    {
        let interner = STRING_INTERNER.lock().unwrap();
        let resolved = interner.resolve(self.0).unwrap();
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
        self.as_str().serialize(serializer)
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

pub fn all_symbols() -> Vec<String> {
    STRING_INTERNER
        .lock()
        .unwrap()
        .iter()
        .map(|(_, s)| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ident_new_and_as_str() {
        let ident = Ident::new("hello");
        assert_eq!(ident.as_str(), "hello");
    }

    #[test]
    fn test_ident_from_str_and_string() {
        let ident1: Ident = "world".into();
        let ident2: Ident = String::from("world").into();
        assert_eq!(ident1, ident2);
        assert_eq!(ident1.as_str(), "world");
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
