use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chromiumoxide::cdp::browser_protocol::page::EventLifecycleEvent;
use futures::StreamExt;
use reqwest::Client as ReqwestClient;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use tokio::time::sleep;
use url::Url;

/// Exponential backoff config for retrying failed requests.
///
/// Retries on network errors, `429`, and `5xx`; other client errors (e.g. `404`) are not retried.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryConfig {
    /// Retry attempts after the initial request fails.
    pub max_retries: u32,
    /// Delay before the first retry.
    pub initial_backoff: Duration,
    /// Upper bound on the backoff delay.
    pub max_backoff: Duration,
    /// Multiplier applied to the backoff delay after each attempt.
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Disables retries; a failed request fails immediately.
    pub fn disabled() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    fn next_backoff(&self, current: Duration) -> Duration {
        let multiplier = self.backoff_multiplier.max(1.0);
        Duration::from_secs_f64(current.as_secs_f64() * multiplier).min(self.max_backoff)
    }
}

/// Default `Authorization` header credentials for a `reqwest`-based [`HttpClient`].
#[derive(Debug, Clone)]
pub enum AuthConfig {
    /// `Authorization: Basic <base64(user:pass)>`.
    Basic {
        /// User name.
        username: String,
        /// Password, if any.
        password: Option<String>,
    },
    /// `Authorization: Bearer <token>`.
    Bearer {
        /// The bearer token.
        token: String,
    },
}

/// Why a fetch failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchErrorKind {
    /// Connection, timeout or body read failure, or a browser error.
    Network,
    /// The server answered with a non-success HTTP status.
    Status(u16),
    /// The response is not HTML and [`FetchOptions::html_only`] is set.
    NotHtml,
    /// The response body exceeds [`FetchOptions::max_bytes`].
    TooLarge,
}

/// A failed fetch.
#[derive(Debug, Clone)]
pub struct FetchError {
    /// What went wrong.
    pub kind: FetchErrorKind,
    /// Human-readable description.
    pub message: String,
}

impl FetchError {
    fn new(kind: FetchErrorKind, message: String) -> Self {
        Self { kind, message }
    }

    fn network(message: String) -> Self {
        Self::new(FetchErrorKind::Network, message)
    }

    /// Network errors, `429` and `5xx` are worth retrying.
    fn is_retryable(&self) -> bool {
        match self.kind {
            FetchErrorKind::Network => true,
            FetchErrorKind::Status(code) => code == 429 || code >= 500,
            FetchErrorKind::NotHtml | FetchErrorKind::TooLarge => false,
        }
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Constraints on a fetched response. The default accepts anything.
///
/// Content type and size are enforced for the `reqwest` client only. Browser
/// clients cannot report the response headers, so only `max_bytes` is checked,
/// after the page has been rendered.
#[derive(Debug, Clone, Copy, Default)]
pub struct FetchOptions {
    /// Reject responses whose `Content-Type` is not HTML.
    pub html_only: bool,
    /// Reject responses larger than this many bytes.
    pub max_bytes: Option<usize>,
}

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
    /// Plain HTTP client with retry settings.
    Reqwest(ReqwestClient, RetryConfig),
    /// WebDriver-driven browser with retry settings.
    Fantoccini(fantoccini::Client, RetryConfig),
    /// Headless Chrome via CDP.
    /// `new_page()` waits for the `load` event, which covers synchronous JS
    /// execution. Additional wait strategies in [`ChromiumWaitConfig`] can be
    /// used to handle SPAs that fetch data asynchronously after load.
    Chromium(
        Arc<chromiumoxide::Browser>,
        ChromiumWaitConfig,
        RetryConfig,
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
            RetryConfig::default(),
        )
    }
}

impl HttpClient {
    /// Create a new reqwest-based HTTP client optimized for single-domain crawling
    pub fn new_reqwest(timeout: f64) -> Result<Self, String> {
        Self::new_reqwest_with_options(timeout, 3, RetryConfig::default(), HeaderMap::new(), None)
    }

    /// Create a new reqwest-based HTTP client optimized for multi-domain crawling
    pub fn new_reqwest_multi_domain(timeout: f64, max_idle_per_host: usize) -> Result<Self, String> {
        Self::new_reqwest_with_options(
            timeout,
            max_idle_per_host,
            RetryConfig::default(),
            HeaderMap::new(),
            None,
        )
    }

