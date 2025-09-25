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
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(host) = env::var("MQ_HOST") {
            config.host = host;
        }

        if let Ok(port_str) = env::var("MQ_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                config.port = port;
            } else {
                eprintln!(
                    "Warning: Invalid MQ_PORT value '{}', using default {}",
                    port_str, config.port
                );
            }
        }

        if let Ok(log_level) = env::var("RUST_LOG") {
            config.log_level = log_level;
        } else if let Ok(log_level) = env::var("MQ_LOG_LEVEL") {
            config.log_level = log_level;
        }

        if let Ok(log_format) = env::var("MQ_LOG_FORMAT") {
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

        if let Ok(cors_origins) = env::var("MQ_CORS_ORIGINS") {
            config.cors_origins = cors_origins
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        // Rate limiting configuration
        if let Ok(database_url) = env::var("DATABASE_URL") {
            config.rate_limit.database_url = database_url;
        } else if let Ok(database_url) = env::var("MQ_RATE_LIMIT_DATABASE_URL") {
            config.rate_limit.database_url = database_url;
        }

        if let Ok(requests_str) = env::var("MQ_RATE_LIMIT_REQUESTS_PER_WINDOW") {
            if let Ok(requests) = requests_str.parse::<i64>() {
                config.rate_limit.requests_per_window = requests;
            } else {
                eprintln!(
                    "Warning: Invalid MQ_RATE_LIMIT_REQUESTS_PER_WINDOW value '{}', using default {}",
                    requests_str, config.rate_limit.requests_per_window
                );
            }
        }

        if let Ok(window_str) = env::var("MQ_RATE_LIMIT_WINDOW_SIZE_SECONDS") {
            if let Ok(window) = window_str.parse::<i64>() {
                config.rate_limit.window_size_seconds = window;
            } else {
                eprintln!(
                    "Warning: Invalid MQ_RATE_LIMIT_WINDOW_SIZE_SECONDS value '{}', using default {}",
                    window_str, config.rate_limit.window_size_seconds
                );
            }
        }

        if let Ok(cleanup_str) = env::var("MQ_RATE_LIMIT_CLEANUP_INTERVAL_SECONDS") {
            if let Ok(cleanup) = cleanup_str.parse::<i64>() {
                config.rate_limit.cleanup_interval_seconds = cleanup;
            } else {
                eprintln!(
                    "Warning: Invalid MQ_RATE_LIMIT_CLEANUP_INTERVAL_SECONDS value '{}', using default {}",
                    cleanup_str, config.rate_limit.cleanup_interval_seconds
                );
            }
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
    use std::env;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.log_level, "mq_web_api=debug,tower_http=debug");
        assert!(matches!(config.log_format, LogFormat::Json));
        assert_eq!(config.cors_origins, vec!["*"]);
    }

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

    #[test]
    fn test_config_from_env() {
        // Save original values
        let original_host = env::var("MQ_HOST").ok();
        let original_port = env::var("MQ_PORT").ok();
        let original_log = env::var("MQ_LOG_LEVEL").ok();
        let original_log_format = env::var("MQ_LOG_FORMAT").ok();
        let original_cors = env::var("MQ_CORS_ORIGINS").ok();

        unsafe {
            // Set test values
            env::set_var("MQ_HOST", "test.example.com");
            env::set_var("MQ_PORT", "9000");
            env::set_var("MQ_LOG_LEVEL", "info");
            env::set_var("MQ_LOG_FORMAT", "text");
            env::set_var("MQ_CORS_ORIGINS", "https://example.com,https://test.com");
        }

        let config = Config::from_env();

        assert_eq!(config.host, "test.example.com");
        assert_eq!(config.port, 9000);
        assert_eq!(config.log_level, "info");
        assert!(matches!(config.log_format, LogFormat::Text));
        assert_eq!(
            config.cors_origins,
            vec!["https://example.com", "https://test.com"]
        );

        unsafe {
            // Restore original values
            match original_host {
                Some(val) => env::set_var("MQ_HOST", val),
                None => env::remove_var("MQ_HOST"),
            }
            match original_port {
                Some(val) => env::set_var("MQ_PORT", val),
                None => env::remove_var("MQ_PORT"),
            }
            match original_log {
                Some(val) => env::set_var("MQ_LOG_LEVEL", val),
                None => env::remove_var("MQ_LOG_LEVEL"),
            }
            match original_log_format {
                Some(val) => env::set_var("MQ_LOG_FORMAT", val),
                None => env::remove_var("MQ_LOG_FORMAT"),
            }
            match original_cors {
                Some(val) => env::set_var("MQ_CORS_ORIGINS", val),
                None => env::remove_var("MQ_CORS_ORIGINS"),
            }
        }
    }
}
