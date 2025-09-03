# XML Functions

The XML module provides functions for parsing, processing, and converting XML (eXtensible Markup Language) data.

## Including the XML Module

To use the XML functions, include the module at the top of your mq script:

```mq
include "xml"
```

## Functions

### `xml_parse(input)`

Parses an XML string and returns the parsed data structure.

**Parameters:**
- `input`: String containing the XML data

**Returns:**
- Parsed data structure representing the XML element with the following structure:
  - `tag`: The element tag name
  - `attributes`: Dictionary of attributes
  - `children`: Array of child elements
  - `text`: Text content of the element (or `None` if empty)

**Example:**
```mq
include "xml"

# Parse simple XML element
| "<person name=\"John\" age=\"30\">Hello World</person>" | xml_parse()
# Returns: {"tag": "person", "attributes": {"name": "John", "age": "30"}, "children": [], "text": "Hello World"}

# Parse self-closing XML element
| "<input type=\"text\" name=\"username\"/>" | xml_parse()
# Returns: {"tag": "input", "attributes": {"type": "text", "name": "username"}, "children": [], "text": None}

# Parse nested XML
| "<book><title>The Great Gatsby</title><author>F. Scott Fitzgerald</author></book>" | xml_parse()
# Returns: {"tag": "book", "attributes": {}, "children": [{"tag": "title", "attributes": {}, "children": [], "text": "The Great Gatsby"}, {"tag": "author", "attributes": {}, "children": [], "text": "F. Scott Fitzgerald"}], "text": None}

# Parse XML with CDATA
| "<description><![CDATA[This is <b>bold</b> text]]></description>" | xml_parse()
# Returns: {"tag": "description", "attributes": {}, "children": [], "text": "This is <b>bold</b> text"}

# Parse XML with declaration
| "<?xml version=\"1.0\" encoding=\"UTF-8\"?><root>Content</root>" | xml_parse()
# Returns: {"tag": "root", "attributes": {}, "children": [], "text": "Content"}
```

### `xml_stringify(data)`

Converts a data structure to an XML string representation.

**Parameters:**
- `data`: XML data structure to convert (should have the structure returned by `xml_parse`)

**Returns:**
- String containing the formatted XML data

**Example:**
```mq
include "xml"

# Convert element to XML string
| {"tag": "person", "attributes": {"name": "John", "age": "30"}, "children": [], "text": "Hello World"} | xml_stringify()
# Returns: "<person name=\"John\" age=\"30\">Hello World</person>"

# Convert self-closing element to XML
| {"tag": "input", "attributes": {"type": "text", "name": "username"}, "children": [], "text": None} | xml_stringify()
# Returns: "<input type=\"text\" name=\"username\"/>"

# Convert nested structure to XML
| {"tag": "book", "attributes": {}, "children": [{"tag": "title", "attributes": {}, "children": [], "text": "The Great Gatsby"}], "text": None} | xml_stringify()
# Returns: "<book><title>The Great Gatsby</title></book>"

# Convert element with both text and children
| {"tag": "div", "attributes": {"class": "container"}, "children": [{"tag": "span", "attributes": {}, "children": [], "text": "Nested"}], "text": "Text content"} | xml_stringify()
# Returns: "<div class=\"container\">Text content<span>Nested</span></div>"

# Convert scalar values (non-XML data)
| "Plain text" | xml_stringify()
# Returns: "Plain text"

| 42 | xml_stringify()
# Returns: "42"
```

### `xml_to_markdown_table(data)`

Converts an XML data structure to a Markdown table format.

**Parameters:**
- `data`: XML data structure to convert (should have the structure returned by `xml_parse`)

**Returns:**
- String containing the Markdown table representation

**Example:**
```mq
include "xml"

# Convert simple element to Markdown table
| {"tag": "person", "attributes": {"name": "John", "age": "30"}, "children": [], "text": "Hello World"} | xml_to_markdown_table()
# Returns:
# | Tag | Attributes | Text | Children |
# | --- | --- | --- | --- |
# | person | name=John, age=30 | Hello World | 0 |

# Convert element with children to Markdown table
| {"tag": "book", "attributes": {}, "children": [{"tag": "title", "attributes": {}, "children": [], "text": "The Great Gatsby"}, {"tag": "author", "attributes": {}, "children": [], "text": "F. Scott Fitzgerald"}], "text": None} | xml_to_markdown_table()
# Returns:
# | Index | Tag | Attributes | Text |
# | --- | --- | --- | --- |
# | 0 | title |  | The Great Gatsby |
# | 1 | author |  | F. Scott Fitzgerald |
#
# ## Children of title:
#
# | Tag | Attributes | Text | Children |
# | --- | --- | --- | --- |
# | title |  | The Great Gatsby | 0 |
#
# ## Children of author:
#
# | Tag | Attributes | Text | Children |
# | --- | --- | --- | --- |
# | author |  | F. Scott Fitzgerald | 0 |

# Convert scalar value to Markdown table
| "Plain text" | xml_to_markdown_table()
# Returns:
# | Value |
# | --- |
# | Plain text |
```

## Data Structure

The XML parser creates data structures with the following format:

- **tag**: String containing the XML element tag name
- **attributes**: Dictionary containing attribute name-value pairs
- **children**: Array of child elements (each following the same structure)
- **text**: String containing the text content, or `None` if empty

## Features

- **XML Declaration Support**: Automatically handles `<?xml version="1.0" encoding="UTF-8"?>` declarations
- **Self-closing Tags**: Properly parses elements like `<input type="text"/>`
- **CDATA Support**: Handles `<![CDATA[...]]>` sections correctly
- **Comment Removal**: Automatically strips XML comments during parsing
- **Nested Elements**: Full support for deeply nested XML structures
- **Attribute Parsing**: Handles both single and double-quoted attribute values
- **Text Content**: Preserves text content while handling mixed content scenarios

## Type Conversion

The XML parser preserves the original string values from the XML:

- **Attributes**: All attribute values are treated as strings
- **Text Content**: Text content is preserved as strings
- **Element Structure**: Elements are represented as dictionaries with the standard structure
