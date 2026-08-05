# Generate an XML sitemap from Markdown files

**Goal**: Build a sitemap entry per file from a set of Markdown docs, based on each file's path.

**Prerequisites**: None.

## Query

```bash
$ mq 'def sitemap(item, base_url):
    let path = replace(to_text(item), ".md", ".html")
    | let loc = base_url + path
    | s"<url>
  <loc>${loc}</loc>
  <priority>1.0</priority>
  </url>"
end
| nodes
| first
| sitemap(__FILE__, "https://example.com/")' docs/**/*.md
```

## Output

```xml
<url>
  <loc>https://example.com/docs/intro.html</loc>
  <priority>1.0</priority>
  </url>
<url>
  <loc>https://example.com/docs/usage.html</loc>
  <priority>1.0</priority>
  </url>
```

## Notes

- **Gotcha**: without a query that consumes all of a file's nodes first, mq re-runs the whole pipeline once per node in the file — since `sitemap` here ignores its piped input and only reads `__FILE__`, that means one duplicate `<url>` entry per node. `nodes | first` collapses each file down to a single evaluation before calling `sitemap`, so you get exactly one entry per file.
