use deadpool::managed;
use libsql::{Builder, Connection, Database};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{debug, error, info};

const MEMORY_DB_URL: &str = ":memory:";

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Database error: {0}")]
    Database(#[from] libsql::Error),
    #[error("Pool error: {0}")]
    Pool(#[from] deadpool::managed::PoolError<libsql::Error>),
    #[error("Rate limit exceeded: {requests} requests in window, limit is {limit}")]
    LimitExceeded { requests: i64, limit: i64 },
    #[error("Configuration error: {0}")]
    Configuration(String),
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub database_url: String,
    pub database_auth_token: Option<String>,
    pub requests_per_window: i64,
    pub window_size_seconds: i64,
    pub cleanup_interval_seconds: i64,
    pub pool_max_size: usize,
    pub pool_timeout_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            database_url: MEMORY_DB_URL.to_string(),
            database_auth_token: None,
            requests_per_window: 100,
            window_size_seconds: 3600,      // 1 hour
            cleanup_interval_seconds: 3600, // Cleanup every hour
            pool_max_size: 10,
            pool_timeout_seconds: 30,
        }
    }
}

type ConnectionPool = deadpool::managed::Pool<ConnectionManager>;

#[derive(Debug)]
pub struct ConnectionManager {
    database: Arc<Database>,
}

impl ConnectionManager {
    pub fn new(database: Database) -> Self {
        Self {
            database: Arc::new(database),
        }
    }
}

impl managed::Manager for ConnectionManager {
    type Type = Connection;
    type Error = libsql::Error;

    fn create(&self) -> impl std::future::Future<Output = Result<Self::Type, Self::Error>> + Send {
        let database = Arc::clone(&self.database);
        async move { database.connect() }
    }

    #[allow(clippy::manual_async_fn)]
    fn recycle(
        &self,
        _conn: &mut Self::Type,
        _metrics: &managed::Metrics,
    ) -> impl std::future::Future<Output = managed::RecycleResult<Self::Error>> + Send {
        async move { Ok(()) }
    }
}

#[derive(Debug)]
pub struct RateLimiter {
    pool: ConnectionPool,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub async fn new(config: RateLimitConfig) -> Result<Self, RateLimitError> {
        let db = match config.clone().database_auth_token {
            Some(token) => {
                Builder::new_remote(config.clone().database_url, token)
                    .build()
                    .await?
            }
            None => Builder::new_local(&config.database_url).build().await?,
        };

        let manager = ConnectionManager::new(db);
        let pool_config = deadpool::managed::PoolConfig::new(config.pool_max_size);
        let pool = deadpool::managed::Pool::builder(manager)
            .config(pool_config)
            .build()
            .map_err(|e| RateLimitError::Configuration(format!("Failed to create pool: {}", e)))?;

        let rate_limiter = Self { pool, config };

        // Run database migrations
        rate_limiter.run_migrations().await?;

        Ok(rate_limiter)
    }

    #[cfg(test)]
    pub async fn get_connection(
        &self,
    ) -> Result<deadpool::managed::Object<ConnectionManager>, RateLimitError> {
        Ok(self.pool.get().await?)
    }

    async fn run_migrations(&self) -> Result<(), RateLimitError> {
        let conn = self.pool.get().await?;

        // Create rate_limits table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS rate_limits (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                identifier TEXT NOT NULL,
                window_start INTEGER NOT NULL,
                request_count INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                expires_at INTEGER NOT NULL
            )",
            (),
        )
        .await?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rate_limits_identifier_window
             ON rate_limits(identifier, window_start)",
            (),
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rate_limits_expires_at
             ON rate_limits(expires_at)",
            (),
        )
        .await?;

        info!("Rate limiter database migrations completed");
        Ok(())
    }

    pub async fn check_and_increment(&self, identifier: &str) -> Result<(), RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);
        let expires_at = window_start + self.config.window_size_seconds;

        let conn = self.pool.get().await?;

        // First, try to increment existing record
        let result = conn
            .execute(
                "UPDATE rate_limits
             SET request_count = request_count + 1
             WHERE identifier = ? AND window_start = ?",
                [identifier, window_start.to_string().as_str()],
            )
            .await?;

        let current_count = if result == 0 {
            // No existing record, create new one
            conn.execute(
                "INSERT INTO rate_limits (identifier, window_start, request_count, expires_at)
                 VALUES (?, ?, 1, ?)",
                [
                    identifier,
                    window_start.to_string().as_str(),
                    expires_at.to_string().as_str(),
                ],
            )
            .await?;
            1
        } else {
            // Get current count
            let mut rows = conn
                .query(
                    "SELECT request_count FROM rate_limits
                 WHERE identifier = ? AND window_start = ?",
                    [identifier, window_start.to_string().as_str()],
                )
                .await?;

            if let Some(row) = rows.next().await? {
                row.get::<i64>(0)?
            } else {
                1
            }
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
        let conn = self.pool.get().await?;

        let deleted_rows = conn
            .execute(
                "DELETE FROM rate_limits WHERE expires_at < ?",
                [now.to_string().as_str()],
            )
            .await?;

        if deleted_rows > 0 {
            debug!("Cleaned up {} expired rate limit records", deleted_rows);
        }

        Ok(deleted_rows)
    }

    pub async fn get_current_usage(&self, identifier: &str) -> Result<Option<i64>, RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);
        let conn = self.pool.get().await?;

        let mut rows = conn
            .query(
                "SELECT request_count FROM rate_limits
             WHERE identifier = ? AND window_start = ?",
                [identifier, window_start.to_string().as_str()],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(row.get::<i64>(0)?))
        } else {
            Ok(None)
        }
    }

    pub async fn reset_limit(&self, identifier: &str) -> Result<(), RateLimitError> {
        let now = current_timestamp();
        let window_start = self.get_window_start(now);
        let conn = self.pool.get().await?;

        conn.execute(
            "DELETE FROM rate_limits WHERE identifier = ? AND window_start = ?",
            [identifier, window_start.to_string().as_str()],
        )
        .await?;

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

    #[tokio::test]
    async fn test_rate_limit_allows_requests_within_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::default()).await.unwrap();
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
        let limiter = RateLimiter::new(config).await.unwrap();
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
        let limiter = RateLimiter::new(RateLimitConfig::default()).await.unwrap();
        // setup_table(&limiter).await;
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
        let config = RateLimitConfig {
            requests_per_window: 5,
            ..RateLimitConfig::default()
        };
        let limiter = RateLimiter::new(config).await.unwrap();
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
        let limiter = RateLimiter::new(RateLimitConfig::default()).await.unwrap();
        // Insert some expired records manually
        let now = current_timestamp();
        let expired_time = now - 3600; // 1 hour ago

        let conn = limiter.pool.get().await.unwrap();
        conn.execute(
            "INSERT INTO rate_limits (identifier, window_start, request_count, expires_at)
             VALUES (?, ?, ?, ?)",
            [
                "expired_user",
                (expired_time - 60).to_string().as_str(),
                "1",
                expired_time.to_string().as_str(),
            ],
        )
        .await
        .unwrap();
        drop(conn); // Release the connection before calling cleanup_expired

        let deleted = limiter.cleanup_expired().await.unwrap();
        assert_eq!(deleted, 1);
    }
}
