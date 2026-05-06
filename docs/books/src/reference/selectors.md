# Selectors

Selectors in mq allow you to select specific markdown nodes from a document. You can also access attributes of selected nodes using dot notation.

## Basic Selector Usage

Selectors use the `.` prefix to select markdown nodes. For example:

```mq
.h       # Selects all heading nodes
.code    # Selects all code blocks
.link    # Selects all link nodes
```

## Selector Calls (Filtered Matching)

Selectors can accept arguments to filter nodes by specific properties, using a function-call syntax:

```mq
.h(1)           # Selects only h1 headings
.h(2, 3)        # Selects h2 and h3 headings
.h(1..3)        # Selects h1, h2, and h3 headings (range)
.code("rust")   # Selects only Rust code blocks
```

### Heading Depth Filtering

Pass one or more numeric arguments to match headings at specific depths:

```mq
# Select only top-level headings
.h(1)

# Select h2 and h3 headings
.h(2, 3)

# Select h1 through h3 using a range
.h(1..3)
```

### Code Language Filtering

Pass a string argument to match code blocks with a specific language:

```mq
# Select only Rust code blocks
.code("rust")

# Select Python or JavaScript code blocks
.code("python") | to_array | concat(.code("javascript"))
```

### Combining with Other Operations

Selector calls can be combined with pipes and functions just like plain selectors:

```mq
# Extract content of all h2 headings
.h(2) | .value

# Count Rust code blocks
.code("rust") | len()

# Replace language of all TypeScript blocks
.code("typescript") | set_attr("lang", "ts")
```

## Attribute Access

Once you've selected a node, you can access its attributes using dot notation. The available attributes depend on the node type.

### Common Attributes

#### `value`

Most nodes support `value` to get the text content:

```mq
.code.value    # Gets the code content
```

### Heading Attributes

Heading nodes support the following attributes:

| Attribute        | Type    | Description              | Example    |
| ---------------- | ------- | ------------------------ | ---------- |
| `depth`, `level` | Integer | The heading level (1-6)  | `.h.level` |
| `value`          | String  | The value of the heading | `.h.value` |

Example:

```mq
# Input: # Hello World

.h.level # Returns: 1
.h.value # Returns: "Hello World"
```

### Code Block Attributes

Code block nodes support the following attributes:

| Attribute          | Type    | Description                             | Example       |
| ------------------ | ------- | --------------------------------------- | ------------- |
| `lang`, `language` | String  | The language of the code block          | `.code.lang`  |
| `value`            | String  | The code content                        | `.code.value` |
| `meta`             | String  | Metadata associated with the code block | `.code.meta`  |
| `fence`            | Boolean | Whether the code block is fenced        | `.code.fence` |

Example:

```mq
# Input: ```rust
# fn main() {}
# ```

.code.lang      # Returns: "rust"
.code.value     # Returns: "fn main() {}"
```

### Link Attributes

Link nodes support the following attributes:

| Attribute | Type   | Description           | Example       |
| --------- | ------ | --------------------- | ------------- |
| `url`     | String | The URL of the link   | `.link.url`   |
| `title`   | String | The title of the link | `.link.title` |
| `value`   | String | The link value        | `.link.value` |

Example:

```mq
# Input: [Example](https://example.com "Example Site")

.link.url       # Returns: "https://example.com"
.link.title     # Returns: "Example Site"
.link.value     # Returns: "Example"
```

### Image Attributes

Image nodes support the following attributes:

| Attribute | Type   | Description               | Example        |
| --------- | ------ | ------------------------- | -------------- |
| `url`     | String | The URL of the image      | `.image.url`   |
| `alt`     | String | The alt text of the image | `.image.alt`   |
| `title`   | String | The title of the image    | `.image.title` |

Example:

```mq
# Input: ![Alt text](image.png "Image Title")

