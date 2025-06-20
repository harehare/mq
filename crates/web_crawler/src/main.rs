use clap::Parser;
use url::Url;
use web_crawler::crawler::Crawler; // Adjusted path

// Remove: use web_crawler::robots::RobotsTxt; // No longer directly used in main

/// A simple web crawler that fetches HTML, converts it to Markdown,
/// and optionally processes it with an mq_lang script.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct CliArgs {
    #[clap(required = true)]
    url: Url,
        /// Optional path to an output DIRECTORY where markdown files will be saved.
        /// If not provided, output is printed to stdout.
    #[clap(short, long)]
    output: Option<String>,
    #[clap(short, long, default_value_t = 1.0)]
    crawl_delay: f64,
    #[clap(long)]
    robots_path: Option<String>, // This will be passed to Crawler::new
    #[clap(short, long)]
    mq_script: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = CliArgs::parse();

    tracing::info!("Initializing crawler for URL: {}", args.url);

    match Crawler::new(
        args.url.clone(), // Pass the initial URL
        args.crawl_delay,
        args.robots_path.clone(), // Pass the custom robots path
        args.mq_script,
        args.output,
    ).await {
        Ok(mut crawler) => {
            if let Err(e) = crawler.run().await { // robots_path no longer passed here
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
