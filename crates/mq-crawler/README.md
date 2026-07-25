<h1 align="center">mq-crawler</h1>

A web crawler that fetches HTML content, converts it to Markdown, and processes it with mq queries.

## Why mq-crawler?

Make web scraping and content extraction effortless with intelligent Markdown conversion:

- **HTML to Markdown**: Automatically convert crawled HTML pages to clean, structured Markdown
- **Ethical Crawling**: Built-in robots.txt compliance to crawl responsibly
- **mq Integration**: Process crawled content with powerful mq queries for filtering and transformation
- **JavaScript Support**: Browser-based crawling with WebDriver for dynamic content
- **High Performance**: Parallel processing with configurable concurrency for faster crawling
- **Flexible Output**: Save to files or stream to stdout

## Features

- **Web Crawling**: Fetch and process web pages with configurable depth and delay
- **HTML to Markdown**: Automatic conversion with customizable options
- **Robots.txt Compliance**: Respects robots.txt rules for ethical crawling
- **mq Query Integration**: Filter and transform crawled content on-the-fly
- **Parallel Processing**: Concurrent workers for faster crawling
- **Depth Control**: Limit crawl depth to control scope
- **Rate Limiting**: Configurable delays to avoid overloading servers
- **Statistics**: Track crawling progress and results
- **Headless Chrome**: Built-in headless Chrome for JavaScript-heavy sites (no external server needed)
- **WebDriver Support**: Use Selenium WebDriver for browser-based crawling
- **Domain Filtering**: Restrict crawling to specific domains
- **Sitemap Ingestion**: Seed the crawl frontier from a `sitemap.xml` (or sitemap index) up front
- **Max Pages Limit**: Cap the total number of pages visited to bound resource usage
- **Checkpoint & Resume**: Periodically snapshot crawl progress and resume an interrupted crawl later
- **Retry with Backoff**: Automatically retries failed requests (network errors, 429, 5xx) with exponential backoff
- **Custom Headers & Cookies**: Send custom HTTP headers and cookies with every request
- **Authentication**: Basic and bearer-token authentication for protected sites

## Installation

### Quick Install (Recommended)

```bash
curl -sSL https://mqlang.org/install_crawler.sh | bash
```

The installer will:
- Download the latest `mq-crawl` binary for your platform
- Install it to `~/.local/bin/`
- Verify the checksum of the downloaded binary
- Update your shell profile to add `mq-crawl` to your PATH

After installation, restart your terminal or source your shell profile, then verify:

```bash
mq-crawl --version
```

### Homebrew

```sh
brew install harehare/tap/mq-crawl
```

### Cargo

```sh
cargo install mq-crawler
```

### From Source

```sh
git clone https://github.com/harehare/mq
cd mq
cargo build --release -p mq-crawler
```

## Usage

### Basic Crawling

```bash
# Crawl a website and output to stdout
mq-crawl https://example.com

# Save crawled content to directory
mq-crawl -o ./output https://example.com

# Crawl with custom delay (default: 0.5 seconds)
mq-crawl -d 2.0 https://example.com

# Limit crawl depth
mq-crawl --depth 2 https://example.com
```

### Processing with mq Queries

```bash
# Extract only headings from crawled pages
mq-crawl -m '.h | select(contains("News"))' https://example.com

# Extract all code blocks
mq-crawl -m '.code' https://developer.example.com

# Extract and transform links
mq-crawl -m '.link | to_text' https://example.com
```

### Parallel Crawling

```bash
# Crawl with 3 concurrent workers
mq-crawl -c 3 https://example.com

# High-speed crawling with 10 workers
mq-crawl -c 10 -d 0.1 https://example.com
```

### Limiting Crawl Size

```bash
# Stop after visiting 500 pages, regardless of depth
mq-crawl --max-pages 500 https://example.com
```

### Checkpoint & Resume

```bash
# Save a checkpoint every 20 pages (default) to crawl-state.json
mq-crawl --checkpoint-path crawl-state.json https://example.com

# Save more frequently
mq-crawl --checkpoint-path crawl-state.json --checkpoint-interval-pages 5 https://example.com

# Resume an interrupted crawl from the last checkpoint
mq-crawl --resume-from crawl-state.json --checkpoint-path crawl-state.json https://example.com
```

