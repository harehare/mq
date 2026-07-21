use clap::Parser;
use fantoccini::wd::TimeoutConfiguration;
use mq_crawler::crawler::Crawler;
use url::Url;

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

/// A simple web crawler that fetches HTML, converts it to Markdown,
/// and optionally processes it with an mq_lang script.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct CliArgs {
    /// Delay (in seconds) between crawl requests to avoid overloading servers.
    #[clap(short = 'd', long, default_value_t = 1.0)]
    crawl_delay: f64,
    /// Number of concurrent workers for parallel processing.
    #[clap(short = 'c', long, default_value_t = 1)]
    concurrency: usize,
    /// Maximum crawl depth. 0 means only the specified URL, 1 means specified URL and its direct links, etc.
    /// If not specified, crawling depth is unlimited.
    #[clap(long)]
    depth: Option<usize>,
    /// Timeout (in seconds) for implicit waits (element finding).
    #[clap(long, default_value_t = 5.0)]
    implicit_timeout: f64,
    /// Optional mq_lang query to process the crawled Markdown content.
    #[clap(short = 'q', long)]
    mq_query: Option<String>,
    /// Timeout (in seconds) for loading a single page.
    #[clap(long, default_value_t = 30.0)]
    page_load_timeout: f64,
    /// Optional path to an output DIRECTORY where markdown files will be saved.
    /// If not provided, output is printed to stdout.
    #[clap(short, long)]
    output: Option<String>,
    /// Optional path to a custom robots.txt file. If not provided, robots.txt will be fetched from the site.
    #[clap(long)]
    robots_path: Option<String>,
    /// Timeout (in seconds) for executing scripts on the page.
    #[clap(long, default_value_t = 10.0)]
    script_timeout: f64,
    /// The initial URL to start crawling from.
    #[clap(required = true)]
    url: Url,
    /// Optional WebDriver URL for browser-based crawling (e.g., http://localhost:4444).
    /// When specified, uses a headless browser to render JavaScript before extracting content.
    #[clap(short = 'U', long, value_name = "WEBDRIVER_URL")]
    webdriver_url: Option<Url>,
    /// Use a built-in headless Chrome to render JavaScript without an external WebDriver server.
    /// Requires Chrome or Chromium to be installed on the system.
    /// Cannot be used together with --webdriver-url.
    #[clap(long, conflicts_with = "webdriver_url")]
    headless: bool,
    /// Path to the Chrome/Chromium executable for headless crawling.
    /// If not specified, Chrome is auto-detected from standard installation paths.
    /// Only used when --headless is set.
    #[clap(long, value_name = "PATH", requires = "headless")]
    chrome_path: Option<std::path::PathBuf>,
    /// Wait time (in seconds) after page load in headless mode.
    /// When --headless-network-idle or --headless-wait-for-selector is used,
    /// this value also acts as the maximum timeout for those strategies (default 30 s).
    /// Only used when --headless is set.
    #[clap(long, default_value_t = 0.0, requires = "headless")]
    headless_wait: f64,
    /// Wait for the browser's networkIdle CDP lifecycle event after page load.
    /// Effective for SPAs that issue XHR/fetch requests after the load event.
    /// The wait is bounded by --headless-wait (or 30 s if not set).
    /// Only used when --headless is set.
    #[clap(long, default_value_t = false, requires = "headless")]
    headless_network_idle: bool,
    /// Wait until the given CSS selector is present in the DOM after page load.
    /// Useful when the page's content is injected by JavaScript.
    /// Example: --headless-wait-for-selector "main"
    /// The wait is bounded by --headless-wait (or 30 s if not set).
    /// Only used when --headless is set.
    #[clap(long, value_name = "SELECTOR", requires = "headless")]
    headless_wait_for_selector: Option<String>,
    /// Comma-separated list of domains to crawl in addition to the start URL's domain.
    /// If not specified, only the start URL's domain is crawled.
    /// If specified, the start URL's domain is always included automatically.
    /// Example: --allowed-domains example.com,docs.example.com
    #[clap(long, value_delimiter = ',', value_name = "DOMAIN")]
    allowed_domains: Option<Vec<String>>,
    /// Output format for results and statistics
    #[clap(short = 'f', long, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    /// Optional URL of a sitemap.xml (or sitemap index) to enumerate additional seed URLs from.
    /// Discovered URLs are added to the crawl frontier alongside the start URL and are still
    /// subject to robots.txt, domain filtering, and depth limits.
    #[clap(long, value_name = "SITEMAP_URL")]
    sitemap: Option<Url>,
    /// Maximum number of retry attempts for a request that fails with a network error,
    /// a 429 (Too Many Requests), or a 5xx server error.
    #[clap(long, default_value_t = 3)]
    max_retries: u32,
    /// Delay (in seconds) before the first retry.
    #[clap(long, default_value_t = 0.5)]
    retry_initial_backoff: f64,
    /// Maximum delay (in seconds) between retries.
    #[clap(long, default_value_t = 10.0)]
    retry_max_backoff: f64,
    /// Multiplier applied to the retry delay after each failed attempt.
    #[clap(long, default_value_t = 2.0)]
    retry_backoff_multiplier: f64,
    /// Custom HTTP header to send with every request, in "Key: Value" form.
    /// Can be specified multiple times. Only applies to non-browser crawling.
    #[clap(long = "header", value_name = "KEY: VALUE")]
    headers: Vec<String>,
    /// Cookie to send with every request, in "name=value" form. Can be specified
    /// multiple times; all values are combined into a single Cookie header.
    /// Only applies to non-browser crawling.
    #[clap(long, value_name = "NAME=VALUE")]
    cookie: Vec<String>,
    /// HTTP Basic authentication credentials, in "username:password" form.
    /// Only applies to non-browser crawling.
    #[clap(long, value_name = "USER:PASS", conflicts_with = "bearer_token")]
    basic_auth: Option<String>,
    /// Bearer token sent as "Authorization: Bearer <token>".
    /// Only applies to non-browser crawling.
    #[clap(long, value_name = "TOKEN", conflicts_with = "basic_auth")]
    bearer_token: Option<String>,
    #[clap(flatten)]
    pub conversion: ConversionArgs,
}

