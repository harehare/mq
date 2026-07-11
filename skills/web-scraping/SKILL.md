---
name: web-scraping
description: Fetches web pages and extracts structured data from them using mq's toolchain (mq-crawl for fetching/crawling/JS rendering, mq for HTML-to-Markdown selector-based extraction). Use when the user wants to scrape a URL, pull structured data out of a webpage, crawl a site, or turn HTML into Markdown/JSON — an alternative to writing a one-off curl-plus-parser script.
---

# Web Scraping with mq

Replace the "curl → throwaway parser script" workflow with the mq ecosystem:

- **`mq-crawl`** — fetches a URL (optionally rendering JS via headless Chrome or a WebDriver), converts the page to Markdown, and can apply an mq query inline. Handles single pages (`--depth 0`) or full site crawls.
- **`mq`** — turns Markdown (or HTML via `-I html`, which is converted to Markdown internally) into filtered/structured output using jq-like selectors. See the `processing-markdown` skill for the full selector/function reference; this skill focuses on the fetch → extract pipeline.
- **`mq`'s `http()` builtin** (`--allow-net`) — makes the request from inside a query itself, for cases `mq-crawl` doesn't cover (POST, custom headers/auth). See "In-Query HTTP Requests" below.

Both tools always print machine content to **stdout**; `mq-crawl` sends progress logs and crawl statistics to **stderr** — redirect with `2>/dev/null` when scripting.

## Quick Fetch (single page → Markdown)

```bash
mq-crawl --depth 0 https://example.com 2>/dev/null
```

Output is `Filename: <slug>.md --` followed by the page rendered as Markdown. `--depth 0` means "only this URL, don't follow links."

## Fetch + Extract in One Step

Pass an mq query with `-q`. When `-q` is given, the "Filename: --" header is dropped and only the query result is printed — safe to pipe directly:

```bash
mq-crawl --depth 0 -q '.h | to_text()' https://example.com 2>/dev/null   # page outline (headings)
mq-crawl --depth 0 -q '.link.url' https://example.com 2>/dev/null        # all links
```

`-q` output is always rendered as Markdown/plain text, regardless of `-f`. `-f text|json` only controls the *crawl statistics* block on stderr, not the content — for JSON/table/grep output of the extracted content, skip `-q` and pipe the page's Markdown into `mq` instead:

```bash
mq-crawl --depth 0 https://example.com 2>/dev/null | mq -F json 'select(.link)'   # links as structured JSON
```

## Discovery (page structure, no raw HTML)

```bash
mq-crawl --depth 0 -q '.h' https://example.com 2>/dev/null       # heading tree, with levels (# ## ###)
mq-crawl --depth 0 -q '.list' https://example.com 2>/dev/null    # list structure
```

To find which part of the page contains specific text (locate), use grep-style output with context:

```bash
mq-crawl --depth 0 https://example.com 2>/dev/null | mq -F grep --context 2 'select(contains("Pricing"))'
```

## Structured Extraction (rows / tables)

For "one row per element, several fields" extraction, ask for the matching nodes as JSON — each node already carries its relevant fields (`url`/`title`/`value` for links, `checked`/`value` for list items, etc.):

```bash
mq-crawl --depth 0 https://example.com 2>/dev/null | mq -F json 'select(.link)'
# [{"type":"Link","url":"...","title":"...","values":[{"type":"Text","value":"..."}]}, ...]
```

For actual Markdown tables, select cells directly:

```bash
mq-crawl --depth 0 https://example.com 2>/dev/null | mq -F json '.[][]'    # table cells (row/column/value)
mq-crawl --depth 0 https://example.com 2>/dev/null | mq -F table '.[][]'   # rendered back as a table
```

Filter with `select(...)` the same way you'd filter jq output, e.g. `select(.h.level <= 2)`, `select(.code.lang == "js")`, `select(contains("keyword"))`.

## JavaScript-Rendered Pages

```bash
mq-crawl --depth 0 --headless --headless-network-idle https://spa-example.com 2>/dev/null
mq-crawl --depth 0 --headless --headless-wait-for-selector "main" https://spa-example.com 2>/dev/null
```

`--headless` needs a local Chrome/Chromium install (auto-detected, or pass `--chrome-path`). Use `--webdriver-url` instead if you have a remote WebDriver (e.g. Selenium) endpoint.

## Crawling Multiple Pages

```bash
mq-crawl https://docs.example.com \
  --depth 2 \
  --allowed-domains docs.example.com \
  --concurrency 4 \
  --crawl-delay 1 \
  -o ./crawled 2>/dev/null
```

- `--depth N` — 0 = start URL only, 1 = + direct links, etc. (omit for unlimited)
- `--allowed-domains` — extra domains to follow beyond the start URL's own domain
- `-o DIR` — write one Markdown file per page instead of stdout
- `robots.txt` is honored automatically; override with `--robots-path`
- `--generate-front-matter` adds YAML front matter (title, url, etc.) to each page
- `--use-title-as-h1` promotes the HTML `<title>` into the Markdown as an H1

After crawling to a directory, aggregate everything with mq's `collection()` function:

```bash
mq -I null 'collection("./crawled") | map(self, fn(page): get(page, "title");)' /dev/null   # every page title
```

## In-Query HTTP Requests (`--allow-net`)

For one-off requests — especially non-GET methods, custom headers, or POST bodies, none of which `mq-crawl` supports — call HTTP directly from inside an mq query with the `http()` builtin, gated by the `--allow-net` flag:

```bash
mq --allow-net -I null 'http("get", "https://example.com")' /dev/null   # response body as a string
```

