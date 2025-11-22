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

- 🌐 **Web Crawling**: Fetch and process web pages with configurable depth and delay
- 🔄 **HTML to Markdown**: Automatic conversion with customizable options
- 🤖 **Robots.txt Compliance**: Respects robots.txt rules for ethical crawling
- 🔍 **mq Query Integration**: Filter and transform crawled content on-the-fly
- 🚀 **Parallel Processing**: Concurrent workers for faster crawling
- 🎯 **Depth Control**: Limit crawl depth to control scope
- ⏱️ **Rate Limiting**: Configurable delays to avoid overloading servers
- 📊 **Statistics**: Track crawling progress and results
- 🌐 **WebDriver Support**: Use Selenium WebDriver for JavaScript-heavy sites

## Installation

### Homebrew

```sh
brew install harehare/tap/mqcr
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
mqcr https://example.com

# Save crawled content to directory
mqcr -o ./output https://example.com

# Crawl with custom delay (default: 0.5 seconds)
mqcr -d 2.0 https://example.com

# Limit crawl depth
mqcr --depth 2 https://example.com
```

### Processing with mq Queries

```bash
# Extract only headings from crawled pages
mqcr -m '.h | select(contains("News"))' https://example.com

# Extract all code blocks
mqcr -m '.code' https://developer.example.com

# Extract and transform links
mqcr -m '.link | to_text()' https://example.com
```

### Parallel Crawling

```bash
# Crawl with 3 concurrent workers
mqcr -c 3 https://example.com

# High-speed crawling with 10 workers
mqcr -c 10 -d 0.1 https://example.com
```

### Custom Robots.txt

```bash
# Use custom robots.txt file
mqcr --robots-path ./custom-robots.txt https://example.com
```

### HTML to Markdown Options

```bash
# Extract scripts as code blocks
mqcr --extract-scripts-as-code-blocks https://example.com

# Generate YAML front matter with metadata
mqcr --generate-front-matter https://example.com

# Use page title as H1 heading
mqcr --use-title-as-h1 https://example.com

# Combine multiple options
mqcr --generate-front-matter --use-title-as-h1 -o ./docs https://example.com
```

### Output Formats

```bash
# Output as JSON
mqcr --format json https://example.com

# Output as text (default)
mqcr --format text https://example.com
```

### Browser-Based Crawling

For JavaScript-heavy sites, use WebDriver (requires Selenium):

```bash
# Start Selenium server first
# docker run -d -p 4444:4444 selenium/standalone-chrome

# Crawl with WebDriver
mqcr -U http://localhost:4444 https://spa-example.com

# Custom timeouts
mqcr -U http://localhost:4444 \
  --page-load-timeout 60 \
  --script-timeout 30 \
  --implicit-timeout 10 \
  https://example.com
```

## Command Line Options

```sh
A simple web crawler that fetches HTML, converts it to Markdown, and optionally processes it with an mq query

Usage: mqcr [OPTIONS] <URL>

Arguments:
  <URL>  The initial URL to start crawling from

Options:
  -d, --crawl-delay <CRAWL_DELAY>
          Delay (in seconds) between crawl requests [default: 0.5]
  -c, --concurrency <CONCURRENCY>
          Number of concurrent workers [default: 1]
      --depth <DEPTH>
          Maximum crawl depth (0 = only start URL, 1 = start URL + direct links)
      --implicit-timeout <IMPLICIT_TIMEOUT>
          Timeout for element finding [default: 5]
  -m, --mq-query <MQ_QUERY>
          Optional mq query to process the crawled Markdown
      --page-load-timeout <PAGE_LOAD_TIMEOUT>
          Timeout for loading a page [default: 30]
  -o, --output <OUTPUT>
          Output directory for markdown files (stdout if not provided)
      --robots-path <ROBOTS_PATH>
          Path to custom robots.txt file
      --script-timeout <SCRIPT_TIMEOUT>
          Timeout for executing scripts [default: 10]
  -U, --webdriver-url <WEBDRIVER_URL>
          WebDriver URL for browser-based crawling (e.g., http://localhost:4444)
      --format <FORMAT>
          Output format: text or json [default: text]
      --extract-scripts-as-code-blocks
          Extract <script> tags as code blocks
      --generate-front-matter
          Generate YAML front matter with metadata
      --use-title-as-h1
          Use page title as H1 heading
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
