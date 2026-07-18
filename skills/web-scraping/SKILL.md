---
name: web-scraping
description: Fetches web pages and extracts structured data using mq's toolchain (mq-crawl for fetching/crawling/JS rendering, mq for HTML-to-Markdown selector-based extraction, http() for in-query requests). Use when the user wants to scrape a URL, pull structured data out of a webpage, crawl a site, or turn HTML into Markdown/JSON — an alternative to a one-off curl-plus-parser script.
---

# Web Scraping with mq

Three tools — pick the smallest one that fits:

- **`mq-crawl`** — fetch/crawl a URL to Markdown, with JS rendering, multi-page crawling, robots.txt handling.
- **`mq`** — filter/transform Markdown or HTML (`-I html`) with jq-like selectors. Full selector/function reference: `processing-markdown` skill.
- **`mq`'s `http()` builtin** (`--allow-net`) — make the request *inside* a query, for cases `mq-crawl` can't (POST, custom headers/auth).

`mq-crawl` sends logs/stats to stderr — redirect with `2>/dev/null` when scripting.

## Fetch & Extract

```bash
mq-crawl --depth 0 https://example.com 2>/dev/null                       # page as Markdown (--depth 0 = don't follow links)
mq-crawl --depth 0 -q '.h | to_text()' https://example.com 2>/dev/null   # inline query, e.g. an outline
```

`-q` output is always Markdown/plain text, regardless of `-f` (which only formats the *stats* block on stderr, not content). For JSON/table/grep, skip `-q` and pipe the page into `mq` instead:

```bash
mq-crawl --depth 0 https://example.com 2>/dev/null | mq -F json 'select(.link)'                            # structured rows
mq-crawl --depth 0 https://example.com 2>/dev/null | mq -F json '.[][]'                                     # table cells
mq-crawl --depth 0 https://example.com 2>/dev/null | mq -F grep --context 2 'select(contains("Pricing"))'   # locate text
```

Filter with `select(...)` like jq: `select(.h.level <= 2)`, `select(.code.lang == "js")`, `select(contains("keyword"))`.

## JS-Rendered Pages & Multi-Page Crawls

```bash
mq-crawl --depth 0 --headless --headless-network-idle https://spa.example.com 2>/dev/null

mq-crawl https://docs.example.com --depth 2 --allowed-domains docs.example.com \
  --concurrency 4 -o ./crawled 2>/dev/null
```

- `--headless` needs local Chrome (or `--webdriver-url` for a remote WebDriver).
- `--allowed-domains` is an **exact-match** domain list, not a wildcard — list every subdomain you want crawled.
- `-o DIR` saves one Markdown file per page instead of stdout; aggregate them with `mq -I null --allow-read 'collection("./crawled") | map(self, fn(page): get(page, "title");)' /dev/null` (`collection` reads from disk, so it requires `--allow-read`).

## In-Query HTTP (`--allow-net`)

For POST/custom headers, or to keep fetch+extract in a single query:

```bash
mq --allow-net -I null 'http("get", "https://example.com")' /dev/null
mq --allow-net -I null 'http("post", "https://api.example.com/submit", "field=value")' /dev/null
```

`http(method, url, body?, headers?)` — `headers` is a dict, e.g. `set(dict(), "Authorization", "Bearer …")`. HTTPS-only; loopback/private/link-local addresses are always blocked. Errors unless `--allow-net` is passed. (`--allow-write` is the equivalent gate for `write_file()`.)

To extract from the response, pipe through `from_html()` — its result is a single `Array`, so selectors like `.link`/`.h` map element-wise and leave `null`s for non-matches instead of dropping them. Use `compact_map()` with an explicit-arg accessor (`get_url()`, `to_text()`, `attr()`) instead:

```bash
mq --allow-net -I null -F json 'http("get", "https://example.com") | from_html(self) | compact_map(self, fn(n): get_url(n);)' /dev/null
```

Prefer `mq-crawl` for plain page-reading (JS rendering, crawling, robots.txt come free); reach for `http()` only when `mq-crawl` can't make the request you need.

## Already Have HTML?

```bash
curl -s https://example.com | mq -I html '.h | to_text()'
curl -s https://example.com | mq -I html 'select(.link) | .link.url'
```

`-I html` converts to Markdown first — use Markdown selectors (`.link`, `.h`, `.text`), not HTML tags.

## Limitations

- No HTTP status/headers/timing from `mq`/`mq-crawl` — use `curl -sI` / `curl -w` for that.
- For JSON/XML APIs, skip `mq-crawl`: `curl -s <url> | mq -I json '...'`.
- Cap output size with `--limit N` / `--skip N`, or pipe through `head`.

Run `mq-crawl --help` / `mq --help` for full flag lists.
