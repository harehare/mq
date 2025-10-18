# Example

## Basic Element Selection

### Heading

```js
.h
```

### Extract table

```js
.[1][]
```

### Extract list

```js
.[1]
```

## Code Block Operations

### Exclude code

```js
select(!.code)
```

### Extract js code

```js
.code("js")
```

### Extracts the language name from code blocks

```js
.code.lang
```

## Link and MDX Operations

### Extract MDX

```python
select(is_mdx())
```

### Extracts the url from link

```js
.link.url
```

## Advanced Markdown Processing

### Markdown TOC

```js
.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, level)
```

### Generate sitemap

```scala
def sitemap(item, base_url):
    let path = replace(to_text(item), ".md", ".html")
    | let loc = add(base_url, path)
    | s"<url>
    <loc>${loc}</loc>
    <priority>1.0</priority>
  </url>"
end
```

## Custom Functions and Programming

### Custom function

```ruby
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | s"${first_char}${rest_str}";
  | join("")
end
| snake_to_camel()
```

### Map

```js
map(arr, fn(x): x + 1;)
```

### Filter

```js
filter(arr, fn(x): x > 10;)
```

## File Processing

### CSV to markdown table

```bash
$ mq 'include "csv" | csv_parse(true) | csv_to_markdown_table()' example.csv
```

### Merging Multiple Files

```bash
$ mq -S 's"\n${__FILE__}\n"' 'identity()' docs/books/**/**.md
```
