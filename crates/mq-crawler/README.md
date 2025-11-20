# mq-crawler

A web crawler and scraper that fetches HTML content, converts it to Markdown, and processes it with mq queries. Part of the [mq](https://mqlang.org) ecosystem.

## Features

- HTML to Markdown conversion: Automatically converts crawled HTML pages to clean Markdown format
- Robots.txt compliance: Respects robots.txt rules to crawl ethically
- mq integration: Process crawled content with mq queries for filtering and transformation
- Browser-based crawling: Uses WebDriver for JavaScript-heavy sites
- Configurable crawl delay: Avoid overloading servers with customizable delays between requests
- Parallel processing: Concurrent workers for faster crawling
- Output flexibility: Save to files or output to stdout
- Comprehensive statistics: Track crawling progress and results

## Installation

```sh
# Using Homebrew (macOS and Linux)
$ brew install harehare/tap/mqcr
```

## Usage

### Command Line Interface

The crawler is available as the `mqcr` binary:

```bash
# Basic crawling - output to stdout
mqcr https://example.com

# Save to directory
mqcr -o ./output https://example.com

# Custom crawl delay (default: 1 second)
mqcr -d 2.0 https://example.com

# Process with mq-lang query
mqcr -q '.h | select(contains("News"))' https://example.com

# Use custom robots.txt file
mqcr --robots-path ./custom-robots.txt https://example.com

# Parallel Crawling (Specifying Concurrency)
# Crawl with 3 concurrent workers
mqcr -c 3 https://example.com
```

### Command Line Options

```sh
A simple web crawler that fetches HTML, converts it to Markdown, and optionally processes it with an mq_lang script

Usage: mqcr [OPTIONS] <URL>

Arguments:
  <URL>  The initial URL to start crawling from

Options:
  -d, --crawl-delay <CRAWL_DELAY>
          Delay (in seconds) between crawl requests to avoid overloading servers [default: 0.5]
  -c, --concurrency <CONCURRENCY>
          Number of concurrent workers for parallel processing [default: 1]
      --implicit-timeout <IMPLICIT_TIMEOUT>
          Timeout (in seconds) for implicit waits (element finding) [default: 5]
  -m, --mq-query <MQ_QUERY>
          Optional mq_lang query to process the crawled Markdown content
      --page-load-timeout <PAGE_LOAD_TIMEOUT>
          Timeout (in seconds) for loading a single page [default: 30]
  -o, --output <OUTPUT>
          Optional path to an output DIRECTORY where markdown files will be saved. If not provided, output is printed to stdout
      --robots-path <ROBOTS_PATH>
          Optional path to a custom robots.txt file. If not provided, robots.txt will be fetched from the site
      --script-timeout <SCRIPT_TIMEOUT>
          Timeout (in seconds) for executing scripts on the page [default: 10]
  -U, --webdriver-url <WEBDRIVER_URL>
          Optional WebDriver URL for browser-based crawling (e.g., http://localhost:4444)
  -h, --help
          Print help
  -V, --version
          Print version
```

### Library Usage

```rust
use mq_crawler::crawler::Crawler;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = Url::parse("https://example.com")?;

    let mut crawler = Crawler::new(
        url,
        1.0,                    // crawl_delay
        None,                   // robots_path
        Some("select(.title)".to_string()), // mq_query
        Some("./output".to_string()), // output_dir
    ).await?;

    crawler.run().await?;
    Ok(())
}
```

## License

Licensed under the MIT License.
