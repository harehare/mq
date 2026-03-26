use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chromiumoxide::cdp::browser_protocol::page::EventLifecycleEvent;
use futures::StreamExt;
use reqwest::Client as ReqwestClient;
use url::Url;

/// Wait strategy configuration for headless Chrome.
///
/// These strategies are applied after the browser's `load` event fires.
/// Multiple strategies can be combined; they are executed in order:
/// network-idle / selector wait first, then the fixed delay.
#[derive(Debug, Clone, Default)]
pub struct ChromiumWaitConfig {
    /// Optional fixed delay applied after all other strategies complete.
    pub fixed_delay: Duration,
    /// If set, poll for this CSS selector to appear in the DOM before
    /// proceeding. Times out after `strategy_timeout`.
    pub wait_for_selector: Option<String>,
    /// If `true`, wait for the browser's `networkIdle` CDP lifecycle event
    /// before proceeding. Times out after `strategy_timeout`.
    pub network_idle: bool,
    /// Maximum time to wait for `wait_for_selector` or `network_idle`.
    /// Defaults to 30 seconds.
    pub strategy_timeout: Duration,
}

/// Enum for different HTTP client implementations
#[derive(Debug, Clone)]
pub enum HttpClient {
    Reqwest(ReqwestClient),
    Fantoccini(fantoccini::Client),
    /// Headless Chrome via CDP.
    /// `new_page()` waits for the `load` event, which covers synchronous JS
    /// execution. Additional wait strategies in [`ChromiumWaitConfig`] can be
    /// used to handle SPAs that fetch data asynchronously after load.
    Chromium(
        Arc<chromiumoxide::Browser>,
        ChromiumWaitConfig,
        Option<Arc<tempfile::TempDir>>,
    ),
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
    /// synchronous JavaScript execution. Additional wait strategies in
    /// [`ChromiumWaitConfig`] (network-idle, CSS selector polling, fixed delay)
    /// can be layered on top for SPAs that fetch data asynchronously after load.
    pub async fn new_chromium(chrome_path: Option<PathBuf>, wait_config: ChromiumWaitConfig) -> Result<Self, String> {
        let mut config_builder = chromiumoxide::browser::BrowserConfig::builder().arg("--disable-gpu");

        if let Some(path) = chrome_path {
            config_builder = config_builder.chrome_executable(path);
        }

        let temp_dir = tempfile::tempdir().map_err(|e| format!("Failed to create temporary directory: {}", e))?;
        config_builder = config_builder.user_data_dir(temp_dir.path());

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

        Ok(Self::Chromium(Arc::new(browser), wait_config, Some(Arc::new(temp_dir))))
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
            HttpClient::Chromium(browser, config, _) => {
                // new_page() waits for the browser's `load` event, which covers
                // synchronous JS. For SPAs that fetch data asynchronously after
                // load, additional strategies in `config` are applied below.
                let page = browser
                    .new_page(url.as_str())
                    .await
                    .map_err(|e| format!("Chrome failed to open page {}: {}", url, e))?;

                // Strategy 1: wait for the CDP networkIdle lifecycle event.
                // This is effective for SPAs that issue XHR/fetch requests after
                // the load event. The listener is registered after load, so it
                // catches the next networkIdle that follows the SPA's data fetching.
                if config.network_idle {
                    let timeout = if config.strategy_timeout.is_zero() {
                        Duration::from_secs(30)
                    } else {
                        config.strategy_timeout
                    };

                    match page.event_listener::<EventLifecycleEvent>().await {
                        Ok(mut events) => {
                            let _ = tokio::time::timeout(timeout, async {
                                while let Some(event) = events.next().await {
                                    if event.name == "networkIdle" {
                                        break;
                                    }
                                }
                            })
                            .await;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to register networkIdle listener for {}: {}", url, e);
                        }
                    }
                }

                // Strategy 2: poll until a CSS selector appears in the DOM.
                // Useful when you know a specific element that the SPA renders
                // once its content is ready (e.g. `--headless-wait-for-selector "main"`).
                if let Some(selector) = &config.wait_for_selector {
                    let timeout = if config.strategy_timeout.is_zero() {
                        Duration::from_secs(30)
                    } else {
                        config.strategy_timeout
                    };
                    let deadline = tokio::time::Instant::now() + timeout;
                    loop {
                        match page.find_element(selector.clone()).await {
                            Ok(_) => break,
                            Err(_) => {
                                if tokio::time::Instant::now() >= deadline {
                                    tracing::warn!("Timed out waiting for selector '{}' on {}", selector, url);
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(200)).await;
                            }
                        }
                    }
                }

                // Strategy 3: fixed delay — applied on top of other strategies.
                if !config.fixed_delay.is_zero() {
                    tokio::time::sleep(config.fixed_delay).await;
                }

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
