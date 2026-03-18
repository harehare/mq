use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use reqwest::Client as ReqwestClient;
use url::Url;

/// Enum for different HTTP client implementations
#[derive(Debug, Clone)]
pub enum HttpClient {
    Reqwest(ReqwestClient),
    Fantoccini(fantoccini::Client),
    /// Headless Chrome via CDP.
    /// `new_page()` waits for the `load` event, which covers synchronous JS
    /// execution. The DOM captured by `page.content()` reflects the fully
    /// rendered state at that point.
    Chromium(Arc<chromiumoxide::Browser>),
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
    /// Create a new reqwest-based HTTP client optimized for single-domain crawling
    pub fn new_reqwest(timeout: f64) -> Result<Self, String> {
        let client = ReqwestClient::builder()
            .user_agent(format!("mq crawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE")))
            // Optimize for single-domain crawling
            .pool_max_idle_per_host(3)
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(timeout as u64))
            .connect_timeout(Duration::from_secs(10))
            .tcp_keepalive(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {}", e))?;
        Ok(Self::Reqwest(client))
    }

    /// Create a new reqwest-based HTTP client optimized for multi-domain crawling
    pub fn new_reqwest_multi_domain(timeout: f64, max_idle_per_host: usize) -> Result<Self, String> {
        let client = ReqwestClient::builder()
            .user_agent(format!("mq crawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE")))
            .pool_max_idle_per_host(max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(timeout as u64))
            .connect_timeout(Duration::from_secs(10))
            .tcp_keepalive(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {}", e))?;
        Ok(Self::Reqwest(client))
    }

    /// Create a headless Chrome client that launches Chrome/Chromium automatically.
    /// No external WebDriver server is required — only Chrome/Chromium must be installed.
    /// If `chrome_path` is `None`, the system Chrome is auto-detected.
    ///
    /// Pages are fetched after the browser's `load` event fires, which includes
    /// synchronous JavaScript execution. The captured DOM reflects the rendered
    /// state at that point, making this suitable for most JS-driven pages.
    pub async fn new_chromium(chrome_path: Option<PathBuf>) -> Result<Self, String> {
        let mut config_builder = chromiumoxide::browser::BrowserConfig::builder().arg("--disable-gpu");

        if let Some(path) = chrome_path {
            config_builder = config_builder.chrome_executable(path);
        }

        let config = config_builder
            .build()
            .map_err(|e| format!("Failed to build Chrome config: {}", e))?;

        let (browser, mut handler) = chromiumoxide::Browser::launch(config)
            .await
            .map_err(|e| format!("Failed to launch Chrome: {}", e))?;

        // Run the browser event loop in a background task.
        // Errors from individual events are logged but do not stop the loop —
        // breaking early would drop the receiver and cause all subsequent
        // page operations to fail with "send failed because receiver is gone".
        tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if let Err(e) = h {
                    tracing::debug!("Browser handler event error: {}", e);
                }
            }
        });

        Ok(Self::Chromium(Arc::new(browser)))
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
                    Err(format!("Request to {} failed with status: {}", url, response.status()))
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
            HttpClient::Chromium(browser) => {
                // new_page() navigates and waits for the load event, which includes
                // synchronous JS execution. The resulting DOM is the rendered state.
                let page = browser
                    .new_page(url.as_str())
                    .await
                    .map_err(|e| format!("Chrome failed to open page {}: {}", url, e))?;

                let content = page
                    .content()
                    .await
                    .map_err(|e| format!("Chrome failed to get content from {}: {}", url, e))?;

                page.close()
                    .await
                    .map_err(|e| format!("Chrome failed to close page: {}", e))?;

                Ok(content)
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
