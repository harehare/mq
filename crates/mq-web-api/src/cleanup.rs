use std::{sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time::interval};
use tracing::{debug, error, info, warn};
use libsql::params;
use crate::rate_limiter::RateLimiter;

pub struct CleanupService {
    rate_limiter: Arc<RateLimiter>,
    cleanup_interval: Duration,
    handle: Option<JoinHandle<()>>,
}

impl CleanupService {
    pub fn new(rate_limiter: Arc<RateLimiter>, cleanup_interval_seconds: u64) -> Self {
        Self {
            rate_limiter,
            cleanup_interval: Duration::from_secs(cleanup_interval_seconds),
            handle: None,
        }
    }

    pub fn start(&mut self) {
        if self.handle.is_some() {
            warn!("Cleanup service is already running");
            return;
        }

        let rate_limiter = Arc::clone(&self.rate_limiter);
        let interval_duration = self.cleanup_interval;

        let handle = tokio::spawn(async move {
            info!(
                "Starting cleanup service with interval: {:?}",
                interval_duration
            );

            let mut cleanup_interval = interval(interval_duration);

            loop {
                match rate_limiter.cleanup_expired().await {
                    Ok(deleted_count) => {
                        if deleted_count > 0 {
                            info!("Cleaned up {} expired rate limit records", deleted_count);
                        } else {
                            debug!("No expired rate limit records to clean up");
                        }
                    }
                    Err(e) => {
                        error!("Failed to cleanup expired rate limit records: {}", e);
                    }
                }

                cleanup_interval.tick().await;
            }
        });

        self.handle = Some(handle);
        info!("Cleanup service started successfully");
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            info!("Cleanup service stopped");
        } else {
            debug!("Cleanup service is not running");
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }
}

impl Drop for CleanupService {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limiter::{RateLimitConfig, RateLimiter, current_timestamp};
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn test_cleanup_service_lifecycle() {
        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()).await.unwrap());
        let mut cleanup_service = CleanupService::new(rate_limiter, 1);

        // Service should not be running initially
        assert!(!cleanup_service.is_running());

        // Start the service
        cleanup_service.start();
        assert!(cleanup_service.is_running());

        // Starting again should warn but not create duplicate
        cleanup_service.start();
        assert!(cleanup_service.is_running());

        // Stop the service
        cleanup_service.stop();

        // Give a moment for the task to be aborted
        sleep(Duration::from_millis(10)).await;
        assert!(!cleanup_service.is_running());

        // Stopping again should be safe
        cleanup_service.stop();
        assert!(!cleanup_service.is_running());
    }

    #[tokio::test]
    async fn test_cleanup_removes_expired_records() {
        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()).await.unwrap());

        // Insert some expired records manually
        let now = current_timestamp();
        let expired_time = now - 3600; // 1 hour ago

        {
            let conn = rate_limiter.get_connection().await.unwrap();
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
        }

        // Create cleanup service with very short interval for testing
        let mut cleanup_service = CleanupService::new(Arc::clone(&rate_limiter), 1);
        cleanup_service.start();

        // Wait for cleanup to run at least once
        sleep(Duration::from_secs(2)).await;

        // Check that expired records were cleaned up
        let mut rows = {
            let conn = rate_limiter.get_connection().await.unwrap();
            conn.query(
                "SELECT COUNT(*) FROM rate_limits WHERE identifier = ?",
                params!["expired_user"],
            )
            .await
            .unwrap()
        };

        let count = if let Some(row) = rows.next().await.unwrap() {
            row.get::<i64>(0).unwrap()
        } else {
            0
        };

        assert_eq!(count, 0, "Expired records should have been cleaned up");

        cleanup_service.stop();
    }
}
