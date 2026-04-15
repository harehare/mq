use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "otel")]
static TRACER_PROVIDER: std::sync::OnceLock<opentelemetry_sdk::trace::SdkTracerProvider> =
    std::sync::OnceLock::new();

use crate::{
    cleanup::CleanupService,
    config::{Config, LogFormat},
    rate_limiter::RateLimiter,
    routes::create_router,
};

/// Builds and installs a global tracing subscriber.
///
/// When the `otel` feature is enabled and `config.otel_endpoint` is set,
/// an OTLP span exporter is attached so traces are forwarded to the
/// configured OpenTelemetry collector.
pub fn init_tracing(config: &Config) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| config.log_level.clone().into());

    #[cfg(feature = "otel")]
    if let Some(ref endpoint) = config.otel_endpoint {
        use opentelemetry::KeyValue;
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_otlp::WithExportConfig;
        use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .expect("Failed to build OTLP span exporter");

        let resource = Resource::builder()
            .with_attribute(KeyValue::new("service.name", config.otel_service_name.clone()))
            .build();

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build();

        opentelemetry::global::set_tracer_provider(provider.clone());

        // Store for graceful shutdown on server exit.
        let _ = TRACER_PROVIDER.set(provider.clone());

        // Create an otel layer per format branch to avoid type-inference conflicts
        // between JsonFields and DefaultFields formatter types.
        match config.log_format {
            LogFormat::Json => {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt::layer().json())
                    .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("mq-web-api")))
                    .init();
            }
            LogFormat::Text => {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt::layer())
                    .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("mq-web-api")))
                    .init();
            }
        }
        return;
    }

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
    let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit.clone()));
    info!("Rate limiter initialized successfully");

    let app = create_router(&config, rate_limiter.clone()).layer(TraceLayer::new_for_http().on_response(
        |response: &axum::response::Response, latency: Duration, _span: &tracing::Span| {
            let ms = latency.as_secs_f64() * 1000.0;
            info!("response latency: {:.2}ms, status: {}", ms, response.status());
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
    info!("  RATE_LIMIT_REQUESTS_PER_WINDOW: Requests per window (default: 100)");
    info!("  RATE_LIMIT_WINDOW_SIZE_SECONDS: Window size in seconds (default: 3600)");
    info!("  RATE_LIMIT_CLEANUP_INTERVAL_SECONDS: Cleanup interval in seconds (default: 3600)");

    // Start cleanup service
    let mut cleanup_service = CleanupService::new(
        Arc::clone(&rate_limiter),
        config.rate_limit.cleanup_interval_seconds as u64,
    );
    cleanup_service.start();

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Failed to start server");

    info!("Shutting down mq-web-api server");

    #[cfg(feature = "otel")]
    if let Some(provider) = TRACER_PROVIDER.get() {
        provider.shutdown().ok();
    }

    Ok(())
}
