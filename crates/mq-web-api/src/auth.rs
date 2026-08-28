//! API key authentication and scope-based authorization.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, collections::HashSet, fmt::Write as _};

use crate::config::AuthConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Read,
    Query,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Scope::Read => "read",
            Scope::Query => "query",
        })
    }
}

fn default_scopes() -> Vec<Scope> {
    vec![Scope::Read, Scope::Query]
}

#[derive(Debug, Deserialize)]
struct ApiKeyDef {
    key: String,
    name: String,
    #[serde(default = "default_scopes")]
    scopes: Vec<Scope>,
    rate_limit_per_window: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub name: String,
    pub scopes: HashSet<Scope>,
    pub rate_limit_per_window: Option<i64>,
}

impl ApiKeyRecord {
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }
}

#[derive(Debug)]
pub struct ApiKeyStore {
    by_hash: HashMap<String, ApiKeyRecord>,
}

impl ApiKeyStore {
    fn from_defs(defs: Vec<ApiKeyDef>) -> Self {
        let by_hash = defs
            .into_iter()
            .map(|def| {
                (
                    hash_key(&def.key),
                    ApiKeyRecord {
                        name: def.name,
                        scopes: def.scopes.into_iter().collect(),
                        rate_limit_per_window: def.rate_limit_per_window,
                    },
                )
            })
            .collect();
        Self { by_hash }
    }

    fn from_raw_keys(keys: Vec<String>) -> Self {
        let by_hash = keys
            .into_iter()
            .enumerate()
            .map(|(i, key)| {
                (
                    hash_key(&key),
                    ApiKeyRecord {
                        name: format!("key-{}", i + 1),
                        scopes: default_scopes().into_iter().collect(),
                        rate_limit_per_window: None,
                    },
                )
            })
            .collect();
        Self { by_hash }
    }

    pub fn from_config(config: &AuthConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }

        let store = if let Some(path) = &config.keys_file {
            let content = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("Failed to read API_KEYS_FILE '{}': {}", path, e));
            let defs: Vec<ApiKeyDef> = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("Failed to parse API_KEYS_FILE '{}': {}", path, e));
            Self::from_defs(defs)
        } else {
            Self::from_raw_keys(config.keys.clone())
        };

        assert!(
            !store.by_hash.is_empty(),
            "AUTH_ENABLED=true but no API keys were configured (set API_KEYS or API_KEYS_FILE)"
        );

        Some(store)
    }

    pub fn authenticate(&self, presented: &str) -> Option<&ApiKeyRecord> {
        self.by_hash.get(&hash_key(presented))
    }

    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }
}

fn hash_key(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(hex, "{:02x}", byte).expect("writing to a String never fails");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_keys_get_both_scopes_and_no_override() {
        let store = ApiKeyStore::from_raw_keys(vec!["secret-1".to_string(), "secret-2".to_string()]);
        assert_eq!(store.len(), 2);

        let record = store.authenticate("secret-1").unwrap();
        assert!(record.has_scope(Scope::Read));
        assert!(record.has_scope(Scope::Query));
        assert_eq!(record.rate_limit_per_window, None);
    }

    #[test]
    fn test_unknown_key_is_rejected() {
        let store = ApiKeyStore::from_raw_keys(vec!["secret-1".to_string()]);
        assert!(store.authenticate("not-a-key").is_none());
    }

    #[test]
    fn test_defs_carry_scopes_and_rate_limit_override() {
        let defs: Vec<ApiKeyDef> = serde_json::from_str(
            r#"[
                {"key": "readonly-key", "name": "acme-readonly", "scopes": ["read"]},
                {"key": "full-key", "name": "acme-full", "rate_limit_per_window": 5000}
            ]"#,
        )
        .unwrap();
        let store = ApiKeyStore::from_defs(defs);

        let readonly = store.authenticate("readonly-key").unwrap();
        assert_eq!(readonly.name, "acme-readonly");
        assert!(readonly.has_scope(Scope::Read));
        assert!(!readonly.has_scope(Scope::Query));
        assert_eq!(readonly.rate_limit_per_window, None);

        let full = store.authenticate("full-key").unwrap();
        assert_eq!(full.name, "acme-full");
        assert!(full.has_scope(Scope::Read));
        assert!(full.has_scope(Scope::Query));
        assert_eq!(full.rate_limit_per_window, Some(5000));
    }

    #[test]
    fn test_from_config_disabled_returns_none() {
        let config = AuthConfig {
            enabled: false,
            keys: vec!["secret".to_string()],
            keys_file: None,
        };
        assert!(ApiKeyStore::from_config(&config).is_none());
    }

    #[test]
    #[should_panic(expected = "no API keys were configured")]
    fn test_from_config_enabled_without_keys_panics() {
        let config = AuthConfig {
            enabled: true,
            keys: vec![],
            keys_file: None,
        };
        ApiKeyStore::from_config(&config);
    }

    #[test]
    fn test_hash_key_is_deterministic_and_distinct() {
        assert_eq!(hash_key("same"), hash_key("same"));
        assert_ne!(hash_key("a"), hash_key("b"));
    }
}
