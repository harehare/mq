use axum::{
    Router,
    http::Method,
    middleware,
    response::Redirect,
    routing::{get, post},
};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    config::Config,
    handlers::{ApiDoc, AppState, get_query_api, health_check, post_check_api, post_format_api, post_query_api},
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
        let origins: Result<Vec<_>, _> = config.cors_origins.iter().map(|origin| origin.parse()).collect();

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

    let v1_routes = Router::new()
        .route("/query", get(get_query_api).post(post_query_api))
        .route("/check", post(post_check_api))
        .route("/format", post(post_format_api));

    Router::new()
        .merge(SwaggerUi::new("/docs").url("/api/v1/openapi.json", ApiDoc::openapi()))
        .route("/health", get(health_check))
        .nest("/api/v1", v1_routes)
        // Legacy paths — kept for backward compatibility, redirects to v1
        .route(
            "/api/query",
            get(|| async { Redirect::permanent("/api/v1/query") })
                .post(|| async { Redirect::permanent("/api/v1/query") }),
        )
        .route("/api/check", post(|| async { Redirect::permanent("/api/v1/check") }))
        .route("/api/format", post(|| async { Redirect::permanent("/api/v1/format") }))
        .route(
            "/openapi.json",
            get(|| async { Redirect::permanent("/api/v1/openapi.json") }),
        )
        .layer(
            ServiceBuilder::new()
                .layer(CompressionLayer::new())
                .layer(TraceLayer::new_for_http())
                .layer(cors),
        )
        .layer(middleware::from_fn_with_state(rate_limiter, rate_limit_middleware))
        .with_state(state)
}