A checkpoint is also written once the crawl stops for any reason (queue exhausted, `--max-pages` reached), so `--resume-from` reflects the true end state of the previous run.

### Sitemap Ingestion

```bash
# Seed the crawl frontier with every URL listed in a sitemap.xml,
# in addition to the start URL. Sitemap index files (<sitemapindex>)
# are followed recursively. Discovered URLs still respect robots.txt,
# --allowed-domains, and --depth.
mq-crawl --sitemap https://example.com/sitemap.xml https://example.com

# Useful with --depth 0 to crawl exactly the pages listed in the sitemap
# without following links at all.
mq-crawl --depth 0 --sitemap https://example.com/sitemap.xml https://example.com
```

### Retry & Backoff

```bash
# Retry failed requests (network errors, 429, 5xx) up to 5 times,
# starting at a 1s delay and doubling up to a 30s cap
mq-crawl --max-retries 5 --retry-initial-backoff 1 --retry-max-backoff 30 https://example.com

# Disable retries entirely
mq-crawl --max-retries 0 https://example.com
```

### Custom Headers, Cookies & Authentication

```bash
# Send a custom header with every request
mq-crawl --header "X-Api-Key: secret" https://example.com

# Send one or more cookies
mq-crawl --cookie "session=abc123" --cookie "theme=dark" https://example.com

# HTTP Basic authentication
mq-crawl --basic-auth alice:s3cret https://example.com

# Bearer token authentication
mq-crawl --bearer-token eyJhbGciOi... https://example.com
```

> **Note**: `--header`, `--cookie`, `--basic-auth`, and `--bearer-token` apply
> only to standard (non-browser) crawling; they are ignored with `--headless`
> or `-U/--webdriver-url`.

### Custom Robots.txt

```bash
# Use custom robots.txt file
mq-crawl --robots-path ./custom-robots.txt https://example.com
```

### HTML to Markdown Options

```bash
# Extract scripts as code blocks
mq-crawl --extract-scripts-as-code-blocks https://example.com

# Generate YAML front matter with metadata
mq-crawl --generate-front-matter https://example.com

# Use page title as H1 heading
mq-crawl --use-title-as-h1 https://example.com

# Combine multiple options
mq-crawl --generate-front-matter --use-title-as-h1 -o ./docs https://example.com
```

### Output Formats

```bash
# Output as JSON
mq-crawl --format json https://example.com

# Output as text (default)
mq-crawl --format text https://example.com
```

### Domain Filtering

```bash
# Crawl only the start URL's domain (default behavior)
mq-crawl https://example.com

# Also crawl docs.example.com and blog.example.com
# The start URL's domain (example.com) is always included automatically
mq-crawl --allowed-domains docs.example.com,blog.example.com https://example.com
```

### Browser-Based Crawling (Headless Chrome)

For JavaScript-heavy sites, use the built-in headless Chrome without an external server:

```bash
# Use built-in headless Chrome (Chrome or Chromium must be installed)
mq-crawl --headless https://spa-example.com

# Specify a custom Chrome/Chromium executable path
mq-crawl --headless --chrome-path /usr/bin/chromium https://spa-example.com
```

### Browser-Based Crawling (WebDriver)

Alternatively, use an external Selenium WebDriver server:

```bash
# Start Selenium server first
# docker run -d -p 4444:4444 selenium/standalone-chrome

# Crawl with WebDriver
mq-crawl -U http://localhost:4444 https://spa-example.com

# Custom timeouts
mq-crawl -U http://localhost:4444 \
  --page-load-timeout 60 \
  --script-timeout 30 \
  --implicit-timeout 10 \
  https://example.com
```

## Command Line Options

