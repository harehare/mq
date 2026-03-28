use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::debug;

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded: {requests} requests in window, limit is {limit}")]
    LimitExceeded { requests: i64, limit: i64 },
    #[error("Configuration error: {0}")]
    Configuration(String),
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_window: i64,
    pub window_size_seconds: i64,
    pub cleanup_interval_seconds: i64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_window: 100,
            window_size_seconds: 3600,      // 1 hour
            cleanup_interval_seconds: 3600, // Cleanup every hour
        }
    }
}

#[derive(Debug)]
struct RateLimitEntry {
    window_start: i64,
    request_count: i64,
    expires_at: i64,
}

#[derive(Debug)]
pub struct RateLimiter {
    store: Mutex<HashMap<String, RateLimitEntry>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
            config,
        }
    }

    pub async fn check_and_increment(&self, identifier: &str) -> Result<(), RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);
        let expires_at = window_start + self.config.window_size_seconds;

        let mut store = self.store.lock().await;
        let entry = store.entry(identifier.to_string()).or_insert_with(|| RateLimitEntry {
            window_start,
            request_count: 0,
            expires_at,
        });

        if entry.window_start != window_start {
            entry.window_start = window_start;
            entry.request_count = 0;
            entry.expires_at = expires_at;
        }

        entry.request_count += 1;
        let count = entry.request_count;

        debug!(
            "Rate limit check for '{}': {}/{} requests in current window",
            identifier, count, self.config.requests_per_window
        );

        if count > self.config.requests_per_window {
            return Err(RateLimitError::LimitExceeded {
                requests: count,
                limit: self.config.requests_per_window,
            });
        }

        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<u64, RateLimitError> {
        let now = current_timestamp();
        let mut store = self.store.lock().await;
        let before = store.len();
        store.retain(|_, entry| entry.expires_at >= now);
        let deleted = (before - store.len()) as u64;

        if deleted > 0 {
            debug!("Cleaned up {} expired rate limit records", deleted);
        }

        Ok(deleted)
    }

    pub async fn get_current_usage(&self, identifier: &str) -> Result<Option<i64>, RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);
        let store = self.store.lock().await;

        if let Some(entry) = store.get(identifier)
            && entry.window_start == window_start
        {
            return Ok(Some(entry.request_count));
        }
        Ok(None)
    }

    pub async fn reset_limit(&self, identifier: &str) -> Result<(), RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);
        let mut store = self.store.lock().await;

        if let Some(entry) = store.get(identifier)
            && entry.window_start == window_start
        {
            store.remove(identifier);
        }

        debug!("Reset rate limit for identifier '{}'", identifier);
        Ok(())
    }

    fn get_window_start(&self, timestamp: i64) -> i64 {
        (timestamp / self.config.window_size_seconds) * self.config.window_size_seconds
    }

    pub fn requests_per_window(&self) -> i64 {
        self.config.requests_per_window
    }

    pub fn window_size_seconds(&self) -> i64 {
        self.config.window_size_seconds
    }

    #[cfg(test)]
    pub async fn insert_entry_for_test(
        &self,
        identifier: &str,
        window_start: i64,
        request_count: i64,
        expires_at: i64,
    ) {
        let mut store = self.store.lock().await;
        store.insert(
            identifier.to_string(),
            RateLimitEntry {
                window_start,
                request_count,
                expires_at,
            },
        );
    }
}

pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limit_allows_requests_within_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        let identifier = "test_user";

        for i in 1..=5 {
            let result = limiter.check_and_increment(identifier).await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[tokio::test]
    async fn test_rate_limit_blocks_excess_requests() {
        let config = RateLimitConfig {
            requests_per_window: 5,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);
        let identifier = "test_user";

        for _ in 1..=5 {
            limiter.check_and_increment(identifier).await.unwrap();
        }

        let result = limiter.check_and_increment(identifier).await;
        assert!(matches!(result, Err(RateLimitError::LimitExceeded { .. })));
    }

    #[tokio::test]
    async fn test_get_current_usage() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        let identifier = "test_user";

        let usage = limiter.get_current_usage(identifier).await.unwrap();
        assert_eq!(usage, None);

        for _ in 1..=3 {
            limiter.check_and_increment(identifier).await.unwrap();
        }

        let usage = limiter.get_current_usage(identifier).await.unwrap();
        assert_eq!(usage, Some(3));
    }

    #[tokio::test]
    async fn test_reset_limit() {
        let config = RateLimitConfig {
            requests_per_window: 5,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config);
        let identifier = "test_user";

        for _ in 1..=5 {
            limiter.check_and_increment(identifier).await.unwrap();
        }

        limiter.reset_limit(identifier).await.unwrap();

        let result = limiter.check_and_increment(identifier).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let limiter = RateLimiter::new(RateLimitConfig::default());
        let now = current_timestamp();
        let expired_time = now - 3600;

        limiter
            .insert_entry_for_test("expired_user", expired_time - 60, 1, expired_time)
            .await;

        let deleted = limiter.cleanup_expired().await.unwrap();
        assert_eq!(deleted, 1);
    }
}
