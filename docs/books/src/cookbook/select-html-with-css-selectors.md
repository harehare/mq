# Select HTML elements with CSS selectors

Goal: Pull the text and an attribute out of every element of an HTML page that matches a CSS selector.

Prerequisites: The built-in `html` module, which needs the `html-parse` build feature (on by default). Read the file with `-I raw` to get its text as a single string.

## Query

```bash
$ mq -I raw 'import "html" | html::html_select(self, "div.item > a[href^=\"https://\"]") | map(fn(a): {"text": html::html_text(a), "href": html::html_attr(a, "href")};)' page.html
```

## Input (`page.html`)

```html
<div class="item"><a href="https://example.com/a">Alpha</a></div>
<div class="item"><a href="/b">Beta</a></div>
```

## Output

```json
[{"text": "Alpha", "href": "https://example.com/a"}]
```

## Notes

- `html_select(data, selector)` takes a string or a `html_parse` tree and returns the matching element dicts in document order, without duplicates. No match gives `[]`. Apply `html_text` or `html_attr` to each result to extract text or attributes.
- Supported selectors:
  - `tag`, `*`, `.class`, `#id`
  - `[attr]`, `[attr=v]`, `[attr~=v]`, `[attr^=v]`, `[attr$=v]`, `[attr*=v]` (value quoted or unquoted)
  - combinators: descendant (space), `>`, `+`, `~`
  - comma-separated lists
  - `:first-child`, `:last-child`, `:nth-child(n | odd | even | an+b)`, `:not(selector list)`
- Not supported: other pseudo-classes and pseudo-elements (`:hover`, `:nth-of-type`, `::before`, ...), namespaces, case-insensitive attribute flags (`[a=v i]`), and escapes in identifiers. An unsupported or invalid selector raises an error.
- `html_text` joins the trimmed text of an element and its descendants with a space.
