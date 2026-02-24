# Operator

## Pipe Operator

A functional operator that allows chaining multiple filter operations together.

### Usage

The pipe operator (`|`) enables sequential processing of filters, where the output of one filter becomes the input of the next filter.

### Examples

```mq
# Basic pipe usage
42 | add(1) | mul(2)
# => 86

# Multiple transformations
let mul2 = def mul2(x): mul(x, 2);
let gt4 = def gt4(x): gt(x, 4);
array(1, 2, 3) | map(mul2) | filter(gt4)
# => [6]

# Function composition
let double = def _double(x): mul(x, 2);
let add_one = def _add_one(x): add(x, 1);
5 | double(self) | add_one(self)
# => 11
```

## Shift Operators

The shift operators (`<<` and `>>`) perform different operations depending on the type of the operand.

### Left Shift (`<<`)

The left shift operator (`<<`) maps to the `shift_left(value, amount)` builtin function.

| Operand type     | Behavior                                                                                            |
| ---------------- | --------------------------------------------------------------------------------------------------- |
| Number           | Bitwise left shift: multiplies the value by `2^amount`                                              |
| String           | Removes `amount` characters from the **start** of the string                                        |
| Array            | Appends the value to the end of the array                                                           |
| Markdown Heading | Decreases the heading depth by `amount` (promotes the heading, e.g. `##` → `#`), minimum depth is 1 |

#### Examples

```mq
# Bitwise left shift on numbers
1 << 2
# => 4

shift_left(1, 3)
# => 8

# Remove characters from the start of a string
shift_left("hello", 2)
# => "llo"

"hello" << 2
# => "llo"

# Promote a heading (decrease depth)
let md = do to_markdown("## Heading 2") | first(); |
md << 1
# => # Heading 2
```

### Right Shift (`>>`)

The right shift operator (`>>`) maps to the `shift_right(value, amount)` builtin function.

| Operand type     | Behavior                                                                                           |
| ---------------- | -------------------------------------------------------------------------------------------------- |
| Number           | Bitwise right shift on the truncated integer value (shifts the bits right by `amount`)             |
| String           | Removes `amount` characters from the **end** of the string                                         |
| Array            | Adds the value to the beginning of the array                                                       |
| Markdown Heading | Increases the heading depth by `amount` (demotes the heading, e.g. `#` → `##`), maximum depth is 6 |

#### Examples

```mq
# Bitwise right shift on numbers
4 >> 2
# => 1

shift_right(8, 2)
# => 2

# Remove characters from the end of a string
shift_right("hello", 2)
# => "hel"

"hello" >> 2
# => "hel"

# Demote a heading (increase depth)
let md = do to_markdown("# Heading 1") | first(); |
md >> 1
# => ## Heading 1
```

## Conversion Operator (`@`)

The conversion operator (`@`) converts a value to a different type or format. It maps to the `convert(value, type)` builtin function.

### Usage

```
value @ type
```

The `type` operand can be a symbol or a string that specifies the target format. The supported conversion targets are:

| Type (symbol) | Type (string)                  | Behavior                                              |
| ------------- | ------------------------------ | ----------------------------------------------------- |
| `:h1`         | `"#"`                          | Convert to a Markdown heading level 1                 |
| `:h2`         | `"##"`                         | Convert to a Markdown heading level 2                 |
| `:h3`         | `"###"`                        | Convert to a Markdown heading level 3                 |
| `:h4`         | `"####"`                       | Convert to a Markdown heading level 4                 |
| `:h5`         | `"#####"`                      | Convert to a Markdown heading level 5                 |
| `:h6`         | `"######"`                     | Convert to a Markdown heading level 6                 |
| `:html`       |                                | Convert Markdown to an HTML string                    |
| `:text`       |                                | Extract the plain text content of a node              |
| `:sh`         |                                | Shell-escape the value for safe use in shell commands |
| `:base64`     |                                | Encode the value as a Base64 string                   |
| `:uri`        |                                | URL-encode the value                                  |
| `:urid`       |                                | URL-decode the value                                  |
|               | `">"`                          | Convert to a Markdown blockquote                      |
|               | `"-"`                          | Convert to a Markdown list item                       |
|               | `"~~"`                         | Convert to a Markdown strikethrough                   |
|               | `"<url>"` (a valid URL string) | Convert to a Markdown link with the given URL         |
|               | `**`  | Convert to a Markdown strong/bold          |

### Examples

```mq
# Convert a string to a Markdown heading
"Hello World" @ :h1
# => # Hello World

"Hello World" @ :h2
# => ## Hello World

# Convert using string syntax
"Hello World" @ "##"
# => ## Hello World

# Convert to a blockquote
"Important note" @ ">"
# => > Important note

# Convert to a list item
"Item one" @ "-"
# => - Item one

# Convert to a strikethrough
"old text" @ "~~"
# => ~~old text~~

# Convert to a Markdown link
"mq" @ "https://harehare.github.io/mq"
# => [mq](https://harehare.github.io/mq)

# Convert Markdown to HTML
let md = do to_markdown("# Hello") | first(); |
md @ :html
# => "<h1>Hello</h1>"

# Extract plain text from a Markdown node
let md = do to_markdown("## Hello World") | first(); |
md @ :text
# => "Hello World"

# Shell-escape a string for safe use in shell
"hello world" @ :sh
# => 'hello world'

"safe-string" @ :sh
# => safe-string

# Encode to Base64
"hello" @ :base64
# => "aGVsbG8="

# URL-encode a string
"hello world" @ :uri
# => "hello%20world"
```

## .. Operator

The range operator (`..`) creates sequences of consecutive values between a start and end point.

### Usage

The range operator generates arrays of values from a starting point to an ending point (inclusive). It works with both numeric values and characters.

### Examples

```mq
# Numeric ranges
1..5
# => [1, 2, 3, 4, 5]

# Character ranges
'a'..'e'
# => ["a", "b", "c", "d", "e"]

# Using ranges with other operations
1..3 | map(fn(x): mul(x, 2);)
# => [2, 4, 6]

# Reverse ranges
5..1
# => [5, 4, 3, 2, 1]

# Single element range
3..3
# => [3]
```
