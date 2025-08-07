# Example

## Markdown TOC

```js
.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, to_number(level))
```

## Exclude code

```js
select(!.code)
```

## Extract js code

```js
.code("js")
```

## Extract table

```js
.[1][]
```

## Extract list

```js
.[1]
```

## Extract MDX

```python
select(is_mdx())
```

## Extracts the language name from code blocks

```js
.code.lang
```

## Extracts the url from link

```js
.link.url
```

## Custom function

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

## Generate sitemap

```scala
def sitemap(item, base_url):
  let path = replace(to_text(item), ".md", ".html")
  | let loc = add(base_url, path)
  | s"<url>
  <loc>${loc}</loc>
  <priority>1.0</priority>
</url>";
```

## CSV to markdown table

```bash
$ mq 'nodes | csv2table()' example.csv
```

## Merging Multiple Files

```bash
$ mq -S 's"\n${__FILE__}\n"' 'identity()' docs/books/**/**.md
```
