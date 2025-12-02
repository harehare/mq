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
//! # Configuration
//!
//! Configure the server using environment variables:
//!
//! - `PORT` - Server port (default: 8080)
//! - `HOST` - Bind address (default: 0.0.0.0)
//! - `RATE_LIMIT_REQUESTS` - Max requests per window
//! - `RATE_LIMIT_WINDOW` - Time window in seconds
//!
pub mod api;
pub mod cleanup;
pub mod config;
pub mod handlers;
pub mod middleware;
pub mod problem;
pub mod rate_limiter;
pub mod routes;
pub mod server;

pub use api::{ApiRequest, InputFormat, query};
pub use cleanup::CleanupService;
pub use config::Config;
pub use rate_limiter::{RateLimitConfig, RateLimitError, RateLimiter};
