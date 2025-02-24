# Builtin selectors and functions

## Functions

| Function Name    | Description                                                                      | Parameters                 | Example                                     |
| ---------------- | -------------------------------------------------------------------------------- | -------------------------- | ------------------------------------------- |
| `halt`           | Terminates the program with the given exit code.                                 | `exit_code`                | `halt(0)`                                   |
| `debug`          | Prints the debug information of the given value.                                 | `value`                    | `debug("Hello, World!")`                    |
| `type`           | Returns the type of the given value.                                             | `value`                    | `type(123)`                                 |
| `array`          | Creates an array from the given values.                                          | `values`                   | `array(1, 2, 3)`                            |
| `from_date`      | Converts a date string to a timestamp.                                           | `date_str`                 | `from_date("2023-01-01")`                   |
| `to_date`        | Converts a timestamp to a date string with the given format.                     | `timestamp`, `format`      | `to_date(1672531200000, "%Y-%m-%d")`        |
| `now`            | Returns the current timestamp.                                                   | None                       | `now()`                                     |
| `base64`         | Encodes the given string to base64.                                              | `input`                    | `base64("Hello, World!")`                   |
| `base64d`        | Decodes the given base64 string.                                                 | `input`                    | `base64d("SGVsbG8sIFdvcmxkIQ==")`           |
| `min`            | Returns the minimum of two values.                                               | `value1`, `value2`         | `min(1, 2)`                                 |
| `max`            | Returns the maximum of two values.                                               | `value1`, `value2`         | `max(1, 2)`                                 |
| `to_html`        | Converts the given markdown string to HTML.                                      | `markdown`                 | `to_html("# Hello")`                        |
| `to_csv`         | Converts the given value to a CSV.                                               | `value`                    | `to_csv([1, 2, 3])`                         |
| `to_tsv`         | Converts the given value to a TSV.                                               | `value`                    | `to_tsv([1, 2, 3])`                         |
| `to_string`      | Converts the given value to a string.                                            | `value`                    | `to_string(123)`                            |
| `to_number`      | Converts the given value to a number.                                            | `value`                    | `to_number("123")`                          |
| `url_encode`     | URL-encodes the given string.                                                    | `input`                    | `url_encode("Hello, World!")`               |
| `to_text`        | Converts the given markdown node to plain text.                                  | `markdown`                 | `to_text("# Hello")`                        |
| `ends_with`      | Checks if the given string ends with the specified substring.                    | `string`, `substring`      | `ends_with("Hello, World!", "World!")`      |
| `starts_with`    | Checks if the given string starts with the specified substring.                  | `string`, `substring`      | `starts_with("Hello, World!", "Hello")`     |
| `match`          | Finds all matches of the given pattern in the string.                            | `string`, `pattern`        | `match("Hello, World!", "o")`               |
| `downcase`       | Converts the given string to lowercase.                                          | `input`                    | `downcase("HELLO")`                         |
| `replace`        | Replaces all occurrences of a substring with another substring.                  | `string`, `from`, `to`     | `replace("Hello, World!", "World", "Rust")` |
| `repeat`         | Repeats the given string a specified number of times.                            | `string`, `count`          | `repeat("Hello", 3)`                        |
| `explode`        | Splits the given string into an array of characters.                             | `string`                   | `explode("Hello")`                          |
| `implode`        | Joins an array of characters into a string.                                      | `array`                    | `implode(['H', 'e', 'l', 'l', 'o'])`        |
| `trim`           | Trims whitespace from both ends of the given string.                             | `input`                    | `trim(" Hello ")`                           |
| `upcase`         | Converts the given string to uppercase.                                          | `input`                    | `upcase("hello")`                           |
| `slice`          | Extracts a substring from the given string.                                      | `string`, `start`, `end`   | `slice("Hello, World!", 0, 5)`              |
| `pow`            | Raises the base to the power of the exponent.                                    | `base`, `exponent`         | `pow(2, 3)`                                 |
| `index`          | Finds the first occurrence of a substring in the given string.                   | `string`, `substring`      | `index("Hello, World!", "World")`           |
| `len`            | Returns the length of the given string or array.                                 | `value`                    | `len("Hello")`                              |
| `rindex`         | Finds the last occurrence of a substring in the given string.                    | `string`, `substring`      | `rindex("Hello, World!", "o")`              |
| `nth`            | Gets the element at the specified index in the array or string.                  | `array_or_string`, `index` | `nth([1, 2, 3], 1)`                         |
| `join`           | Joins the elements of an array into a string with the given separator.           | `array`, `separator`       | `join([1, 2, 3], ",")`                      |
| `reverse`        | Reverses the given string or array.                                              | `value`                    | `reverse("Hello")`                          |
| `sort`           | Sorts the elements of the given array.                                           | `array`                    | `sort([3, 1, 2])`                           |
| `compact`        | Removes None values from the given array.                                        | `array`                    | `compact([1, None, 2])`                     |
| `range`          | Creates an array of numbers within the specified range.                          | `start`, `end`             | `range(1, 5)`                               |
| `split`          | Splits the given string by the specified separator.                              | `string`, `separator`      | `split("Hello, World!", ",")`               |
| `uniq`           | Removes duplicate elements from the given array.                                 | `array`                    | `uniq([1, 2, 2, 3])`                        |
| `eq`             | Checks if two values are equal.                                                  | `value1`, `value2`         | `eq(1, 1)`                                  |
| `ne`             | Checks if two values are not equal.                                              | `value1`, `value2`         | `ne(1, 2)`                                  |
| `gt`             | Checks if the first value is greater than the second value.                      | `value1`, `value2`         | `gt(2, 1)`                                  |
| `gte`            | Checks if the first value is greater than or equal to the second value.          | `value1`, `value2`         | `gte(2, 2)`                                 |
| `lt`             | Checks if the first value is less than the second value.                         | `value1`, `value2`         | `lt(1, 2)`                                  |
| `lte`            | Checks if the first value is less than or equal to the second value.             | `value1`, `value2`         | `lte(2, 2)`                                 |
| `add`            | Adds two values.                                                                 | `value1`, `value2`         | `add(1, 2)`                                 |
| `sub`            | Subtracts the second value from the first value.                                 | `value1`, `value2`         | `sub(2, 1)`                                 |
| `div`            | Divides the first value by the second value.                                     | `value1`, `value2`         | `div(4, 2)`                                 |
| `mul`            | Multiplies two values.                                                           | `value1`, `value2`         | `mul(2, 3)`                                 |
| `mod`            | Calculates the remainder of the division of the first value by the second value. | `value1`, `value2`         | `mod(5, 2)`                                 |
| `and`            | Performs a logical AND operation on two boolean values.                          | `value1`, `value2`         | `and(true, false)`                          |
| `or`             | Performs a logical OR operation on two boolean values.                           | `value1`, `value2`         | `or(true, false)`                           |
| `not`            | Performs a logical NOT operation on a boolean value.                             | `value`                    | `not(true)`                                 |
| `round`          | Rounds the given number to the nearest integer.                                  | `number`                   | `round(1.5)`                                |
| `trunc`          | Truncates the given number to an integer by removing the fractional part.        | `number`                   | `trunc(1.5)`                                |
| `ceil`           | Rounds the given number up to the nearest integer.                               | `number`                   | `ceil(1.1)`                                 |
| `floor`          | Rounds the given number down to the nearest integer.                             | `number`                   | `floor(1.9)`                                |
| `del`            | Deletes the element at the specified index in the array or string.               | `array_or_string`, `index` | `del([1, 2, 3], 1)`                         |
| `abs`            | Returns the absolute value of the given number.                                  | `number`                   | `abs(-1)`                                   |
| `md_name`        | Returns the name of the given markdown node.                                     | `markdown`                 | `md_name(node)`                             |
| `md_text`        | Creates a markdown text node with the given value.                               | `value`                    | `md_text("Hello")`                          |
| `md_image`       | Creates a markdown image node with the given URL, alt text, and title.           | `url`, `alt`, `title`      | `md_image("url", "alt", "title")`           |
| `md_code`        | Creates a markdown code block with the given value and language.                 | `value`, `language`        | `md_code("code", "rust")`                   |
| `md_code_inline` | Creates an inline markdown code node with the given value.                       | `value`                    | `md_code_inline("code")`                    |
| `md_h`           | Creates a markdown heading node with the given value and depth.                  | `value`, `depth`           | `md_h("Heading", 1)`                        |
| `md_math`        | Creates a markdown math block with the given value.                              | `value`                    | `md_math("E=mc^2")`                         |
| `md_math_inline` | Creates an inline markdown math node with the given value.                       | `value`                    | `md_math_inline("E=mc^2")`                  |
| `md_strong`      | Creates a markdown strong (bold) node with the given value.                      | `value`                    | `md_strong("bold")`                         |
| `md_em`          | Creates a markdown emphasis (italic) node with the given value.                  | `value`                    | `md_em("italic")`                           |
| `md_hr`          | Creates a markdown horizontal rule node.                                         | None                       | `md_hr()`                                   |
| `md_list`        | Creates a markdown list node with the given value and indent level.              | `value`, `indent`          | `md_list("item", 1)`                        |
| `md_check`       | Creates a markdown list node with the given checked state.                       | `list`, `checked`          | `md_check(list, true)`                      |

