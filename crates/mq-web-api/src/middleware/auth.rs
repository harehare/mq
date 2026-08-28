use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::{
    auth::{ApiKeyRecord, ApiKeyStore, Scope},
    problem::ProblemDetails,
    rate_limiter::{RateLimitError, RateLimiter},
};

#[derive(Clone)]
pub struct AuthState {
    pub store: Arc<ApiKeyStore>,
    pub rate_limiter: Arc<RateLimiter>,
}

#[derive(Clone)]
pub struct AuthContext {
    pub key_name: String,
}

fn required_scope(path: &str) -> Option<Scope> {
    if path == "/health"
        || path.starts_with("/docs")
        || path == "/api/v1/openapi.json"
        || path == "/openapi.json"
        || path == "/api/query"
        || path == "/api/check"
        || path == "/api/format"
    {
        return None;
    }

    if path == "/api/v1/query" || path == "/api/v1/batch" {
        return Some(Scope::Query);
    }

    if path.starts_with("/api/v1/") {
        return Some(Scope::Read);
    }

    Some(Scope::Query)
}

fn extract_bearer(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

pub async fn auth_middleware(State(auth): State<AuthState>, mut request: Request, next: Next) -> Response {
    let Some(required) = required_scope(request.uri().path()) else {
        return next.run(request).await;
    };

    let Some(presented) = extract_bearer(&request) else {
        return unauthorized("Missing API key");
    };

    let Some(record) = auth.store.authenticate(presented) else {
        warn!("Auth failed: unknown API key presented for {}", request.uri().path());
        return unauthorized("Invalid API key");
    };

    if !record.has_scope(required) {
        warn!(
            "Auth failed: key '{}' lacks scope {:?} for {}",
            record.name,
            required,
            request.uri().path()
        );
        return forbidden(&record.name, required);
    }

    if let Some(response) = check_key_quota(&auth.rate_limiter, record).await {
        return response;
    }

    debug!("Authenticated request as '{}'", record.name);
    request.extensions_mut().insert(AuthContext {
        key_name: record.name.clone(),
    });

    next.run(request).await
}

async fn check_key_quota(rate_limiter: &RateLimiter, record: &ApiKeyRecord) -> Option<Response> {
    let identifier = format!("key:{}", record.name);

    match rate_limiter
        .check_and_increment_with_limit(&identifier, record.rate_limit_per_window)
        .await
    {
        Ok(()) => None,
        Err(RateLimitError::LimitExceeded { requests, limit }) => {
            warn!(
                "Per-key rate limit exceeded for '{}': {}/{}",
                record.name, requests, limit
            );
            Some(key_rate_limited(requests, limit))
        }
        Err(err) => {
            warn!("Rate limiter error for key '{}': {}", record.name, err);
            None
        }
    }
}

fn unauthorized(detail: &str) -> Response {
    let mut response = ProblemDetails::new(StatusCode::UNAUTHORIZED)
        .with_title("Unauthorized")
        .with_detail("error", detail)
        .into_response();
    response
        .headers_mut()
        .insert(axum::http::header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

fn forbidden(key_name: &str, required: Scope) -> Response {
    ProblemDetails::new(StatusCode::FORBIDDEN)
        .with_title("Insufficient scope")
        .with_detail("key", key_name)
        .with_detail("required_scope", &required.to_string())
        .into_response()
}

fn key_rate_limited(requests: i64, limit: i64) -> Response {
    let mut response = ProblemDetails::new(StatusCode::TOO_MANY_REQUESTS)
        .with_title("API key rate limit exceeded")
        .with_detail("requests", &requests.to_string())
        .with_detail("limit", &limit.to_string())
        .into_response();
    let headers = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
        headers.insert("X-RateLimit-Key-Limit", v);
    }
    if let Ok(v) = HeaderValue::from_str(&requests.to_string()) {
        headers.insert("X-RateLimit-Key-Used", v);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_scope_public_paths() {
        for path in ["/health", "/docs", "/docs/", "/api/v1/openapi.json", "/openapi.json"] {
            assert_eq!(required_scope(path), None, "path: {}", path);
        }
    }

    #[test]
    fn test_required_scope_query_paths() {
        for path in ["/api/v1/query", "/api/v1/batch", "/.h1", "/upcase"] {
            assert_eq!(required_scope(path), Some(Scope::Query), "path: {}", path);
        }
    }

    #[test]
    fn test_required_scope_read_paths() {
        for path in [
            "/api/v1/functions",
            "/api/v1/functions/map",
            "/api/v1/selectors",
            "/api/v1/check",
            "/api/v1/format",
            "/api/v1/lint",
        ] {
            assert_eq!(required_scope(path), Some(Scope::Read), "path: {}", path);
        }
    }

    #[test]
    fn test_extract_bearer() {
        let request = Request::builder()
            .header("authorization", "Bearer my-key")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer(&request), Some("my-key"));

        let missing = Request::builder().body(axum::body::Body::empty()).unwrap();
        assert_eq!(extract_bearer(&missing), None);

        let wrong_scheme = Request::builder()
            .header("authorization", "Basic dXNlcjpwYXNz")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer(&wrong_scheme), None);
    }
}
