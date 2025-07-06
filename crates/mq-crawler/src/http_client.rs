use std::time::Duration;

use reqwest::Client as ReqwestClient;
use url::Url;

/// Enum for different HTTP client implementations
#[derive(Debug, Clone)]
pub enum HttpClient {
    Reqwest(ReqwestClient),
    Fantoccini(fantoccini::Client),
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::Reqwest(
            ReqwestClient::builder()
                .user_agent(format!("mq crawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE")))
                .build()
                .expect("Failed to build default reqwest client"),
        )
    }
}

impl HttpClient {
    /// Create a new reqwest-based HTTP client
    pub fn new_reqwest(timeout: f64) -> Result<Self, String> {
        let client = ReqwestClient::builder()
            .user_agent(format!("mq crawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE")))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(timeout as u64))
            .connect_timeout(Duration::from_secs(10))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {}", e))?;
        Ok(Self::Reqwest(client))
    }

    /// Fetch content from a URL
    pub async fn fetch(&self, url: Url) -> Result<String, String> {
        match self {
            HttpClient::Reqwest(client) => {
                let response = client
                    .get(url.clone())
                    .send()
                    .await
                    .map_err(|e| format!("Failed to fetch URL {}: {}", url, e))?;

                if response.status().is_success() {
                    response
                        .text()
                        .await
                        .map_err(|e| format!("Failed to read response text: {}", e))
                } else {
                    Err(format!(
                        "Request to {} failed with status: {}",
                        url,
                        response.status()
                    ))
                }
            }
            HttpClient::Fantoccini(client) => {
                let url_str = url.as_str();

                client
                    .goto(url_str)
                    .await
                    .map_err(|e| format!("Fantoccini failed to navigate to {}: {}", url, e))?;

                let page_source = client
                    .source()
                    .await
                    .map_err(|e| format!("Fantoccini failed to get page source: {}", e))?;

                Ok(page_source)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_client_creation() {
        let client = HttpClient::default();
        assert!(matches!(client, HttpClient::Reqwest(_)));
    }

    #[test]
    fn test_new_reqwest_client() {
        let client = HttpClient::new_reqwest(30.0).unwrap();
        assert!(matches!(client, HttpClient::Reqwest(_)));
    }
}
