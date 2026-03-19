# Web crawler

mq-crawler is a web crawler that fetches HTML content from websites, converts it to Markdown format, and processes it with mq queries.

## Key Features

- **HTML to Markdown conversion**: Automatically converts crawled HTML pages to clean Markdown
- **robots.txt compliance**: Respects robots.txt rules for ethical web crawling
- **mq-lang integration**: Processes content with mq-lang queries for filtering and transformation
- **Configurable crawling**: Customizable delays, domain restrictions, and link discovery
- **Flexible output**: Save to files or output to stdout
- **Headless Chrome**: Built-in headless Chrome for JavaScript-heavy sites (no external server needed)
- **WebDriver support**: Browser-based crawling via Selenium WebDriver
- **Domain filtering**: Restrict crawling to specific domains

## Installation

### Quick Install

```bash
curl -sSL https://mqlang.org/install_crawler.sh | bash
```

The installer will:
- Download the latest `mq-crawl` binary for your platform
- Install it to `~/.mq/bin/`
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

### Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases).

## Usage

```bash
mq-crawl [OPTIONS] <URL>
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `-o, --output <OUTPUT>` | Directory to save markdown files (stdout if not specified) | stdout |
| `-d, --crawl-delay <SECONDS>` | Delay between requests in seconds | `1` |
| `-c, --concurrency <N>` | Number of concurrent workers | `1` |
| `--depth <DEPTH>` | Maximum crawl depth (0 = start URL only) | unlimited |
| `-q, --mq-query <QUERY>` | mq-lang query for processing content | — |
| `--robots-path <PATH>` | Custom robots.txt file path | — |
| `--allowed-domains <DOMAINS>` | Comma-separated list of extra domains to crawl; the start URL's domain is always included | start domain only |
| `--headless` | Use built-in headless Chrome (Chrome/Chromium must be installed) | — |
| `--chrome-path <PATH>` | Path to Chrome/Chromium executable (requires `--headless`) | auto-detect |
| `-U, --webdriver-url <URL>` | External WebDriver URL for browser-based crawling | — |
| `--page-load-timeout <SECONDS>` | Timeout for loading a single page | `30` |
| `--script-timeout <SECONDS>` | Timeout for executing scripts on the page | `10` |
| `--implicit-timeout <SECONDS>` | Timeout for element finding | `5` |
| `--extract-scripts-as-code-blocks` | Extract `<script>` tags as code blocks | — |
| `--generate-front-matter` | Generate YAML front matter from page metadata | — |
| `--use-title-as-h1` | Use the HTML `<title>` as the first H1 heading | — |
| `-f, --format <FORMAT>` | Output format: `text` or `json` | `text` |

### Examples

```bash
# Basic crawling to stdout
mq-crawl https://example.com

# Save to directory with custom delay
mq-crawl -o ./output -d 2 https://example.com

# Limit crawl depth and use concurrent workers
mq-crawl --depth 2 -c 3 https://example.com

# Process with mq-lang query
mq-crawl -q '.h | select(contains("News"))' https://example.com

# Extract code blocks from a docs site
mq-crawl -q '.code' https://docs.example.com
```

### Domain Filtering

By default, only the start URL's domain is crawled. Use `--allowed-domains` to include additional domains:

```bash
# Also crawl docs.example.com and blog.example.com
# The start URL's domain is always included automatically
mq-crawl --allowed-domains docs.example.com,blog.example.com https://example.com
```

### Headless Chrome

For JavaScript-heavy sites, use the built-in headless Chrome without an external server:

```bash
# Use built-in headless Chrome (Chrome or Chromium must be installed)
mq-crawl --headless https://spa-example.com

# Specify a custom Chrome/Chromium executable path
mq-crawl --headless --chrome-path /usr/bin/chromium https://spa-example.com
```

### WebDriver

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
  https://spa-example.com
```

### HTML to Markdown Options

```bash
# Generate YAML front matter with metadata
mq-crawl --generate-front-matter https://example.com

# Use page title as H1 heading
mq-crawl --use-title-as-h1 https://example.com

# Extract <script> tags as code blocks
mq-crawl --extract-scripts-as-code-blocks https://example.com

# Combine options
mq-crawl --generate-front-matter --use-title-as-h1 -o ./docs https://example.com
```

### Output Formats

```bash
# Output as JSON
mq-crawl --format json https://example.com

# Output as plain text (default)
mq-crawl --format text https://example.com
```
