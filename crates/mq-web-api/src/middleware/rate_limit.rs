use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::rate_limiter::{RateLimitError, RateLimiter};

pub async fn rate_limit_middleware(
    State(rate_limiter): State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    let identifier = extract_identifier(&request);

    match rate_limiter.check_and_increment(&identifier).await {
        Ok(()) => {
            debug!("Rate limit check passed for identifier: {}", identifier);

            // Add rate limit headers
            let mut response = next.run(request).await;
            if let Ok(current_usage) = rate_limiter.get_current_usage(&identifier).await {
                add_rate_limit_headers(&mut response, current_usage.unwrap_or(1), &rate_limiter);
            }
            response
        }
        Err(RateLimitError::LimitExceeded { requests, limit }) => {
            warn!(
                "Rate limit exceeded for identifier '{}': {}/{} requests",
                identifier, requests, limit
            );

            let mut response = create_rate_limit_exceeded_response(requests, limit);
            add_rate_limit_headers(&mut response, requests, &rate_limiter);
            response
        }
        Err(err) => {
            warn!("Rate limiter error for identifier '{}': {}", identifier, err);

            // On database errors, allow the request to proceed
            // This ensures the service remains available even if rate limiting fails
            debug!("Allowing request due to rate limiter error");
            next.run(request).await
        }
    }
}

fn extract_identifier(request: &Request) -> String {
    // Try to get identifier from various sources in order of preference:
    // 1. X-Forwarded-For header (for proxied requests)
    // 2. X-Real-IP header
    // 3. Connection remote address
    // 4. Fallback to "unknown"

    if let Some(forwarded_for) = request.headers().get("x-forwarded-for")
        && let Ok(forwarded_str) = forwarded_for.to_str()
    {
        // Take the first IP from the comma-separated list
        if let Some(first_ip) = forwarded_str.split(',').next() {
            return first_ip.trim().to_string();
        }
    }

    if let Some(real_ip) = request.headers().get("x-real-ip")
        && let Ok(ip_str) = real_ip.to_str()
    {
        return ip_str.to_string();
    }

    // Fallback - in a real deployment, you might want to extract from the connection
    "unknown".to_string()
}

fn create_rate_limit_exceeded_response(requests: i64, limit: i64) -> Response {
    let body = serde_json::json!({
        "error": "Rate limit exceeded",
        "message": format!("Too many requests: {}/{} requests in current window", requests, limit),
        "requests": requests,
        "limit": limit
    });

    (
        StatusCode::TOO_MANY_REQUESTS,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn add_rate_limit_headers(response: &mut Response, current_usage: i64, rate_limiter: &RateLimiter) {
    let headers = response.headers_mut();

    // Add current usage
    if let Ok(usage_header) = HeaderValue::from_str(&current_usage.to_string()) {
        headers.insert("X-RateLimit-Used", usage_header);
    }

    // Add limit
    if let Ok(limit_header) = HeaderValue::from_str(&rate_limiter.requests_per_window().to_string()) {
        headers.insert("X-RateLimit-Limit", limit_header);
    }

    // Add remaining requests
    let remaining = (rate_limiter.requests_per_window() - current_usage).max(0);
    if let Ok(remaining_header) = HeaderValue::from_str(&remaining.to_string()) {
        headers.insert("X-RateLimit-Remaining", remaining_header);
    }

    // Add window size
    if let Ok(window_header) = HeaderValue::from_str(&rate_limiter.window_size_seconds().to_string()) {
        headers.insert("X-RateLimit-Window", window_header);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limiter::{RateLimitConfig, RateLimiter};
    use axum::{body::Body, http::Request};

    #[tokio::test]
    async fn test_extract_identifier_from_forwarded_for() {
        let request = Request::builder()
            .header("x-forwarded-for", "192.168.1.1, 10.0.0.1")
            .body(Body::empty())
            .unwrap();

        let identifier = extract_identifier(&request);
        assert_eq!(identifier, "192.168.1.1");
    }

    #[tokio::test]
    async fn test_extract_identifier_from_real_ip() {
        let request = Request::builder()
            .header("x-real-ip", "192.168.1.100")
            .body(Body::empty())
            .unwrap();

        let identifier = extract_identifier(&request);
        assert_eq!(identifier, "192.168.1.100");
    }

    #[tokio::test]
    async fn test_extract_identifier_fallback() {
        let request = Request::builder().body(Body::empty()).unwrap();

        let identifier = extract_identifier(&request);
        assert_eq!(identifier, "unknown");
    }

    #[tokio::test]
    async fn test_rate_limit_headers_added() {
        let config = RateLimitConfig {
            database_url: ":memory:".to_string(),
            database_auth_token: None,
            requests_per_window: 10,
            window_size_seconds: 3600,
            cleanup_interval_seconds: 3600,
            pool_max_size: 10,
            pool_timeout_seconds: 30,
        };
        let rate_limiter = RateLimiter::new(config).await.unwrap();

        let mut response = Response::new(Body::empty());
        add_rate_limit_headers(&mut response, 3, &rate_limiter);

        let headers = response.headers();
        assert_eq!(headers.get("X-RateLimit-Used").unwrap(), "3");
        assert_eq!(headers.get("X-RateLimit-Limit").unwrap(), "10");
        assert_eq!(headers.get("X-RateLimit-Remaining").unwrap(), "7");
        assert_eq!(headers.get("X-RateLimit-Window").unwrap(), "3600");
    }
}