`http(method, url, body?, headers?)` — `method` is `"get"`/`:get`/`"post"`/etc. (string or symbol), `body` is a string, `headers` is a dict. HTTPS-only; requests to loopback/private/link-local addresses are always blocked regardless of the flag (SSRF protection). Without `--allow-net`, calling `http()` raises a runtime error instead of silently failing.

```bash
mq --allow-net -I null 'let token = get(get(ARGS, "named"), "TOKEN") | let h = dict() | let h = set(h, "Authorization", "Bearer " + token) | http("get", "https://api.example.com/data", h)' /dev/null --args TOKEN "secret"
mq --allow-net -I null 'http("post", "https://api.example.com/submit", "field=value")' /dev/null
```

To turn the fetched HTML into extractable Markdown in the same query, pipe through `from_html()`. Its result is a single `Array` value, so plain selector chains (`.link`, `.h`) map element-wise and leave `null`s for non-matches instead of dropping them — use `compact_map()` with an explicit-argument accessor (`get_url()`, `to_text()`, `attr()`) instead, which filters `None` results out cleanly:

```bash
mq --allow-net -I null -F json 'http("get", "https://example.com") | from_html(self) | compact_map(self, fn(n): get_url(n);)' /dev/null
# ["https://iana.org/domains/example"]

mq --allow-net -I null 'http("get", "https://example.com") | from_html(self) | compact_map(self, fn(n): if (is_h(n)): to_text(n) else: None;)' /dev/null
# page headings, as plain text
```

`--allow-write` is the equivalent gate for `write_file()` (also disabled by default).

Prefer `mq-crawl` for anything that's "fetch and read a page" — it also gives you JS rendering, multi-page crawling, and robots.txt handling for free. Reach for `http()` when you need a request `mq-crawl` can't make (POST, custom headers/auth) or want the whole fetch-and-extract step to live inside one mq query/script instead of shelling out to a second binary.

## Restricting Network Access (allowlist Patterns)

The mq toolchain is default-deny for outbound network reach — nothing is fetched without an explicit flag, and each flag scopes a different kind of reach:

**1. `mq-crawl --allowed-domains` — which *links* a crawl may follow.**
With no flag, `mq-crawl` only follows links whose domain **exactly matches** the start URL's domain — everything else is silently skipped (visible as `Skipping URL from disallowed domain` in the stderr log). This is the safe default; a `--depth 0` fetch never needs it.

```bash
mq-crawl https://docs.example.com --depth 3 --allowed-domains docs.example.com 2>/dev/null           # explicit, same as default
mq-crawl https://docs.example.com --depth 3 --allowed-domains docs.example.com,blog.example.com 2>/dev/null  # widen to a second host
```

Matching is **exact string equality per domain, not a suffix/wildcard match** — `--allowed-domains example.com` does *not* also permit `docs.example.com`. List every subdomain you want crawled explicitly.

**2. `mq --allowed-domain` — where `import "https://..."` inside an mq script may load from.**
This is unrelated to page-fetching; it only gates mq's own module-import statement (used if an extraction query pulls in a shared `.mq` script over HTTP). Default: only `raw.githubusercontent.com/harehare`. Matching here *is* a prefix match (any path under the host), and `github.com/{user}/{repo}` is auto-expanded to the equivalent `raw.githubusercontent.com` path:

```bash
mq --allowed-domain example.com 'import "https://example.com/lib/extract.mq" | ...' page.md
mq --allowed-domain github.com/someuser/mq-scripts '...' page.md   # scoped to one repo
```

**3. `mq --allow-net` — whether the `http()` builtin may run at all.**
No domain scoping — it's an on/off switch for outbound requests made *from inside a query* (see "In-Query HTTP Requests" above). Off by default; combine with the narrowest query logic needed rather than fetching more than required.

Repeat the domain-list flags to allow multiple entries. Prefer the narrowest scope that gets the job done — a single explicit domain (or `--depth 0`, which needs no allowlist at all) over a broad one.

## When You Already Have HTML in Hand

If HTML content is already fetched (e.g. via `curl`, or already in context), skip `mq-crawl` and pipe straight into `mq -I html`:

```bash
curl -s https://example.com | mq -I html '.h | to_text()'
curl -s https://example.com | mq -I html 'select(.link) | .link.url'
```

`-I html` converts HTML to Markdown first, so use Markdown selectors (`.text`, `.link`, `.h`, …), not HTML tag names — see the `processing-markdown` skill for the complete selector table, attribute reference, and output-format flags (`-F json`, `-F table`, `-F grep`, etc.).

## Limitations — Pair with curl When You Need

- **Raw HTTP metadata** (status code, response headers, timing): neither `mq` nor `mq-crawl` exposes this — use `curl -sI` / `curl -w` for that, then hand the body to `mq`.
- **Non-HTML APIs**: for JSON/XML endpoints, skip `mq-crawl` and use `curl -s <url> | mq -I json '...'` (or `-I xml`) directly.
- **Token/output budgeting**: use the global `--limit N` / `--skip N` flags, or pipe through `head`, to cap result size.

## Essential `mq-crawl` Flags

| Flag                          | Purpose                                              |
| ------------------------------ | ----------------------------------------------------- |
| `--depth N`                   | Crawl depth (0 = single page)                        |
| `-q, --mq-query`               | Apply an mq query to each page's Markdown inline     |
| `-o, --output DIR`             | Save pages as files instead of printing to stdout    |
| `-f, --format text\|json`      | Crawl statistics format (content format is separate — pipe into `mq -F ...`) |
| `--allowed-domains`            | Domains to follow beyond the start URL's domain      |
| `--headless` / `--webdriver-url` | Render JavaScript before extracting                |
| `--concurrency`, `--crawl-delay` | Parallelism and politeness delay                   |
| `--generate-front-matter`      | Add YAML front matter (title, url) per page          |

Run `mq-crawl --help` for the full flag list.