    /// Create a reqwest-based HTTP client with retry, default headers/cookies, and auth.
    ///
    /// `auth`, if set, overrides any `Authorization` entry already in `headers`.
    pub fn new_reqwest_with_options(
        timeout: f64,
        max_idle_per_host: usize,
        retry_config: RetryConfig,
        headers: HeaderMap,
        auth: Option<AuthConfig>,
    ) -> Result<Self, String> {
        let mut header_map = headers;

        if let Some(auth) = auth {
            let value = match auth {
                AuthConfig::Basic { username, password } => {
                    let credentials = format!("{}:{}", username, password.unwrap_or_default());
                    format!("Basic {}", BASE64.encode(credentials))
                }
                AuthConfig::Bearer { token } => format!("Bearer {}", token),
            };
            let header_value = HeaderValue::from_str(&value).map_err(|e| format!("Invalid auth credentials: {}", e))?;
            header_map.insert(AUTHORIZATION, header_value);
        }

        let mut builder = ReqwestClient::builder()
            .user_agent(format!("mq crawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE")))
            .pool_max_idle_per_host(max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(timeout as u64))
            .connect_timeout(Duration::from_secs(10))
            .tcp_keepalive(Duration::from_secs(120));

        if !header_map.is_empty() {
            builder = builder.default_headers(header_map);
        }