.image.url      # Returns: "image.png"
.image.alt      # Returns: "Alt text"
.image.title    # Returns: "Image Title"
```

### List Attributes

List nodes support the following attributes:

| Attribute | Type    | Description                        | Example         |
| --------- | ------- | ---------------------------------- | --------------- |
| `index`   | Integer | The index of the list item         | `.list.index`   |
| `level`   | Integer | The nesting level of the list item | `.list.level`   |
| `ordered` | Boolean | Whether the list is ordered        | `.list.ordered` |
| `checked` | Boolean | The checked state (for task lists) | `.list.checked` |
| `value`   | String  | The text content of the list item  | `.list.value`   |

### Table Cell Attributes

Table cell nodes support the following attributes:

| Attribute               | Type    | Description                                | Example                         |
| ----------------------- | ------- | ------------------------------------------ | ------------------------------- |
| `row`                   | Integer | The row number of the cell                 | `.[0][0].row`                   |
| `column`                | Integer | The column number of the cell              | `.[0][0].column`                |
| `last_cell_in_row`      | Boolean | Whether this is the last cell in the row   | `.[0][0].last_cell_in_row`      |
| `last_cell_of_in_table` | Boolean | Whether this is the last cell in the table | `.[0][0].last_cell_of_in_table` |
| `value`                 | String  | The text content of the cell               | `.[0][0].value`                 |

### Reference Nodes Attributes

Reference nodes (link references, image references, footnotes) support:

| Node Type       | Attributes                       | Description                            |
| --------------- | -------------------------------- | -------------------------------------- |
| `.link_ref`     | `ident`, `label`                 | Identifier and label of link reference |
| `.image_ref`    | `ident`, `label`, `alt`          | Identifier, label, and alt text        |
| `.footnote_ref` | `ident`, `label`                 | Identifier and label of footnote       |
| `.footnote`     | `ident`, `text`                  | Identifier and content of footnote     |
| `.definition`   | `ident`, `url`, `title`, `label` | Link/image definition attributes       |

### MDX Attributes

MDX nodes support the following attributes:

| Attribute | Type   | Description                 | Example                      |
| --------- | ------ | --------------------------- | ---------------------------- |
| `name`    | String | The name of the MDX element | `.mdx_jsx_flow_element.name` |
| `value`   | String | The content of the MDX node | `.mdx_flow_expression.value` |

### Text Nodes Attributes

Text, HTML, YAML, TOML, Math nodes support:

| Attribute | Type   | Description      | Example       |
| --------- | ------ | ---------------- | ------------- |
| `value`   | String | The text content | `.text.value` |

## Property Selector (Dict Key Access)

Property selectors access values from dict (object) values using dot notation. They work on both single dicts and arrays of dicts.

### Bare Form

Use `.key` to access a dict key by name:

```mq
# Input dict: {"name": "Alice", "age": 30}

.name   # Returns: "Alice"
.age    # Returns: 30
```

### Quoted Form

Use `."key"` to access keys that contain spaces, special characters, or conflict with reserved selector names:

```mq
# Input dict: {"h1": "title", "my key": "value"}

."h1"       # Returns: "title"  (avoids conflict with .h1 heading selector)
."my key"   # Returns: "value"  (key with space)
."url"      # Returns: the "url" key value (avoids conflict with .url attribute)
```

Escape sequences inside quoted keys: `\"` for a literal `"` and `\\` for a literal `\`.

### Arrays of Dicts

When applied to an array of dicts, the property selector maps over each element:

```mq
# Input: [{"name": "Alice"}, {"name": "Bob"}, {"name": "Charlie"}]

.name   # Returns: ["Alice", "Bob", "Charlie"]
```

Non-dict elements in the array return `none`.

### Missing Keys

Accessing a key that doesn't exist returns `none`:

```mq
# Input dict: {"name": "Alice"}

.age    # Returns: none
```

## Combining Selectors with Functions

You can combine selectors with functions like `select()`, `map()`, and `filter()` for powerful transformations:

### Using select()

The `select()` function filters elements based on a condition:

```mq
# Select only code blocks (exclude non-code nodes)
select(.code)

# Select nodes that are not code blocks
select(!.code)
```

### Using map()

Transform each selected node:

```mq
# Get all heading levels
.h | map(fn(h): h.level;)

# Get all code block languages
.code | map(fn(c): c.lang;)
```

### Using filter()

Filter nodes based on attribute values:

```mq
# Get only level 2 headings
.h | filter(fn(h): h.level == 2;)

# Get only rust code blocks
.code | filter(fn(c): c.lang == "rust";)
```

The selector call syntax provides a more concise alternative for common cases:

```mq
.h(2)           # equivalent to: .h | filter(fn(h): h.level == 2;)
.code("rust")   # equivalent to: .code | filter(fn(c): c.lang == "rust";)
```

### Extract Code Languages

```mq
.code.lang
```

### Extract All Links

```mq
.link.url
```

### Filter High-Level Headings

```mq
# Using attribute comparison
select(.h.level <= 2)

# Using selector call for exact levels
.h(1, 2)
```

## Setting Attributes

You can also modify node attributes using the `set_attr()` function:

```mq
# Change code block language
.code | set_attr("lang", "javascript")

# Update link URL
.link | set_attr("url", "https://new-url.com")

# Update heading level
.h | set_attr("level", 2)
```

Note: Not all attributes are settable. Refer to the implementation in `mq-markdown/src/node.rs` for details on which attributes can be modified.

## See Also

- [Builtin selectors](builtin_selectors.md) - Complete list of available selectors
- [Builtin functions](builtin_functions.md) - Functions to use with selectors
- [Nodes](nodes.md) - Details about markdown node types