/// Parses a "Key: Value" header string into a `(HeaderName, HeaderValue)` pair.
fn parse_header(raw: &str) -> Result<(reqwest::header::HeaderName, reqwest::header::HeaderValue), String> {
    let (name, value) = raw
        .split_once(':')
        .ok_or_else(|| format!("Invalid header '{}': expected 'Key: Value'", raw))?;
    let name = reqwest::header::HeaderName::from_bytes(name.trim().as_bytes())
        .map_err(|e| format!("Invalid header name in '{}': {}", raw, e))?;
    let value = reqwest::header::HeaderValue::from_str(value.trim())
        .map_err(|e| format!("Invalid header value in '{}': {}", raw, e))?;
    Ok((name, value))
}

/// Builds the default `HeaderMap` (custom headers plus a combined Cookie header)
/// applied to every request made by the reqwest-based HTTP client.
fn build_header_map(headers: &[String], cookies: &[String]) -> Result<reqwest::header::HeaderMap, String> {
    let mut header_map = reqwest::header::HeaderMap::new();

    for raw in headers {
        let (name, value) = parse_header(raw)?;
        header_map.insert(name, value);
    }

    if !cookies.is_empty() {
        let cookie_value = cookies.join("; ");
        let value = reqwest::header::HeaderValue::from_str(&cookie_value)
            .map_err(|e| format!("Invalid cookie value '{}': {}", cookie_value, e))?;
        header_map.insert(reqwest::header::COOKIE, value);
    }

    Ok(header_map)
}