```sh
A simple web crawler that fetches HTML, converts it to Markdown, and optionally processes it with an mq query

Usage: mq-crawl [OPTIONS] <URL>

Arguments:
  <URL>  The initial URL to start crawling from

Options:
  -d, --crawl-delay <CRAWL_DELAY>
          Delay (in seconds) between crawl requests to avoid overloading servers [default: 1]
  -c, --concurrency <CONCURRENCY>
          Number of concurrent workers for parallel processing [default: 1]
      --depth <DEPTH>
          Maximum crawl depth. 0 means only the specified URL, 1 means specified URL and its direct links, etc. If not specified, crawling depth is unlimited
      --max-pages <MAX_PAGES>
          Maximum total number of pages to visit (queued, in-flight, or crawled) before stopping. If not specified, crawling continues until the frontier is exhausted or --depth is reached
      --checkpoint-path <PATH>
          Path to write periodic crawl checkpoints to (JSON). Enables resuming an interrupted crawl later with --resume-from. A checkpoint is written every --checkpoint-interval-pages pages, and once more when the crawl stops
      --checkpoint-interval-pages <CHECKPOINT_INTERVAL_PAGES>
          Number of successfully crawled pages between checkpoint saves. Only used when --checkpoint-path is set [default: 20]
      --resume-from <PATH>
          Resume a previous crawl from a checkpoint file written via --checkpoint-path. The visited set and pending frontier are restored; the URL argument is only used to determine crawl configuration such as the allowed domain
      --implicit-timeout <IMPLICIT_TIMEOUT>
          Timeout (in seconds) for implicit waits (element finding) [default: 5]
  -q, --mq-query <MQ_QUERY>
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
          Optional WebDriver URL for browser-based crawling (e.g., http://localhost:4444). When specified, uses a headless browser to render JavaScript before extracting content
      --headless
          Use a built-in headless Chrome to render JavaScript without an external WebDriver server. Requires Chrome or Chromium to be installed on the system. Cannot be used together with --webdriver-url
      --chrome-path <PATH>
          Path to the Chrome/Chromium executable for headless crawling. If not specified, Chrome is auto-detected from standard installation paths. Only used when --headless is set
      --headless-wait <HEADLESS_WAIT>
          Wait time (in seconds) after page load in headless mode. When --headless-network-idle or --headless-wait-for-selector is used, this value also acts as the maximum timeout for those strategies (default 30 s). Only used when --headless is set [default: 0]
      --headless-network-idle
          Wait for the browser's networkIdle CDP lifecycle event after page load. Effective for SPAs that issue XHR/fetch requests after the load event. The wait is bounded by --headless-wait (or 30 s if not set). Only used when --headless is set
      --headless-wait-for-selector <SELECTOR>
          Wait until the given CSS selector is present in the DOM after page load. Useful when the page's content is injected by JavaScript. Example: --headless-wait-for-selector "main" The wait is bounded by --headless-wait (or 30 s if not set). Only used when --headless is set
      --allowed-domains <DOMAIN>
          Comma-separated list of domains to crawl in addition to the start URL's domain. If not specified, only the start URL's domain is crawled. If specified, the start URL's domain is always included automatically. Example: --allowed-domains example.com,docs.example.com
  -f, --format <FORMAT>
          Output format for results and statistics [default: text] [possible values: text, json]
      --sitemap <SITEMAP_URL>
          Optional URL of a sitemap.xml (or sitemap index) to enumerate additional seed URLs from. Discovered URLs are added to the crawl frontier alongside the start URL and are still subject to robots.txt, domain filtering, and depth limits
      --max-retries <MAX_RETRIES>
          Max retry attempts on network error, 429, or 5xx [default: 3]
      --retry-initial-backoff <RETRY_INITIAL_BACKOFF>
          Delay (seconds) before the first retry [default: 0.5]
      --retry-max-backoff <RETRY_MAX_BACKOFF>
          Max delay (seconds) between retries [default: 10]
      --retry-backoff-multiplier <RETRY_BACKOFF_MULTIPLIER>
          Retry delay multiplier per failed attempt [default: 2]
      --header <KEY: VALUE>
          Custom header ("Key: Value"), repeatable. Non-browser crawling only
      --cookie <NAME=VALUE>
          Cookie ("name=value"), repeatable, combined into one Cookie header. Non-browser crawling only
      --basic-auth <USER:PASS>
          HTTP Basic auth ("username:password"). Non-browser crawling only
      --bearer-token <TOKEN>
          Bearer token for "Authorization: Bearer <token>". Non-browser crawling only
      --extract-scripts-as-code-blocks
          Extract <script> tags as code blocks in Markdown
      --generate-front-matter
          Generate YAML front matter from page metadata
      --use-title-as-h1
          Use the HTML <title> as the first H1 in Markdown
  -h, --help
          Print help
  -V, --version
          Print version
```

## Development

### Building from Source

```sh
git clone https://github.com/harehare/mq
cd mq
cargo build --release -p mq-crawler
```

### Running Tests

```sh
cargo test -p mq-crawler
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
