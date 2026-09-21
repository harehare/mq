# Read values from an XML file

Goal: Pull elements, text, and attributes out of an XML file such as a Maven `pom.xml` or an RSS feed, and print them as a Markdown table or list.

Prerequisites: The built-in `xml` module. Read the file with `-I xml`, which parses it into a tree of `{tag, attributes, children, text}` dicts. The example also uses the built-in `csv` module to render a table.

## Query

The dependencies of a `pom.xml` as a Markdown table:

```bash
$ mq -I xml 'import "xml" | import "csv" | xml::xml_find_all("dependency") | map(fn(d): {"group": xml::xml_text(xml::xml_find(d, "groupId")), "artifact": xml::xml_text(xml::xml_find(d, "artifactId")), "version": xml::xml_text(xml::xml_find(d, "version"))};) | csv::csv_to_markdown_table()' pom.xml
```

## Input (`pom.xml`)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project>
  <modelVersion>4.0.0</modelVersion>
  <artifactId>demo-app</artifactId>
  <version>1.2.0</version>
  <dependencies>
    <dependency>
      <groupId>org.slf4j</groupId>
      <artifactId>slf4j-api</artifactId>
      <version>2.0.9</version>
    </dependency>
    <dependency>
      <groupId>com.google.guava</groupId>
      <artifactId>guava</artifactId>
      <version>33.0.0-jre</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>
```

## Output

```markdown
| group | artifact | version |
| --- | --- | --- |
| org.slf4j | slf4j-api | 2.0.9 |
| com.google.guava | guava | 33.0.0-jre |
```

## Filter by an attribute

`xml_attr(element, name)` returns an attribute's value, or `None` when it is not set. This lists the `id` of every `<user>` whose `role` is `dev`:

```bash
$ echo '<users><user id="1" role="admin"/><user id="2" role="dev"/><user id="3" role="dev"/></users>' | mq -I xml 'import "xml" | xml::xml_find_all("user") | filter(fn(u): xml::xml_attr(u, "role") == "dev";) | map(fn(u): xml::xml_attr(u, "id");)'
```

```
["2", "3"]
```

## Turn an RSS feed into a list

```bash
$ curl -s https://example.com/feed.xml | mq -I xml 'import "xml" | xml::xml_find_all("item") | map(fn(i): "- [" + xml::xml_text(xml::xml_find(i, "title")) + "](" + xml::xml_text(xml::xml_find(i, "link")) + ")";) | join("\n")'
```

With a feed that has two `<item>` entries:

```markdown
- [v0.8.5](https://example.com/v0.8.5)
- [v0.8.4](https://example.com/v0.8.4)
```

## Notes

- `xml_find_all(tree, tag)` returns every element named `tag` in document order, including the root itself. `xml_find` returns only the first match, or `None` when nothing matches. Because matching is at any depth, `xml_find(tree, "version")` on the `pom.xml` above returns the project's own `1.2.0`, since it comes before the dependency versions.
- `xml_text(element)` joins the text of an element and all its descendants with single spaces. For `<user><name>Alice</name><email>a@example.com</email></user>` it returns `Alice a@example.com`.
- Keep only some elements by testing a child: `filter(fn(d): xml::xml_text(xml::xml_find(d, "scope")) == "test";)` keeps the test-scoped dependencies of the `pom.xml` above. Elements without a `<scope>` do not match.
- `-I xml` is shorthand for reading with `-I raw` and calling `xml::xml_parse` first. Use the longer form when the XML is a string inside other data, such as a field of a JSON document.
- For path-style queries such as `//user[@role="dev"]/name`, see [Query XML with XPath](query-xml-with-xpath.md). To go from XML to JSON and back, see [Convert between XML and JSON](convert-xml-and-json.md).
- For RSS 2.0 and Atom feeds specifically, the [feed.mq](https://github.com/harehare/feed.mq) extension module parses both into one shape.
