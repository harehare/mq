use clap::Parser;
use mq_crawler::crawler::Crawler;
use url::Url;

/// A simple web crawler that fetches HTML, converts it to Markdown,
/// and optionally processes it with an mq_lang script.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct CliArgs {
    /// Optional path to an output DIRECTORY where markdown files will be saved.
    /// If not provided, output is printed to stdout.
    #[clap(short, long)]
    output: Option<String>,
    /// Delay (in seconds) between crawl requests to avoid overloading servers.
    #[clap(short, long, default_value_t = 1.0)]
    crawl_delay: f64,
    /// Optional path to a custom robots.txt file. If not provided, robots.txt will be fetched from the site.
    #[clap(long)]
    robots_path: Option<String>,
    /// Optional mq_lang query to process the crawled Markdown content.
    #[clap(short, long)]
    mq_query: Option<String>,
    /// The initial URL to start crawling from.
    #[clap(required = true)]
    url: Url,
    /// Optional WebDriver URL for browser-based crawling (e.g., http://localhost:4444).
    #[clap(short = 'U', long, value_name = "WEBDRIVER_URL")]
    webdriver_url: Option<Url>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = CliArgs::parse();

    tracing::info!("Initializing crawler for URL: {}", args.url);

    match Crawler::new(
        if let Some(url) = args.webdriver_url {
            mq_crawler::http_client::HttpClient::Fantoccini(
                fantoccini::ClientBuilder::native()
                    .connect(url.as_ref())
                    .await
                    .expect("Failed to connect to WebDriver"),
            )
        } else {
            mq_crawler::http_client::HttpClient::new_reqwest().unwrap()
        },
        args.url.clone(),
        args.crawl_delay,
        args.robots_path.clone(),
        args.mq_query.clone(),
        args.output,
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
