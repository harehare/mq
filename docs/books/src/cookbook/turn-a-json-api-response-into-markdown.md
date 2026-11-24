# Turn a JSON API response into Markdown

Goal: Call a JSON API and render the response as a Markdown report, with nested objects and arrays expanded into headings, tables, and lists.

Prerequisites: The `--allow-net` flag, since outbound requests are disabled by default (see [Fetch a web page and filter it](fetch-and-filter-a-web-page.md) for the details). Pass `-I null -A` because there is no input file, so the query runs exactly once.

## Query

```bash
$ mq -I null -A --allow-net=api.github.com 'import "json" | import "md" | http(:get, "https://api.github.com/repos/harehare/mq", {"Accept": "application/vnd.github+json", "User-Agent": "mq"}) | json::json_parse() | let r = . | md::doc(md::h1(r["full_name"]), md::from_data({"stars": r["stargazers_count"], "language": r["language"], "license": r["license"], "topics": r["topics"]}))'
```

## Output

```markdown
# harehare/mq
|Key|Value|
|---|---|
|stars|1086|
|language|Rust|
## license
|Key|Value|
|---|---|
|key|mit|
|name|MIT License|
|spdx\_id|MIT|
## topics
- cli
- jq
- markdown
```

The numbers and lists above depend on the live API, so expect them to drift (long fields such as URLs are trimmed here).

## Notes

- `http()` returns the response body as a string, so `json::json_parse()` turns it into dicts and arrays. `md::from_data` then expands them: scalars become a `Key`/`Value` table, a nested object becomes a heading with its own table, and an array becomes a bullet list.
- An array of flat objects becomes a table with one column per key. Build the rows you want first with `map`, for example `http(...) | json::json_parse() | map(fn(r): {"tag": r["tag_name"], "published": r["published_at"]};) | md::from_data(.)` on `/repos/OWNER/REPO/releases`.
- Select the fields you need before calling `md::from_data`. A raw API response is large, and picking keys keeps the report readable.
- `md::from_data(data, depth)` sets the first heading level (default 2), so the result can be embedded under an existing heading. Add `-F html` to render HTML instead.
- GitHub rejects requests without a `User-Agent` header, which is why it is passed in the headers dict.
