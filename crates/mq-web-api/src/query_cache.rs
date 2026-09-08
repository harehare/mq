//! Short-lived in-memory cache for `/query`-family results, keyed by the full
//! request shape (`query`, `input`, `input_format`, `modules`, `args`,
//! `output_format`, `aggregate`).
//!
//! Skipped for queries that call a nondeterministic builtin (`now`, `uuid`,
//! `uuid_v4`, `uuid_v7`, `rand`, `rand_int`, `random_string`): caching those
//! would hand a stale timestamp/UUID/random value to unrelated callers.
//! `http`/`read_file`/`write_file` don't need the same treatment — mq-web-api
//! never enables mq-lang's `http`/`file-io` features, so those builtins
//! aren't compiled into this binary and can't be called at all.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::api::{ApiRequest, InputFormat, OutputFormat, QueryApiResponse};

const NONDETERMINISTIC_BUILTINS: &[&str] = &["now", "uuid", "uuid_v4", "uuid_v7", "rand", "rand_int", "random_string"];

#[derive(Debug, Clone)]
pub struct QueryCacheConfig {
    pub enabled: bool,
    pub ttl: Duration,
    pub max_entries: usize,
    /// Maximum bytes retained by one cache key and response combined.
    pub max_entry_bytes: usize,
    /// Maximum bytes retained by all cache keys and responses combined.
    pub max_total_bytes: usize,
}

impl Default for QueryCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl: Duration::from_secs(30),
            max_entries: 1000,
            max_entry_bytes: 1024 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
        }
    }
}

struct CacheEntry {
    response: QueryApiResponse,
    inserted_at: Instant,
    size_bytes: usize,
}

#[derive(Default)]
struct CacheStore {
    entries: HashMap<String, CacheEntry>,
    total_bytes: usize,
}

/// A capacity-bounded, TTL-expiring cache of query results.
///
/// Entries are checked for expiry lazily on [`QueryCache::get`]. Both the
/// number of entries and their retained bytes are bounded; oversized responses
/// are deliberately served but never cached.
pub struct QueryCache {
    store: Mutex<CacheStore>,
    config: QueryCacheConfig,
}

impl QueryCache {
    pub fn new(config: QueryCacheConfig) -> Self {
        Self {
            store: Mutex::new(CacheStore::default()),
            config,
        }
    }

    pub async fn get(&self, key: &str) -> Option<QueryApiResponse> {
        if !self.config.enabled {
            return None;
        }

        let mut store = self.store.lock().await;
        let expired = store
            .entries
            .get(key)
            .is_some_and(|entry| entry.inserted_at.elapsed() >= self.config.ttl);
        if expired {
            remove_entry(&mut store, key);
            return None;
        }
        store.entries.get(key).map(|entry| entry.response.clone())
    }

    pub async fn insert(&self, key: String, response: QueryApiResponse) {
        if !self.config.enabled
            || self.config.max_entries == 0
            || self.config.max_entry_bytes == 0
            || self.config.max_total_bytes == 0
        {
            return;
        }

        let size_bytes = cache_entry_size(&key, &response);
        if size_bytes > self.config.max_entry_bytes || size_bytes > self.config.max_total_bytes {
            return;
        }

        let mut store = self.store.lock().await;
        remove_expired_entries(&mut store, self.config.ttl);
        remove_entry(&mut store, &key);

        while store.entries.len() >= self.config.max_entries
            || store.total_bytes.saturating_add(size_bytes) > self.config.max_total_bytes
        {
            let Some(evicted) = store.entries.keys().next().cloned() else {
                break;
            };
            remove_entry(&mut store, &evicted);
        }

        store.total_bytes = store.total_bytes.saturating_add(size_bytes);
        store.entries.insert(
            key,
            CacheEntry {
                response,
                inserted_at: Instant::now(),
                size_bytes,
            },
        );
    }

    #[cfg(test)]
    pub async fn entry_count(&self) -> usize {
        self.store.lock().await.entries.len()
    }

    #[cfg(test)]
    pub async fn total_bytes(&self) -> usize {
        self.store.lock().await.total_bytes
    }
}

fn cache_entry_size(key: &str, response: &QueryApiResponse) -> usize {
    key.len()
        .saturating_add(response.results.iter().map(String::len).sum::<usize>())
}

fn remove_entry(store: &mut CacheStore, key: &str) {
    if let Some(entry) = store.entries.remove(key) {
        store.total_bytes = store.total_bytes.saturating_sub(entry.size_bytes);
    }
}

fn remove_expired_entries(store: &mut CacheStore, ttl: Duration) {
    let expired: Vec<String> = store
        .entries
        .iter()
        .filter(|(_, entry)| entry.inserted_at.elapsed() >= ttl)
        .map(|(key, _)| key.clone())
        .collect();
    for key in expired {
        remove_entry(store, &key);
    }
}

#[derive(serde::Serialize)]
struct CacheKey<'a> {
    query: &'a str,
    input: &'a Option<String>,
    input_format: &'a Option<InputFormat>,
    modules: &'a Option<Vec<String>>,
    // `HashMap` iteration order isn't stable across instances, so args are
    // sorted into a `BTreeMap` before hashing to keep the key deterministic.
    args: Option<BTreeMap<&'a String, &'a String>>,
    output_format: &'a Option<OutputFormat>,
    aggregate: Option<bool>,
}

