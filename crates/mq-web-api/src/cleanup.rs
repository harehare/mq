use crate::rate_limiter::RateLimiter;
use std::{sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time::interval};
use tracing::{debug, error, info, warn};

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
            info!("Starting cleanup service with interval: {:?}", interval_duration);

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
        self.handle.as_ref().map(|h| !h.is_finished()).unwrap_or(false)
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
        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));
        let mut cleanup_service = CleanupService::new(rate_limiter, 1);

        assert!(!cleanup_service.is_running());

        cleanup_service.start();
        assert!(cleanup_service.is_running());

        cleanup_service.start();
        assert!(cleanup_service.is_running());

        cleanup_service.stop();

        sleep(Duration::from_millis(10)).await;
        assert!(!cleanup_service.is_running());

        cleanup_service.stop();
        assert!(!cleanup_service.is_running());
    }

    #[tokio::test]
    async fn test_cleanup_removes_expired_records() {
        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));

        let now = current_timestamp();
        let expired_time = now - 3600;

        rate_limiter
            .insert_entry_for_test("expired_user", expired_time - 60, 1, expired_time)
            .await;

        let mut cleanup_service = CleanupService::new(Arc::clone(&rate_limiter), 1);
        cleanup_service.start();

        sleep(Duration::from_secs(2)).await;

        let usage = rate_limiter.get_current_usage("expired_user").await.unwrap();
        assert_eq!(usage, None, "Expired records should have been cleaned up");

        cleanup_service.stop();
    }
}
