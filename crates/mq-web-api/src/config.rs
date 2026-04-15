use crate::rate_limiter::RateLimitConfig;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub log_format: LogFormat,
    pub cors_origins: Vec<String>,
    pub rate_limit: RateLimitConfig,
    /// OTLP exporter endpoint (e.g. `http://localhost:4317`). Requires the `otel` feature.
    pub otel_endpoint: Option<String>,
    /// Service name reported to the OpenTelemetry collector.
    pub otel_service_name: String,
}

#[derive(Debug, Clone)]
pub enum LogFormat {
    Text,
    Json,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "mq_web_api=debug,tower_http=debug".to_string(),
            log_format: LogFormat::Json,
            cors_origins: vec!["*".to_string()],
            rate_limit: RateLimitConfig::default(),
            otel_endpoint: None,
            otel_service_name: "mq-web-api".to_string(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(host) = env::var("HOST") {
            config.host = host;
        }

        if let Ok(port_str) = env::var("PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                config.port = port;
            } else {
                eprintln!(
                    "Warning: Invalid PORT value '{}', using default {}",
                    port_str, config.port
                );
            }
        }

        if let Ok(log_level) = env::var("RUST_LOG") {
            config.log_level = log_level;
        }

        if let Ok(log_format) = env::var("LOG_FORMAT") {
            config.log_format = match log_format.to_lowercase().as_str() {
                "text" | "plain" => LogFormat::Text,
                "json" => LogFormat::Json,
                _ => {
                    eprintln!(
                        "Warning: Invalid MQ_LOG_FORMAT value '{}', using default JSON",
                        log_format
                    );
                    LogFormat::Json
                }
            };
        }

        if let Ok(cors_origins) = env::var("CORS_ORIGINS") {
            config.cors_origins = cors_origins
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        // Rate limiting configuration
        if let Ok(requests_str) = env::var("RATE_LIMIT_REQUESTS_PER_WINDOW") {
            if let Ok(requests) = requests_str.parse::<i64>() {
                config.rate_limit.requests_per_window = requests;
            } else {
                eprintln!(
                    "Warning: Invalid RATE_LIMIT_REQUESTS_PER_WINDOW value '{}', using default {}",
                    requests_str, config.rate_limit.requests_per_window
                );
            }
        }

        if let Ok(window_str) = env::var("RATE_LIMIT_WINDOW_SIZE_SECONDS") {
            if let Ok(window) = window_str.parse::<i64>() {
                config.rate_limit.window_size_seconds = window;
            } else {
                eprintln!(
                    "Warning: Invalid RATE_LIMIT_WINDOW_SIZE_SECONDS value '{}', using default {}",
                    window_str, config.rate_limit.window_size_seconds
                );
            }
        }

        if let Ok(cleanup_str) = env::var("RATE_LIMIT_CLEANUP_INTERVAL_SECONDS") {
            if let Ok(cleanup) = cleanup_str.parse::<i64>() {
                config.rate_limit.cleanup_interval_seconds = cleanup;
            } else {
                eprintln!(
                    "Warning: Invalid RATE_LIMIT_CLEANUP_INTERVAL_SECONDS value '{}', using default {}",
                    cleanup_str, config.rate_limit.cleanup_interval_seconds
                );
            }
        }

        if let Ok(endpoint) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            && !endpoint.is_empty()
        {
            config.otel_endpoint = Some(endpoint);
        }

        if let Ok(service_name) = env::var("OTEL_SERVICE_NAME")
            && !service_name.is_empty()
        {
            config.otel_service_name = service_name;
        }

        config
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn server_url(&self) -> String {
        if self.port == 80 {
            format!("http://{}", self.host)
        } else if self.port == 443 {
            format!("https://{}", self.host)
        } else {
            format!("http://{}:{}", self.host, self.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_address() {
        let config = Config {
            host: "localhost".to_string(),
            port: 3000,
            ..Default::default()
        };
        assert_eq!(config.bind_address(), "localhost:3000");
    }

    #[test]
    fn test_server_url() {
        let config = Config {
            host: "example.com".to_string(),
            port: 8080,
            ..Default::default()
        };
        assert_eq!(config.server_url(), "http://example.com:8080");

        let config_80 = Config {
            host: "example.com".to_string(),
            port: 80,
            ..Default::default()
        };
        assert_eq!(config_80.server_url(), "http://example.com");

        let config_443 = Config {
            host: "example.com".to_string(),
            port: 443,
            ..Default::default()
        };
        assert_eq!(config_443.server_url(), "https://example.com");
    }
}