/// Cache key for `request`, or `None` if the query isn't safe to cache.
pub fn cache_key(request: &ApiRequest) -> Option<String> {
    if calls_nondeterministic_builtin(&request.query) {
        return None;
    }

    let key = CacheKey {
        query: &request.query,
        input: &request.input,
        input_format: &request.input_format,
        modules: &request.modules,
        args: request.args.as_ref().map(|m| m.iter().collect()),
        output_format: &request.output_format,
        aggregate: request.aggregate,
    };

    serde_json::to_string(&key).ok().map(|serialized| {
        Sha256::digest(serialized)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
}

fn calls_nondeterministic_builtin(query: &str) -> bool {
    let mut hir = mq_hir::Hir::default();
    let (source_id, _) = hir.add_code(None, query);

    hir.find_symbols_in_source(source_id).iter().any(|symbol| {
        matches!(symbol.kind, mq_hir::SymbolKind::Call)
            && symbol
                .value
                .as_deref()
                .is_some_and(|name| NONDETERMINISTIC_BUILTINS.contains(&name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::collections::HashMap;

    fn request(query: &str) -> ApiRequest {
        ApiRequest {
            query: query.to_string(),
            input: Some("# Title".to_string()),
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        }
    }

    #[rstest]
    #[case("now()")]
    #[case("uuid()")]
    #[case("uuid_v4()")]
    #[case("uuid_v7()")]
    #[case("rand()")]
    #[case("rand_int(1, 10)")]
    #[case(r#"random_string(8, "abc")"#)]
    #[case(r#"def wrapper(): now(); | wrapper()"#)]
    fn nondeterministic_queries_are_not_cached(#[case] query: &str) {
        assert!(cache_key(&request(query)).is_none(), "query: {query}");
    }

    #[rstest]
    #[case(".h1")]
    #[case(r#"let x = "now""#)]
    #[case(r#"def now_like(): 1; | now_like()"#)]
    fn deterministic_queries_are_cached(#[case] query: &str) {
        assert!(cache_key(&request(query)).is_some(), "query: {query}");
    }

    #[test]
    fn cache_key_is_stable_regardless_of_args_insertion_order() {
        let mut args_a = HashMap::new();
        args_a.insert("a".to_string(), "1".to_string());
        args_a.insert("b".to_string(), "2".to_string());

        let mut args_b = HashMap::new();
        args_b.insert("b".to_string(), "2".to_string());
        args_b.insert("a".to_string(), "1".to_string());

        let req_a = ApiRequest {
            args: Some(args_a),
            ..request(".h1")
        };
        let req_b = ApiRequest {
            args: Some(args_b),
            ..request(".h1")
        };

        assert_eq!(cache_key(&req_a), cache_key(&req_b));
    }

    #[tokio::test]
    async fn get_returns_none_before_insert() {
        let cache = QueryCache::new(QueryCacheConfig::default());
        assert_eq!(cache.get("missing").await, None);
    }

    #[tokio::test]
    async fn insert_then_get_hits() {
        let cache = QueryCache::new(QueryCacheConfig::default());
        let response = QueryApiResponse {
            results: vec!["# Title".to_string()],
        };
        cache.insert("key".to_string(), response.clone()).await;

        let cached = cache.get("key").await.expect("expected cache hit");
        assert_eq!(cached.results, response.results);
    }

    #[tokio::test]
    async fn expired_entries_are_not_returned() {
        let cache = QueryCache::new(QueryCacheConfig {
            ttl: Duration::from_millis(1),
            ..QueryCacheConfig::default()
        });
        cache
            .insert(
                "key".to_string(),
                QueryApiResponse {
                    results: vec!["x".to_string()],
                },
            )
            .await;

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(cache.get("key").await, None);
    }

    #[tokio::test]
    async fn disabled_cache_never_stores_or_returns() {
        let cache = QueryCache::new(QueryCacheConfig {
            enabled: false,
            ..QueryCacheConfig::default()
        });
        cache
            .insert(
                "key".to_string(),
                QueryApiResponse {
                    results: vec!["x".to_string()],
                },
            )
            .await;
        assert_eq!(cache.get("key").await, None);
        assert_eq!(cache.entry_count().await, 0);
    }

    #[tokio::test]
    async fn insert_evicts_when_over_capacity() {
        let cache = QueryCache::new(QueryCacheConfig {
            max_entries: 2,
            ttl: Duration::from_secs(60),
            ..QueryCacheConfig::default()
        });

        for i in 0..3 {
            cache
                .insert(
                    format!("key-{i}"),
                    QueryApiResponse {
                        results: vec![i.to_string()],
                    },
                )
                .await;
        }

        assert!(cache.entry_count().await <= 2);
    }

    #[tokio::test]
    async fn oversized_responses_are_not_cached() {
        let cache = QueryCache::new(QueryCacheConfig {
            max_entry_bytes: 8,
            ..QueryCacheConfig::default()
        });
        cache
            .insert(
                "key".to_string(),
                QueryApiResponse {
                    results: vec!["too large".to_string()],
                },
            )
            .await;

        assert_eq!(cache.entry_count().await, 0);
        assert_eq!(cache.total_bytes().await, 0);
    }

    #[tokio::test]
    async fn insert_evicts_to_respect_total_byte_capacity() {
        let cache = QueryCache::new(QueryCacheConfig {
            max_entries: 10,
            max_entry_bytes: 16,
            max_total_bytes: 9,
            ..QueryCacheConfig::default()
        });
        for key in ["a", "b"] {
            cache
                .insert(
                    key.to_string(),
                    QueryApiResponse {
                        results: vec!["1234".to_string()],
                    },
                )
                .await;
        }

        assert_eq!(cache.entry_count().await, 1);
        assert!(cache.total_bytes().await <= 9);
    }

    #[test]
    fn cache_key_does_not_retain_request_content() {
        let input = "sensitive input".repeat(1024);
        let key = cache_key(&ApiRequest {
            input: Some(input.clone()),
            ..request(".")
        })
        .expect("deterministic query should have a cache key");

        assert_eq!(key.len(), 64);
        assert!(!key.contains(&input));
    }
}
