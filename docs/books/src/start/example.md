# Example

under preparation

## Hello world

```js
# Hello world
select(or(.[], .code, .h)) | upcase() | add(" Hello World")
```

## Markdown TOC

```js
.h
| let link = to_link(add("#", to_text(self)), to_text(self), "")
| if (is_h1()):
  to_md_list(link, 1)
elif (is_h2()):
  to_md_list(link, 2)
elif (is_h3()):
  to_md_list(link, 3)
elif (is_h4()):
  to_md_list(link, 4)
elif (is_h5()):
  to_md_list(link, 5)
else:
  None
```

## Exclude code

```js
select(not(.code))
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

## Custom function

```js
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | s"${first_char}${rest_str}";
  | join("");
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