## Selectors

| Selector Name           | Description                                                     | Parameters      | Example                 |
| ----------------------- | --------------------------------------------------------------- | --------------- | ----------------------- |
| `.h`                    | Selects a heading node with the specified depth.                |                 | `.h`                    |
| `.h1`                   | Selects a heading node with the 1 depth.                        | None            | `.h1`                   |
| `.h2`                   | Selects a heading node with the 2 depth.                        | None            | `.h2`                   |
| `.h3`                   | Selects a heading node with the 3 depth.                        | None            | `.h3`                   |
| `.h4`                   | Selects a heading node with the 4 depth.                        | None            | `.h4`                   |
| `.h5`                   | Selects a heading node with the 5 depth.                        | None            | `.h5`                   |
| `.#`                    | Selects a heading node with the 1 depth.                        | None            | `.#`                    |
| `.##`                   | Selects a heading node with the 2 depth.                        | None            | `.##`                   |
| `.###`                  | Selects a heading node with the 3 depth.                        | None            | `.###`                  |
| `.####`                 | Selects a heading node with the 4 depth.                        | None            | `.####`                 |
| `.#####`                | Selects a heading node with the 5 depth.                        | None            | `.#####`                |
| `.code`                 | Selects a code block node with the specified language.          | `lang`          | `.code "rust"`          |
| `.code_inline`          | Selects an inline code node.                                    | None            | `.code_inline`          |
| `.inline_math`          | Selects an inline math node.                                    | None            | `.inline_math`          |
| `.strong`               | Selects a strong (bold) node.                                   | None            | `.strong`               |
| `.emphasis`             | Selects an emphasis (italic) node.                              | None            | `.emphasis`             |
| `.delete`               | Selects a delete (strikethrough) node.                          | None            | `.delete`               |
| `.link`                 | Selects a link node.                                            | None            | `.link`                 |
| `.link_ref`             | Selects a link reference node.                                  | None            | `.link_ref`             |
| `.image`                | Selects an image node.                                          | None            | `.image`                |
| `.heading`              | Selects a heading node with the specified depth.                | None            | `.heading 1`            |
| `.horizontal_rule`      | Selects a horizontal rule node.                                 | None            | `.horizontal_rule`      |
| `.blockquote`           | Selects a blockquote node.                                      | None            | `.blockquote`           |
| `.[][]`                 | Selects a table cell node with the specified row and column.    | `row`, `column` | `.[1][1]`               |
| `.html` ,`.<>`          | Selects an HTML node.                                           | None            | `.html`, `.<>`          |
| `.footnote`             | Selects a footnote node.                                        | None            | `.footnote`             |
| `.mdx_jsx_flow_element` | Selects an MDX JSX flow element node.                           | None            | `.mdx_jsx_flow_element` |
| `.list`,`.[]`           | Selects a list node with the specified index and checked state. | `indent`        | `.list(1)`, `.[1]`      |
| `.mdx_js_esm`           | Selects an MDX JS ESM node.                                     | None            | `.mdx_js_esm`           |
| `.toml`                 | Selects a TOML node.                                            | None            | `.toml`                 |
| `.yaml`                 | Selects a YAML node.                                            | None            | `.yaml`                 |
| `.break`                | Selects a break node.                                           | None            | `.break`                |
| `.mdx_text_expression`  | Selects an MDX text expression node.                            | None            | `.mdx_text_expression`  |
| `.footnote_ref`         | Selects a footnote reference node.                              | None            | `.footnote_ref`         |
| `.image_ref`            | Selects an image reference node.                                | None            | `.image_ref`            |
| `.mdx_jsx_text_element` | Selects an MDX JSX text element node.                           | None            | `.mdx_jsx_text_element` |
| `.math`                 | Selects a math node.                                            | None            | `.math`                 |
| `.math_inline`          | Selects a math inline node.                                     | None            | `.math_inline`          |
| `.mdx_flow_expression`  | Selects an MDX flow expression node.                            | None            | `.mdx_flow_expression`  |
| `.definition`           | Selects a definition node.                                      | None            | `.definition`           |
