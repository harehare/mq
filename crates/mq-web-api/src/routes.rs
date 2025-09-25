use axum::{Router, http::Method, middleware, routing::get};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    config::Config,
    handlers::{AppState, get_diagnostics_api, get_query_api, openapi_json, post_query_api},
    middleware::rate_limit_middleware,
    rate_limiter::RateLimiter,
};

pub fn create_router(config: &Config, rate_limiter: Arc<RateLimiter>) -> Router {
    let state = AppState {};

    let cors = if config.cors_origins.contains(&"*".to_string()) {
        CorsLayer::new()
            .allow_methods([Method::GET, Method::POST])
            .allow_headers(Any)
            .allow_origin(Any)
    } else {
        let origins: Result<Vec<_>, _> = config
            .cors_origins
            .iter()
            .map(|origin| origin.parse())
            .collect();

        match origins {
            Ok(origins) => CorsLayer::new()
                .allow_methods([Method::GET, Method::POST])
                .allow_headers(Any)
                .allow_origin(origins),
            Err(_) => {
                eprintln!("Warning: Invalid CORS origins, falling back to allow all");
                CorsLayer::new()
                    .allow_methods([Method::GET, Method::POST])
                    .allow_headers(Any)
                    .allow_origin(Any)
            }
        }
    };

    Router::new()
        .route("/api/query", get(get_query_api).post(post_query_api))
        .route("/api/query/diagnostics", get(get_diagnostics_api))
        .route("/openapi.json", get(openapi_json))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(cors),
        )
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_middleware,
        ))
        .with_state(state)
}