/// Options for Markdown conversion.
#[derive(Debug, Clone, clap::Args)]
pub struct ConversionArgs {
    /// Extract <script> tags as code blocks in Markdown
    #[clap(
        long,
        help = "Extract <script> tags as code blocks in Markdown",
        default_value_t = false
    )]
    pub extract_scripts_as_code_blocks: bool,
    /// Generate YAML front matter from page metadata
    #[clap(
        long,
        help = "Generate YAML front matter from page metadata",
        default_value_t = false
    )]
    pub generate_front_matter: bool,
    /// Use the HTML <title> as the first H1 in Markdown
    #[clap(
        long,
        help = "Use the HTML <title> as the first H1 in Markdown",
        default_value_t = false
    )]
    pub use_title_as_h1: bool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();
    let args = CliArgs::parse();

    tracing::info!("Initializing crawler for URL: {}", args.url);

    // Build the effective allowed domains list.
    // When --allowed-domains is provided, always include the start URL's domain as well.
    let effective_allowed = args.allowed_domains.map(|v| {
        let mut v: Vec<String> = v.into_iter().map(|d| d.trim().to_lowercase()).collect();
        if let Some(start_domain) = args.url.domain() {
            let start_domain = start_domain.to_lowercase();
            if !v.contains(&start_domain) {
                v.push(start_domain);
            }
        }
        v
    });

    let retry_config = mq_crawler::http_client::RetryConfig {
        max_retries: args.max_retries,
        initial_backoff: std::time::Duration::from_secs_f64(args.retry_initial_backoff.max(0.0)),
        max_backoff: std::time::Duration::from_secs_f64(args.retry_max_backoff.max(0.0)),
        backoff_multiplier: args.retry_backoff_multiplier,
    };

    let header_map = match build_header_map(&args.headers, &args.cookie) {
        Ok(header_map) => header_map,
        Err(e) => {
            tracing::error!("{}", e);
            return;
        }
    };

    let auth = if let Some(ref basic) = args.basic_auth {
        let (username, password) = basic.split_once(':').unwrap_or((basic.as_str(), ""));
        Some(mq_crawler::http_client::AuthConfig::Basic {
            username: username.to_string(),
            password: if password.is_empty() {
                None
            } else {
                Some(password.to_string())
            },
        })
    } else {
        args.bearer_token
            .clone()
            .map(|token| mq_crawler::http_client::AuthConfig::Bearer { token })
    };

    if (args.webdriver_url.is_some() || args.headless) && (!header_map.is_empty() || auth.is_some()) {
        tracing::warn!(
            "--header, --cookie, --basic-auth, and --bearer-token only apply to non-browser crawling and will be ignored."
        );
    }

    let client = if let Some(url) = args.webdriver_url {
        mq_crawler::http_client::HttpClient::Fantoccini(
            {
                let fantoccini_client = fantoccini::ClientBuilder::rustls()
                    .expect("Failed to create rustls client builder")
                    .connect(url.as_ref())
                    .await
                    .expect("Failed to connect to WebDriver");

                fantoccini_client
                    .update_timeouts(TimeoutConfiguration::new(
                        Some(std::time::Duration::from_secs_f64(args.script_timeout)),
                        Some(std::time::Duration::from_secs_f64(args.page_load_timeout)),
                        Some(std::time::Duration::from_secs_f64(args.implicit_timeout)),
                    ))
                    .await
                    .expect("Failed to set timeouts on Fantoccini client");

                fantoccini_client
            },
            retry_config.clone(),
        )
    } else if args.headless {
        let headless_wait_secs = if !args.headless_wait.is_finite() || args.headless_wait < 0.0 {
            tracing::warn!(
                "Invalid value for --headless-wait ({}). Falling back to 0 seconds.",
                args.headless_wait
            );
            0.0
        } else {
            args.headless_wait
        };

        // strategy_timeout: use --headless-wait if > 0, otherwise 30 s.
        let strategy_timeout = if headless_wait_secs > 0.0 {
            std::time::Duration::from_secs_f64(headless_wait_secs)
        } else {
            std::time::Duration::from_secs(30)
        };
        // fixed_delay: only apply when no other strategy is active.
        let fixed_delay = if args.headless_network_idle || args.headless_wait_for_selector.is_some() {
            std::time::Duration::ZERO
        } else {
            std::time::Duration::from_secs_f64(headless_wait_secs)
        };

        let wait_config = mq_crawler::http_client::ChromiumWaitConfig {
            fixed_delay,
            wait_for_selector: args.headless_wait_for_selector.clone(),
            network_idle: args.headless_network_idle,
            strategy_timeout,
        };

        mq_crawler::http_client::HttpClient::new_chromium(args.chrome_path, wait_config, retry_config.clone())
            .await
            .expect("Failed to launch headless Chrome. Ensure Chrome or Chromium is installed.")
    } else if effective_allowed.is_some() {
        mq_crawler::http_client::HttpClient::new_reqwest_with_options(
            args.page_load_timeout,
            args.concurrency.max(5),
            retry_config,
            header_map,
            auth,
        )
        .unwrap()
    } else {
        mq_crawler::http_client::HttpClient::new_reqwest_with_options(
            args.page_load_timeout,
            3,
            retry_config,
            header_map,
            auth,
        )
        .unwrap()
    };

    let format = match args.format {
        OutputFormat::Text => mq_crawler::crawler::OutputFormat::Text,
        OutputFormat::Json => mq_crawler::crawler::OutputFormat::Json,
    };

    let sitemap_seed_urls = if let Some(ref sitemap_url) = args.sitemap {
        tracing::info!("Fetching seed URLs from sitemap: {}", sitemap_url);
        match mq_crawler::sitemap::fetch_sitemap_urls(&client, sitemap_url).await {
            Ok(urls) => {
                tracing::info!("Discovered {} seed URL(s) from sitemap.", urls.len());
                urls
            }
            Err(e) => {
                tracing::error!("Failed to fetch sitemap {}: {}", sitemap_url, e);
                return;
            }
        }
    } else {
        Vec::new()
    };

    match Crawler::new(
        client,
        args.url.clone(),
        args.crawl_delay,
        args.robots_path.clone(),
        args.mq_query.clone(),
        args.output,
        args.concurrency,
        format,
        mq_markdown::ConversionOptions {
            extract_scripts_as_code_blocks: args.conversion.extract_scripts_as_code_blocks,
            generate_front_matter: args.conversion.generate_front_matter,
            use_title_as_h1: args.conversion.use_title_as_h1,
        },
        args.depth,
        effective_allowed,
        sitemap_seed_urls,
    )
    .await
    {
        Ok(mut crawler) => {
            if let Err(e) = crawler.run().await {
                // robots_path no longer passed here
                tracing::error!("Crawler run failed: {}", e);
            } else {
                tracing::info!("Crawling complete.");
            }
        }
        Err(e) => {
            tracing::error!("Failed to initialize crawler: {}", e);
        }
    }
}
