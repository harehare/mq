# Extract links from an HTML page

Goal: List every link on an HTML page as a Markdown list, skipping anchors that have no `href`.

Prerequisites: The built-in `html` module, which needs the `css-selector` build feature (on by default). Read the file with `-I raw` to get its text as a single string. For a live page, pipe `curl -s <url>` into `mq -I raw`.

## Query

```bash
$ mq -I raw 'import "html" | html::html_parse(.) | html::html_find_all("a") | filter(fn(a): !is_none(html::html_attr(a, "href"));) | map(fn(a): "- [" + html::html_text(a) + "](" + html::html_attr(a, "href") + ")";) | join("\n")' page.html
```

## Input (`page.html`)

```html
<html><body>
<h1>Docs</h1>
<ul>
  <li><a href="https://example.com/a">Alpha</a></li>
  <li><a href="/b" class="ext">Beta <b>2</b></a></li>
  <li><a>No href</a></li>
</ul>
</body></html>
```

## Output

```markdown
- [Alpha](https://example.com/a)
- [Beta 2](/b)
```

## Notes

- `html_find_all(tree, "a")` returns every `<a>` element in document order. `html_text` joins the text of an element and its descendants, so `Beta <b>2</b>` becomes `Beta 2`.
- `html_attr` returns `None` when the attribute is missing, which is what the `filter` step tests for. Without it, the last link would render with no URL.
- For structured output, replace the last two steps with `map(fn(a): {"text": html::html_text(a), "href": html::html_attr(a, "href")};)` and add `-F json`.
- To convert a whole page to Markdown instead, see [Fetch a web page and filter it](fetch-and-filter-a-web-page.md).
