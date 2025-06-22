# Web crawler

mq-crawler is a web crawler that fetches HTML content from websites, converts it to Markdown format, and processes it with mq queries. It's distributed as the `mqcr` binary.

## Key Features

- **HTML to Markdown conversion**: Automatically converts crawled HTML pages to clean Markdown
- **robots.txt compliance**: Respects robots.txt rules for ethical web crawling
- **mq-lang integration**: Processes content with mq-lang queries for filtering and transformation
- **Configurable crawling**: Customizable delays, domain restrictions, and link discovery
- **Flexible output**: Save to files or output to stdout

## Usage

```bash
mqcr [OPTIONS] <URL>
```

### Options

- `-o, --output <OUTPUT>`: Directory to save markdown files (stdout if not specified)
- `-c, --crawl-delay <CRAWL_DELAY>`: Delay between requests in seconds (default: 1)
- `--robots-path <ROBOTS_PATH>`: Custom robots.txt URL
- `-m, --mq-query <MQ_QUERY>`: mq-lang query for processing content (default: `identity()`)

### Examples

```bash
# Basic crawling to stdout
mqcr https://example.com

# Save to directory with custom delay
mqcr -o ./output -c 2 https://example.com

# Process with mq-lang query
mqcr -m '.h | select(contains("News"))' https://example.com
```
