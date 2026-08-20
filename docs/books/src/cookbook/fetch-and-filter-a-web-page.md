# Fetch a web page and filter it

Goal: Pull a live web page straight into an mq pipeline and slice it down to just the parts you need (headings, links, ...) — no separate `curl` + parsing script.

Prerequisites: The `--allow-net` flag, since outbound requests are disabled by default. `--allow-net=DOMAIN` restricts requests to just that domain (and any path under it); a bare `--allow-net` allows any host. Only `https://` URLs are accepted — the client is SSRF-hardened (no automatic redirects, and loopback/private/link-local addresses are blocked even with `--allow-net` set). There's no file to read here, so also pass `-I null` (null input); without it, mq has zero input nodes to run the pipeline against and the query never executes. Add `-A` too, so the query — and any `http_get()`/`http()` call inside it — runs exactly once instead of once per top-level input node.

## Query

```bash
$ mq -I null -A --allow-net=mqlang.org 'http_get("https://mqlang.org") | from_html | .h(1..2)'
```

## Output

```markdown
# Query. Filter.

## What is mq?

## Why mq?

## Features

## Subcommands

## Try mq right now
```

## Notes

- `from_html()` converts the fetched HTML body to Markdown and parses it into the same kind of node array mq builds from a `.md` file, so any selector or function that works on a Markdown document works here too.
- `http_get(url, headers = {})` is a convenience wrapper for `http(:get, url, headers)`. Matching wrappers exist for the other verbs (`http_post`, `http_put`, `http_patch`, `http_delete`, `http_head`), and `http_all([{"url": ...}, ...])` fetches a batch of URLs concurrently for the same pattern applied to a list of pages.
- Gotcha: chaining a second selector directly off an in-memory array like `from_html()`'s result doesn't drop non-matching nodes — it leaves a `None` placeholder for each one instead, so `.link.url` produces a result padded with blanks. Use `select(.link) | .url` instead, which filters first and then projects; this differs from running mq on a real file, where each top-level node streams through the pipeline separately and non-matches are dropped automatically.
- The headings above are whatever mqlang.org's homepage has today — expect this exact output to drift as the site changes.
- `-A` matters most once this pattern is adapted to run over real file input instead of `-I null`: mq normally evaluates the query once per top-level node, so a `http_get()`/`http()` call inside it would fire once per node in the document(s). `-A` aggregates all input into a single array first, so the query body — including any network calls — runs exactly once regardless of how many nodes or files are involved.
