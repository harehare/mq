use std::{sync::Arc, time::Duration};
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    cleanup::CleanupService,
    config::{Config, LogFormat},
    rate_limiter::RateLimiter,
    routes::create_router,
};

pub fn init_tracing(config: &Config) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| config.log_level.clone().into());

    match config.log_format {
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json())
                .init();
        }
        LogFormat::Text => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer())
                .init();
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

pub async fn start_server(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting mq-web-api server with config: {:?}", config);

    // Initialize rate limiter
    let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit.clone()).await?);
    info!("Rate limiter initialized successfully");

    let app = create_router(&config, rate_limiter.clone()).layer(TraceLayer::new_for_http().on_response(
        |response: &axum::response::Response, latency: Duration, _span: &tracing::Span, req: &axum::http::Request<axum::body::Body>| {
            tracing::info!(
                "response latency: {:?}, method: {}, path: {}, status: {}",
                latency,
                req.method(),
                req.uri().path(),
                response.status()
            );
        },
    ));

    let bind_address = config.bind_address();
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .unwrap_or_else(|_| panic!("Failed to bind to address {}", bind_address));

    let server_url = config.server_url();
    info!("Server running on {}", server_url);
    info!("OpenAPI docs available at {}/openapi.json", server_url);

    // Print available environment variables for configuration
    info!("Configuration options:");
    info!("  HOST: Host to bind to (default: 0.0.0.0)");
    info!("  PORT: Port to bind to (default: 8080)");
    info!("  RUST_LOG or MQ_LOG_LEVEL: Log level (default: mq_web_api=debug,tower_http=debug)");
    info!("  LOG_FORMAT: Log format - 'json' or 'text' (default: json)");
    info!("  CORS_ORIGINS: Comma-separated CORS origins (default: *)");
    info!("  RATE_LIMIT_DATABASE_URL: Rate limit database URL (default: :memory:)");
    info!("  RATE_LIMIT_REQUESTS_PER_WINDOW: Requests per window (default: 100)");
    info!("  RATE_LIMIT_WINDOW_SIZE_SECONDS: Window size in seconds (default: 3600)");
    info!("  RATE_LIMIT_CLEANUP_INTERVAL_SECONDS: Cleanup interval in seconds (default: 3600)");
    info!("  RATE_LIMIT_POOL_MAX_SIZE: Connection pool max size (default: 10)");
    info!("  RATE_LIMIT_POOL_TIMEOUT_SECONDS: Connection pool timeout in seconds (default: 30)");

    // Start cleanup service
    let mut cleanup_service = CleanupService::new(
        Arc::clone(&rate_limiter),
        config.rate_limit.cleanup_interval_seconds as u64,
    );
    cleanup_service.start();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Failed to start server");

    info!("Shutting down mq-web-api server");

    Ok(())
}
