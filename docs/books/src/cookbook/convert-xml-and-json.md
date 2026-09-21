# Convert between XML and JSON

Goal: Turn an XML file into JSON, reshape it into the plain records you actually want, and write JSON (or a modified tree) back out as XML.

Prerequisites: The built-in `xml` module for reshaping and writing trees. Plain conversion needs no import: `-I xml` reads XML and `-F xml` writes it.

## Query

XML to a plain JSON array:

```bash
$ mq -I xml -F json 'import "xml" | xml::xml_find_all("book") | map(fn(b): {"id": xml::xml_attr(b, "id"), "title": xml::xml_text(xml::xml_find(b, "title")), "year": to_number(xml::xml_text(xml::xml_find(b, "year")))};)' books.xml
```

JSON to XML:

```bash
$ echo '{"name": "web", "ports": [80, 443], "debug": false}' | mq -I json -F xml '.'
```

## Input (`books.xml`)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<library>
  <book id="b1" lang="en">
    <title>Dune</title>
    <year>1965</year>
  </book>
  <book id="b2" lang="ja">
    <title>Kokoro</title>
    <year>1914</year>
  </book>
</library>
```

## Output

XML to a plain JSON array:

```json
[
  {
    "id": "b1",
    "title": "Dune",
    "year": 1965
  },
  {
    "id": "b2",
    "title": "Kokoro",
    "year": 1914
  }
]
```

JSON to XML:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<root>
  <name>web</name>
  <ports>
    <item>80</item>
    <item>443</item>
  </ports>
  <debug>false</debug>
</root>
```

## See the raw tree

`mq -I xml -F json '.'` prints the tree that every `xml` function works on. For `<book id="b1"><title>Dune</title><year>1965</year></book>` it is:

```json
{
  "tag": "book",
  "attributes": {
    "id": "b1"
  },
  "children": [
    {
      "tag": "title",
      "attributes": {},
      "children": [],
      "text": "Dune"
    },
    {
      "tag": "year",
      "attributes": {},
      "children": [],
      "text": "1965"
    }
  ],
  "text": null
}
```

Every element has the same four keys. `text` is `null` for an element that only holds other elements.

## Notes

- Text values are always strings, which is why the query wraps `year` in `to_number`.
- JSON to XML wraps the data in a `<root>` element, names array entries `<item>`, and writes every object key as a child element. To produce attributes, build a tree in the `{tag, attributes, children, text}` shape and pass it to `xml::xml_stringify`: `mq -I null 'import "xml" | xml::xml_stringify({"tag": "greeting", "attributes": {"lang": "en"}, "children": [], "text": "hello"})'` prints `<greeting lang="en">hello</greeting>` under an XML declaration.
- `mq -I xml -F xml '.' file.xml` reads a file and writes it back pretty-printed, which is a quick way to reformat XML. `xml_stringify` on a parsed tree writes it on a single line instead.
- Other formats work the same way, for example `mq -I yaml -F xml '.' config.yaml`.
- For the JSON and YAML pair, see [Convert between YAML and JSON](convert-yaml-and-json.md). To pull values out of XML without converting it, see [Read values from an XML file](read-values-from-xml.md).
