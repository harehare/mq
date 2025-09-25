use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{debug, error};

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("Rate limit exceeded: {requests} requests in window, limit is {limit}")]
    LimitExceeded { requests: i64, limit: i64 },
    #[error("Configuration error: {0}")]
    Configuration(String),
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub database_url: String,
    pub requests_per_window: i64,
    pub window_size_seconds: i64,
    pub cleanup_interval_seconds: i64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            database_url: ":memory:".to_string(),
            requests_per_window: 100,
            window_size_seconds: 3600,      // 1 hour
            cleanup_interval_seconds: 3600, // Cleanup every hour
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimiter {
    pool: SqlitePool,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub async fn new(config: RateLimitConfig) -> Result<Self, RateLimitError> {
        let pool = SqlitePool::connect(&config.database_url)
            .await
            .map_err(RateLimitError::Database)?;

        let rate_limiter = Self { pool, config };

        // Run database migrations
        rate_limiter.run_migrations().await?;

        Ok(rate_limiter)
    }

    #[cfg(test)]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn run_migrations(&self) -> Result<(), RateLimitError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        debug!("Rate limiter database migrations completed");
        Ok(())
    }

    pub async fn check_and_increment(&self, identifier: &str) -> Result<(), RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);
        let expires_at = window_start + self.config.window_size_seconds;

        // First, try to increment existing record
        let result = sqlx::query(
            "UPDATE rate_limits
             SET request_count = request_count + 1
             WHERE identifier = ? AND window_start = ?",
        )
        .bind(identifier)
        .bind(window_start)
        .execute(&self.pool)
        .await
        .map_err(RateLimitError::Database)?;

        let current_count = if result.rows_affected() == 0 {
            // No existing record, create new one
            sqlx::query(
                "INSERT INTO rate_limits (identifier, window_start, request_count, expires_at)
                 VALUES (?, ?, 1, ?)",
            )
            .bind(identifier)
            .bind(window_start)
            .bind(expires_at)
            .execute(&self.pool)
            .await
            .map_err(RateLimitError::Database)?;
            1
        } else {
            sqlx::query_scalar::<_, i64>(
                "SELECT request_count FROM rate_limits
                 WHERE identifier = ? AND window_start = ?",
            )
            .bind(identifier)
            .bind(window_start)
            .fetch_one(&self.pool)
            .await
            .map_err(RateLimitError::Database)?
        };

        debug!(
            "Rate limit check for '{}': {}/{} requests in current window",
            identifier, current_count, self.config.requests_per_window
        );

        if current_count > self.config.requests_per_window {
            return Err(RateLimitError::LimitExceeded {
                requests: current_count,
                limit: self.config.requests_per_window,
            });
        }

        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<u64, RateLimitError> {
        let now = current_timestamp();

        let result = sqlx::query("DELETE FROM rate_limits WHERE expires_at < ?")
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(RateLimitError::Database)?;

        let deleted_rows = result.rows_affected();

        if deleted_rows > 0 {
            debug!("Cleaned up {} expired rate limit records", deleted_rows);
        }

        Ok(deleted_rows)
    }

    pub async fn get_current_usage(&self, identifier: &str) -> Result<Option<i64>, RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);

        let result = sqlx::query_scalar::<_, i64>(
            "SELECT request_count FROM rate_limits
             WHERE identifier = ? AND window_start = ?",
        )
        .bind(identifier)
        .bind(window_start)
        .fetch_optional(&self.pool)
        .await
        .map_err(RateLimitError::Database)?;

        Ok(result)
    }

    pub async fn reset_limit(&self, identifier: &str) -> Result<(), RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);

        sqlx::query("DELETE FROM rate_limits WHERE identifier = ? AND window_start = ?")
            .bind(identifier)
            .bind(window_start)
            .execute(&self.pool)
            .await
            .map_err(RateLimitError::Database)?;

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

    async fn create_test_limiter() -> RateLimiter {
        let config = RateLimitConfig {
            database_url: ":memory:".to_string(),
            requests_per_window: 5,
            window_size_seconds: 60,
            cleanup_interval_seconds: 60,
        };
        RateLimiter::new(config).await.unwrap()
    }

    #[tokio::test]
    async fn test_rate_limit_allows_requests_within_limit() {
        let limiter = create_test_limiter().await;
        let identifier = "test_user";

        for i in 1..=5 {
            let result = limiter.check_and_increment(identifier).await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[tokio::test]
    async fn test_rate_limit_blocks_excess_requests() {
        let limiter = create_test_limiter().await;
        let identifier = "test_user";

        // Fill up the limit
        for _ in 1..=5 {
            limiter.check_and_increment(identifier).await.unwrap();
        }

        // Next request should be blocked
        let result = limiter.check_and_increment(identifier).await;
        assert!(matches!(result, Err(RateLimitError::LimitExceeded { .. })));
    }

    #[tokio::test]
    async fn test_get_current_usage() {
        let limiter = create_test_limiter().await;
        let identifier = "test_user";

        // Initially no usage
        let usage = limiter.get_current_usage(identifier).await.unwrap();
        assert_eq!(usage, None);

        // Make some requests
        for _ in 1..=3 {
            limiter.check_and_increment(identifier).await.unwrap();
        }

        let usage = limiter.get_current_usage(identifier).await.unwrap();
        assert_eq!(usage, Some(3));
    }

    #[tokio::test]
    async fn test_reset_limit() {
        let limiter = create_test_limiter().await;
        let identifier = "test_user";

        // Fill up the limit
        for _ in 1..=5 {
            limiter.check_and_increment(identifier).await.unwrap();
        }

        // Reset the limit
        limiter.reset_limit(identifier).await.unwrap();

        // Should be able to make requests again
        let result = limiter.check_and_increment(identifier).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let limiter = create_test_limiter().await;

        // Insert some expired records manually
        let now = current_timestamp();
        let expired_time = now - 3600; // 1 hour ago

        sqlx::query(
            "INSERT INTO rate_limits (identifier, window_start, request_count, expires_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind("expired_user")
        .bind(expired_time - 60)
        .bind(1)
        .bind(expired_time)
        .execute(&limiter.pool)
        .await
        .unwrap();

        let deleted = limiter.cleanup_expired().await.unwrap();
        assert_eq!(deleted, 1);
    }
}