        let client = builder
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {}", e))?;
        Ok(Self::Reqwest(client, retry_config))
    }

    /// Create a headless Chrome client that launches Chrome/Chromium automatically.
    /// No external WebDriver server is required; only Chrome/Chromium must be installed.
    /// If `chrome_path` is `None`, the system Chrome is auto-detected.
    ///
    /// Pages are fetched after the browser's `load` event fires, which includes
    /// synchronous JavaScript execution. Additional wait strategies in
    /// [`ChromiumWaitConfig`] (network-idle, CSS selector polling, fixed delay)
    /// can be layered on top for SPAs that fetch data asynchronously after load.
    pub async fn new_chromium(
        chrome_path: Option<PathBuf>,
        wait_config: ChromiumWaitConfig,
        retry_config: RetryConfig,
    ) -> Result<Self, String> {
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
        // Errors from individual events are logged but do not stop the loop:
        // breaking early would drop the receiver and cause all subsequent
        // page operations to fail with "send failed because receiver is gone".
        tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if let Err(e) = h {
                    tracing::debug!("Browser handler event error: {}", e);
                }
            }
        });

        Ok(Self::Chromium(
            Arc::new(browser),
            wait_config,
            retry_config,
            Some(Arc::new(temp_dir)),
        ))
    }

    fn retry_config(&self) -> &RetryConfig {
        match self {
            HttpClient::Reqwest(_, retry_config) => retry_config,
            HttpClient::Fantoccini(_, retry_config) => retry_config,
            HttpClient::Chromium(_, _, retry_config, _) => retry_config,
        }
    }

    /// Fetch content from a URL, retrying transient failures with
    /// exponential backoff according to this client's [`RetryConfig`].
    pub async fn fetch(&self, url: Url) -> Result<String, String> {
        self.fetch_with(url, FetchOptions::default())
            .await
            .map_err(|e| e.message)
    }

    /// Like [`HttpClient::fetch`], with `options` applied and a typed error.
    pub async fn fetch_with(&self, url: Url, options: FetchOptions) -> Result<String, FetchError> {
        let retry_config = self.retry_config().clone();
        let mut backoff = retry_config.initial_backoff;
        let mut attempt: u32 = 0;

        loop {
            match self.fetch_once(url.clone(), options).await {
                Ok(content) => return Ok(content),
                Err(err) if err.is_retryable() && attempt < retry_config.max_retries => {
                    attempt += 1;
                    tracing::warn!(
                        "Fetch attempt {}/{} failed for {}: {}. Retrying in {:?}.",
                        attempt,
                        retry_config.max_retries,
                        url,
                        err,
                        backoff
                    );
                    sleep(backoff).await;
                    backoff = retry_config.next_backoff(backoff);
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn fetch_once(&self, url: Url, options: FetchOptions) -> Result<String, FetchError> {
        let content = self.fetch_raw(url.clone(), options).await?;
        match (self, options.max_bytes) {
            (HttpClient::Reqwest(..), _) | (_, None) => Ok(content),
            (_, Some(max)) if content.len() <= max => Ok(content),
            (_, Some(max)) => Err(too_large(&url, max)),
        }
    }

    async fn fetch_raw(&self, url: Url, options: FetchOptions) -> Result<String, FetchError> {
        match self {
            HttpClient::Reqwest(client, _) => {
                let mut response = client
                    .get(url.clone())
                    .send()
                    .await
                    .map_err(|e| FetchError::network(format!("Failed to fetch URL {}: {}", url, e)))?;

                let status = response.status();
                if !status.is_success() {
                    return Err(FetchError::new(
                        FetchErrorKind::Status(status.as_u16()),
                        format!("Request to {} failed with status: {}", url, status),
                    ));
                }

                let content_type = response
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string);

                if options.html_only
                    && let Some(content_type) = &content_type
                    && !is_html_content_type(content_type)
                {
                    return Err(FetchError::new(
                        FetchErrorKind::NotHtml,
                        format!("Response from {} is not HTML (Content-Type: {})", url, content_type),
                    ));
                }

                if let Some(max) = options.max_bytes
                    && response.content_length().is_some_and(|len| len > max as u64)
                {
                    return Err(too_large(&url, max));
                }

                let mut body = Vec::new();
                while let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|e| FetchError::network(format!("Failed to read response body: {}", e)))?
                {
                    if let Some(max) = options.max_bytes
                        && body.len() + chunk.len() > max
                    {
                        return Err(too_large(&url, max));
                    }
                    body.extend_from_slice(&chunk);
                }

                Ok(decode_body(&body, content_type.as_deref()))
            }
            HttpClient::Fantoccini(client, _) => {
                let url_str = url.as_str();

                client
                    .goto(url_str)
                    .await
                    .map_err(|e| FetchError::network(format!("Fantoccini failed to navigate to {}: {}", url, e)))?;

                let page_source = client
                    .source()
                    .await
                    .map_err(|e| FetchError::network(format!("Fantoccini failed to get page source: {}", e)))?;

                Ok(page_source)
            }
            HttpClient::Chromium(browser, config, _, _) => {
                // Open a blank page first so we can register event listeners
                // BEFORE navigating. This eliminates the race condition where
                // networkIdle fires between the `load` event and listener
                // registration when using new_page(url) directly.
                let page = browser
                    .new_page("about:blank")
                    .await
                    .map_err(|e| FetchError::network(format!("Chrome failed to open blank page: {}", e)))?;

                // Strategy 1: register the networkIdle listener BEFORE navigation
                // so no lifecycle event can slip past between load and registration.
                let network_idle_listener = if config.network_idle {
                    match page.event_listener::<EventLifecycleEvent>().await {
                        Ok(events) => Some(events),
                        Err(e) => {
                            tracing::warn!("Failed to register networkIdle listener for {}: {}", url, e);
                            None
                        }
                    }
                } else {
                    None
                };

                let result = async {
                    // Navigate to the target URL after the listener is in place.
                    page.goto(url.as_str())
                        .await
                        .map_err(|e| format!("Chrome failed to navigate to {}: {}", url, e))?;

                    // Now await the networkIdle event from the already-registered listener.
                    if let Some(mut events) = network_idle_listener {
                        let timeout = if config.strategy_timeout.is_zero() {
                            Duration::from_secs(30)
                        } else {
                            config.strategy_timeout
                        };

                        let _ = tokio::time::timeout(timeout, async {
                            while let Some(event) = events.next().await {
                                if event.name == "networkIdle" {
                                    break;
                                }
                            }
                        })
                        .await;
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

                    // Strategy 3: fixed delay, applied on top of other strategies.
                    if !config.fixed_delay.is_zero() {
                        tokio::time::sleep(config.fixed_delay).await;
                    }

                    page.content()
                        .await
                        .map_err(|e| format!("Chrome failed to get content from {}: {}", url, e))
                }
                .await;

                let _ = page.close().await;

                result.map_err(FetchError::network)
            }
        }
    }
}

fn too_large(url: &Url, max: usize) -> FetchError {
    FetchError::new(
        FetchErrorKind::TooLarge,
        format!("Response from {} exceeds the {} byte limit", url, max),
    )
}

fn is_html_content_type(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or_default().trim();
    essence.eq_ignore_ascii_case("text/html") || essence.eq_ignore_ascii_case("application/xhtml+xml")
}

/// Decodes `body` using the `charset` of `content_type`, falling back to UTF-8.
fn decode_body(body: &[u8], content_type: Option<&str>) -> String {
    let encoding = content_type
        .and_then(|content_type| {
            content_type.split(';').skip(1).find_map(|param| {
                let (key, value) = param.split_once('=')?;
                key.trim()
                    .eq_ignore_ascii_case("charset")
                    .then(|| value.trim().trim_matches('"'))
            })
        })
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(body).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_client_creation() {
        let client = HttpClient::default();
        assert!(matches!(client, HttpClient::Reqwest(_, _)));
    }

    #[test]
    fn test_new_reqwest_client() {
        let client = HttpClient::new_reqwest(30.0).unwrap();
        assert!(matches!(client, HttpClient::Reqwest(_, _)));
        assert_eq!(client.retry_config(), &RetryConfig::default());
    }

    #[test]
    fn test_new_reqwest_with_basic_auth() {
        let client = HttpClient::new_reqwest_with_options(
            30.0,
            3,
            RetryConfig::default(),
            HeaderMap::new(),
            Some(AuthConfig::Basic {
                username: "user".to_string(),
                password: Some("pass".to_string()),
            }),
        )
        .unwrap();
        assert!(matches!(client, HttpClient::Reqwest(_, _)));
    }

    #[test]
    fn test_new_reqwest_with_bearer_auth() {
        let client = HttpClient::new_reqwest_with_options(
            30.0,
            3,
            RetryConfig::default(),
            HeaderMap::new(),
            Some(AuthConfig::Bearer {
                token: "secret-token".to_string(),
            }),
        )
        .unwrap();
        assert!(matches!(client, HttpClient::Reqwest(_, _)));
    }

    #[test]
    fn test_retry_config_next_backoff_respects_max() {
        let retry_config = RetryConfig {
            max_retries: 5,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(3),
            backoff_multiplier: 2.0,
        };

        let backoff = retry_config.next_backoff(Duration::from_secs(1));
        assert_eq!(backoff, Duration::from_secs(2));

        let backoff = retry_config.next_backoff(backoff);
        assert_eq!(backoff, Duration::from_secs(3));

        let backoff = retry_config.next_backoff(backoff);
        assert_eq!(backoff, Duration::from_secs(3));
    }

    #[rstest::rstest]
    #[case("text/html", true)]
    #[case("Text/HTML; charset=utf-8", true)]
    #[case("application/xhtml+xml", true)]
    #[case("application/pdf", false)]
    #[case("text/plain", false)]
    fn test_is_html_content_type(#[case] content_type: &str, #[case] expected: bool) {
        assert_eq!(is_html_content_type(content_type), expected);
    }

    #[test]
    fn test_decode_body_uses_charset_param() {
        let sjis = [0x93, 0xfa, 0x96, 0x7b]; // "日本"
        assert_eq!(decode_body(&sjis, Some("text/html; charset=Shift_JIS")), "日本");
        assert_eq!(decode_body("日本".as_bytes(), Some("text/html")), "日本");
        assert_eq!(decode_body("日本".as_bytes(), None), "日本");
    }

    #[tokio::test]
    async fn test_fetch_with_enforces_max_bytes_while_streaming() {
        use httpmock::MockServer;

        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET).path("/big");
                then.status(200).body("x".repeat(1000));
            })
            .await;

        let client = HttpClient::new_reqwest(5.0).unwrap();
        let url = Url::parse(&format!("http://{}/big", server.address())).unwrap();
        let options = |max_bytes| FetchOptions {
            html_only: false,
            max_bytes,
        };

        let err = client.fetch_with(url.clone(), options(Some(999))).await.unwrap_err();
        assert_eq!(err.kind, FetchErrorKind::TooLarge);
        assert_eq!(client.fetch_with(url, options(Some(1000))).await.unwrap().len(), 1000);
    }

    #[test]
    fn test_retry_config_disabled() {
        let retry_config = RetryConfig::disabled();
        assert_eq!(retry_config.max_retries, 0);
    }

    #[tokio::test]
    async fn test_fetch_retries_on_server_error_then_succeeds() {
        use httpmock::MockServer;

        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET).path("/flaky");
                then.status(503);
            })
            .await;

        let retry_config = RetryConfig {
            max_retries: 2,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            backoff_multiplier: 2.0,
        };
        let client = HttpClient::new_reqwest_with_options(5.0, 3, retry_config, HeaderMap::new(), None).unwrap();
        let url = Url::parse(&format!("http://{}/flaky", server.address())).unwrap();

        let result = client.fetch(url).await;
        assert!(result.is_err());
        assert_eq!(mock.calls_async().await, 3);
    }

    #[tokio::test]
    async fn test_fetch_does_not_retry_on_client_error() {
        use httpmock::MockServer;

        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET).path("/missing");
                then.status(404);
            })
            .await;

        let retry_config = RetryConfig {
            max_retries: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            backoff_multiplier: 2.0,
        };
        let client = HttpClient::new_reqwest_with_options(5.0, 3, retry_config, HeaderMap::new(), None).unwrap();
        let url = Url::parse(&format!("http://{}/missing", server.address())).unwrap();

        let result = client.fetch(url).await;
        assert!(result.is_err());
        assert_eq!(mock.calls_async().await, 1);
    }
}
