# Query XML with XPath

Goal: Select elements, attributes, and text from an XML file with a path expression such as `//user[@role="dev"]/name/text()`, instead of walking the tree by hand.

Prerequisites: The [xpath.mq](https://github.com/harehare/xpath.mq) extension module. Copy `xpath.mq` into your module directory, place it anywhere and point at it with `-L <dir>`, or import it over HTTP with `--allow-http-import` and `import "github.com/harehare/xpath.mq"`. Read the file with `-I xml`, so the query's input is the parsed tree.

## Query

The names of the users whose role is `dev`:

```bash
$ mq -I xml 'import "xpath" | xpath::xpath_query(., "//user[@role=\"dev\"]/name/text()")' users.xml
```

## Input (`users.xml`)

```xml
<?xml version="1.0"?>
<users>
  <user id="1" role="admin"><name>Alice</name><email>alice@example.com</email></user>
  <user id="2" role="dev"><name>Bob</name></user>
  <user id="3" role="dev"><name>Carol</name><email>carol@example.com</email></user>
</users>
```

## Output

```
["Bob", "Carol"]
```

## More paths

Each of these runs against the same `users.xml`:

| XPath | Result |
| ----- | ------ |
| `//user/@id` | `["1", "2", "3"]` |
| `//user[email]/name/text()` | `["Alice", "Carol"]` |
| `//user[contains(email, "carol")]/@id` | `["3"]` |
| `//user[2]/name/text()` | `["Bob"]` |

To get a single value instead of an array, use `xpath_first`, which returns `None` when there is no match:

```bash
$ mq -I xml 'import "xpath" | xpath::xpath_first(., "//user[@id=\"3\"]/name/text()")' users.xml
```

```
Carol
```

## Notes

- `xpath_query` returns an array. An element step (`//user`) returns the element dicts, `@name` returns the attribute's string, and `text()` returns the element's text. Pass the elements to the `xml` module to keep working on them, for example `map(fn(u): xml::xml_text(xml::xml_find(u, "name"));)`.
- No match returns `[]`, which prints nothing in the default output format. Add `-F json` to see the `[]`.
- This is an abbreviated XPath. It supports child, descendant (`//`), self, parent, `*` wildcards, `@attr`, `text()`, `name()`, positional and attribute or child predicates, and `and`, `or`, `not(...)`, `contains(...)`, `starts-with(...)`. It does not implement the full XPath function library, so fall back to `xml_find_all` for anything else.
- A malformed path raises an error rather than returning `[]`.
- For simple lookups by tag name, the built-in `xml_find_all` needs no extra module. See [Read values from an XML file](read-values-from-xml.md).
