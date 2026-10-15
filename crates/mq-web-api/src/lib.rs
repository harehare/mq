//! Web API server for the mq markdown processing service.
//!
//! This crate provides a REST API server that exposes mq functionality over HTTP,
//! allowing clients to process markdown, MDX, and HTML through HTTP requests.
//!
//! # Usage
//!
//! Start the web API server:
//!
//! ```rust,ignore
//! use mq_web_api::{Config, server};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Config::from_env()?;
//!     server::start(config).await?;
//!     Ok(())
//! }
//! ```
//!
//! # API Endpoints
//!
//! POST `/api/query` - Execute an mq query
//!
//! Request body:
//! ```json
//! {
//!   "code": ".h | select(level == 1)",
//!   "input": "# Hello\n## World",
//!   "input_format": "markdown"
//! }
//! ```
//!
//! Response:
//! ```json
//! {
//!   "results": ["# Hello"]
//! }
//! ```
//!
//! # Rate Limiting
//!
//! The API includes configurable rate limiting to prevent abuse:
//!
//! - Per-IP rate limits
//! - Configurable request windows
//! - Automatic cleanup of expired entries
//!
//! # Authentication
//!
//! Opt-in via `AUTH_ENABLED=true`, `API_KEYS`/`API_KEYS_FILE`, and `Authorization: Bearer <key>`.
//!
//! # Configuration
//!
//! Configure the server using environment variables:
//!
//! - `PORT` - Server port (default: 8080)
//! - `HOST` - Bind address (default: 0.0.0.0)
//! - `RATE_LIMIT_REQUESTS` - Max requests per window
//! - `RATE_LIMIT_WINDOW` - Time window in seconds
//! - `AUTH_ENABLED` - Require an API key (default: false)
//!
/// Query and format request types and execution.
pub mod api;
pub mod auth;
pub mod banner;
/// Periodic cleanup of rate limiter state.
pub mod cleanup;
/// Server configuration.
pub mod config;
/// HTTP request handlers.
pub mod handlers;
/// Authentication and rate limiting middleware.
pub mod middleware;
/// RFC 9457 problem details responses.
pub mod problem;
pub mod query_cache;
/// Request rate limiting.
pub mod rate_limiter;
/// Router construction.
pub mod routes;
/// Server startup and shutdown.
pub mod server;

pub use api::{ApiRequest, InputFormat, query};
pub use auth::{ApiKeyStore, Scope};
pub use cleanup::CleanupService;
pub use config::Config;
pub use query_cache::{QueryCache, QueryCacheConfig};
pub use rate_limiter::{RateLimitConfig, RateLimitError, RateLimiter};
