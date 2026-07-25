---
paths: crates/mq-crawler/**
---

# mq-crawler Rules

## Purpose

Web crawler that fetches pages over HTTP(S), converts HTML to Markdown, and collects the
results for batch processing with mq. It is not a filesystem/directory crawler.

## Coding Rules

- Respect `robots.txt` directives before fetching a URL
- Support seeding crawls from `sitemap.xml` in addition to a single start URL
- Limit crawl depth, breadth, and total page count (e.g. `--max-pages`) appropriately
- Support concurrent/parallel fetching with configurable concurrency limits
- Apply rate limiting / politeness delays between requests to the same host
- Retry failed requests with exponential backoff
- Support custom HTTP headers, cookies, user agents, and basic/bearer authentication
- Handle HTTP and network errors gracefully using `miette`
- Convert fetched HTML to Markdown via `mq-markdown`
- Track crawl statistics (pages crawled/skipped/failed, links discovered, duration)
- Support checkpointing crawl progress to disk and resuming via `--resume-from`
- Write tests for various HTML structures, robots.txt rules, and sitemap formats
- Document all CLI flags and configuration options
- Test edge cases: unreachable hosts, malformed HTML, redirects, disallowed paths, empty sitemaps
