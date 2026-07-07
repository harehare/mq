//! Shared lock-file data model for HTTP module imports.
//!
//! Records each fetched URL's SHA-256 hash so a later fetch with different content (a
//! mutable ref that drifted) can be detected. Pure data logic; each fetcher owns the I/O.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::http_import::is_versioned_url;

/// Default lock-file name, mirroring the convention of `deno.lock`/`package-lock.json`.
pub const LOCKFILE_NAME: &str = "mq.lock";

const LOCKFILE_VERSION: &str = "1";

/// SHA-256 hex digest of `content`, shared by every fetcher backend (native `UreqFetcher`,
/// WASM `WasmFetcher`) so they compute identical `mq.lock` hashes for identical content.
pub fn compute_hash(content: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(content.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// The result of checking a freshly-fetched module's hash against the lock file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockCheck {
    /// No entry existed yet for this URL; it should now be recorded.
    NewEntry,
    /// The existing entry's hash matches the freshly fetched content.
    Match,
    /// The existing entry's hash does not match; the remote content has drifted.
    Mismatch { locked: String },
}

/// A parsed `mq.lock` file: maps a fetched module URL to the SHA-256 hex digest of the
/// content that was fetched for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleLock {
    version: String,
    #[serde(default)]
    remote: BTreeMap<String, String>,
}

impl Default for ModuleLock {
    fn default() -> Self {
        Self {
            version: LOCKFILE_VERSION.to_string(),
            remote: BTreeMap::new(),
        }
    }
}

impl ModuleLock {
    /// Parses a lock file from its serialized JSON form.
    pub fn parse(content: &str) -> Result<Self, String> {
        serde_json::from_str(content).map_err(|e| e.to_string())
    }

    /// Serializes the lock file to pretty-printed JSON, with a trailing newline.
    pub fn to_json(&self) -> String {
        format!(
            "{}\n",
            serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
        )
    }

    /// Checks a freshly-fetched content `hash` against the recorded entry for `url`, if any.
    pub fn check(&self, url: &str, hash: &str) -> LockCheck {
        match self.remote.get(url) {
            None => LockCheck::NewEntry,
            Some(locked) if locked == hash => LockCheck::Match,
            Some(locked) => LockCheck::Mismatch { locked: locked.clone() },
        }
    }

    /// Records (or overwrites) the hash for `url`.
    pub fn insert(&mut self, url: impl Into<String>, hash: impl Into<String>) {
        self.remote.insert(url.into(), hash.into());
    }

    /// Drops every entry whose URL is not a versioned (tagged) ref.
    pub fn retain_versioned_only(&mut self) {
        self.remote.retain(|url, _| is_versioned_url(url));
    }

    /// Returns `true` if there are no recorded entries.
    pub fn is_empty(&self) -> bool {
        self.remote.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_empty_with_current_version() {
        let lock = ModuleLock::default();
        assert!(lock.is_empty());
        assert_eq!(lock.version, LOCKFILE_VERSION);
    }

    #[test]
    fn test_check_new_entry() {
        let lock = ModuleLock::default();
        assert_eq!(
            lock.check("https://example.invalid/a.mq", "deadbeef"),
            LockCheck::NewEntry
        );
    }

    #[test]
    fn test_check_match() {
        let mut lock = ModuleLock::default();
        lock.insert("https://example.invalid/a.mq", "deadbeef");
        assert_eq!(lock.check("https://example.invalid/a.mq", "deadbeef"), LockCheck::Match);
    }

    #[test]
    fn test_check_mismatch() {
        let mut lock = ModuleLock::default();
        lock.insert("https://example.invalid/a.mq", "deadbeef");
        assert_eq!(
            lock.check("https://example.invalid/a.mq", "cafebabe"),
            LockCheck::Mismatch {
                locked: "deadbeef".to_string()
            }
        );
    }

    #[test]
    fn test_insert_overwrites_existing_entry() {
        let mut lock = ModuleLock::default();
        lock.insert("https://example.invalid/a.mq", "deadbeef");
        lock.insert("https://example.invalid/a.mq", "cafebabe");
        assert_eq!(lock.check("https://example.invalid/a.mq", "cafebabe"), LockCheck::Match);
    }

    #[test]
    fn test_roundtrip_parse_and_to_json() {
        let mut lock = ModuleLock::default();
        lock.insert("https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq", "abc123");
        lock.insert(
            "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq",
            "def456",
        );

        let json = lock.to_json();
        let parsed = ModuleLock::parse(&json).unwrap();

        assert_eq!(parsed, lock);
    }

    #[test]
    fn test_parse_missing_remote_defaults_to_empty() {
        let lock = ModuleLock::parse(r#"{"version": "1"}"#).unwrap();
        assert!(lock.is_empty());
    }

    #[test]
    fn test_parse_invalid_json_errors() {
        assert!(ModuleLock::parse("not json").is_err());
    }

    #[test]
    fn test_retain_versioned_only_drops_mutable_entries() {
        let mut lock = ModuleLock::default();
        lock.insert(
            "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq",
            "mutable-hash",
        );
        lock.insert(
            "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq",
            "versioned-hash",
        );

        lock.retain_versioned_only();

        assert_eq!(
            lock.check(
                "https://raw.githubusercontent.com/harehare/lisp/HEAD/lisp.mq",
                "mutable-hash"
            ),
            LockCheck::NewEntry
        );
        assert_eq!(
            lock.check(
                "https://raw.githubusercontent.com/harehare/lisp/v0.1.0/lisp.mq",
                "versioned-hash"
            ),
            LockCheck::Match
        );
    }
}
