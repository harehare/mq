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
    #[clap(short = 'U', long, value_name = "WEBDRIVER_URL")]
    webdriver_url: Option<Url>,
    /// Output format for results and statistics
    #[clap(short = 'f', long, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    #[clap(flatten)]
    pub conversion: ConversionArgs,
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
    tracing_subscriber::fmt::init();
    let args = CliArgs::parse();

    tracing::info!("Initializing crawler for URL: {}", args.url);

    let client = if let Some(url) = args.webdriver_url {
        mq_crawler::http_client::HttpClient::Fantoccini({
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
        })
    } else {
        mq_crawler::http_client::HttpClient::new_reqwest(args.page_load_timeout).unwrap()
    };

    let format = match args.format {
        OutputFormat::Text => mq_crawler::crawler::OutputFormat::Text,
        OutputFormat::Json => mq_crawler::crawler::OutputFormat::Json,
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
