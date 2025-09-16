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
        STRING_INTERNER
            .lock()
            .unwrap()
            .resolve(self.0)
            .unwrap()
            .to_string()
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
