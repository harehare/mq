# Example

under preparation

## Hello world

```js
# Hello world
def hello_world():
  add(" Hello World")?;
select(or(.[], .code, .h)) | upcase() | hello_world()
```

## Markdown TOC

```js
.h
| let link = md_link(add("#", to_text(self)), to_text(self))
| if (eq(md_name(), "h1")):
  md_list(link, 1)
elif (eq(md_name(), "h2")):
  md_list(link, 2)
elif (eq(md_name(), "h3")):
  md_list(link, 3)
elif (eq(md_name(), "h4")):
  md_list(link, 4)
elif (eq(md_name(), "h5")):
  md_list(link, 5)
else:
  None
```

## Extract js code

```js
.code("js")
```

## Custom function

```js
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
    let first_char = upcase(first(word))
    | let rest_str = downcase(slice(word, 1, len(word)))
    | add(first_char, rest_str);
  | join("");
| snake_to_camel()
```

## Generate sitemap

```python
def sitemap(item, base_url):
  let url = "<url>
  <loc>${loc}</loc>
</url>"
  | .[] 
  | let path = replace(to_text(item), ".md", ".html")
  | replace(url, "${loc}", add(base_url, path));
  | sitemap("https://example.com/")
```
