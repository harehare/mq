# Builtin functions

| Function Name       | Description                                                                                 | Parameters                    | Example                                         |
| ------------------- | ------------------------------------------------------------------------------------------- | ----------------------------- | ----------------------------------------------- |
| `halt`              | Terminates the program with the given exit code.                                            | `exit_code`                   | `halt(0)`                                       |
| `debug`             | Prints the debug information of the given value.                                            | `value`                       | `debug("Hello, World!")`                        |
| `type`              | Returns the type of the given value.                                                        | `value`                       | `type(123)`                                     |
| `array`             | Creates an array from the given values.                                                     | `values`                      | `array(1, 2, 3)`                                |
| `from_date`         | Converts a date string to a timestamp.                                                      | `date_str`                    | `from_date("2023-01-01")`                       |
| `to_date`           | Converts a timestamp to a date string with the given format.                                | `timestamp`, `format`         | `to_date(1672531200000, "%Y-%m-%d")`            |
| `to_date_iso8601`   | Formats a date to ISO 8601 format (YYYY-MM-DDTHH:MM:SSZ).                                   | `timestamp`                   | `to_date_iso8601(1672531200000)`                |
| `now`               | Returns the current timestamp.                                                              | None                          | `now()`                                         |
| `base64`            | Encodes the given string to base64.                                                         | `input`                       | `base64("Hello, World!")`                       |
| `base64d`           | Decodes the given base64 string.                                                            | `input`                       | `base64d("SGVsbG8sIFdvcmxkIQ==")`               |
| `csv2table`         | Convert csv string to markdown table.                                                       | `csv`                         | `csv2table("a,b,c")`                            |
| `children`          | Retrieves a child element at the specified index from a markdown node.                      | `node`, `index`               | `children(self, 1)`                             |
| `gsub`              | Replaces all occurrences matching a regular expression pattern with the replacement string. | `pattern`, `from`, `to`       | `gsub("Hello, World!", "World", "Rust")`        |
| `min`               | Returns the minimum of two values.                                                          | `value1`, `value2`            | `min(1, 2)`                                     |
| `max`               | Returns the maximum of two values.                                                          | `value1`, `value2`            | `max(1, 2)`                                     |
| `to_html`           | Converts the given markdown string to HTML.                                                 | `markdown`                    | `to_html("# Hello")`                            |
| `to_csv`            | Converts the given value to a CSV.                                                          | `value`                       | `to_csv([1, 2, 3])`                             |
| `to_tsv`            | Converts the given value to a TSV.                                                          | `value`                       | `to_tsv([1, 2, 3])`                             |
| `to_string`         | Converts the given value to a string.                                                       | `value`                       | `to_string(123)`                                |
| `to_number`         | Converts the given value to a number.                                                       | `value`                       | `to_number("123")`                              |
| `url_encode`        | URL-encodes the given string.                                                               | `input`                       | `url_encode("Hello, World!")`                   |
| `to_text`           | Converts the given markdown node to plain text.                                             | `markdown`                    | `to_text("# Hello")`                            |
| `ends_with`         | Checks if the given string ends with the specified substring.                               | `string`, `substring`         | `ends_with("Hello, World!", "World!")`          |
| `starts_with`       | Checks if the given string starts with the specified substring.                             | `string`, `substring`         | `starts_with("Hello, World!", "Hello")`         |
| `match`             | Finds all matches of the given pattern in the string.                                       | `string`, `pattern`           | `match("Hello, World!", "o")`                   |
| `downcase`          | Converts the given string to lowercase.                                                     | `input`                       | `downcase("HELLO")`                             |
| `replace`           | Replaces all occurrences of a substring with another substring.                             | `string`, `from`, `to`        | `replace("Hello, World!", "World", "Rust")`     |
| `repeat`            | Repeats the given string a specified number of times.                                       | `string`, `count`             | `repeat("Hello", 3)`                            |
| `explode`           | Splits the given string into an array of characters.                                        | `string`                      | `explode("Hello")`                              |
| `implode`           | Joins an array of characters into a string.                                                 | `array`                       | `implode(['H', 'e', 'l', 'l', 'o'])`            |
| `trim`              | Trims whitespace from both ends of the given string.                                        | `input`                       | `trim(" Hello ")`                               |
| `upcase`            | Converts the given string to uppercase.                                                     | `input`                       | `upcase("hello")`                               |
| `update`            | Update the value with specified value.                                                      | `target_value`,`source_value` | `update("target_value", "source_value")`        |
| `slice`             | Extracts a substring from the given string.                                                 | `string`, `start`, `end`      | `slice("Hello, World!", 0, 5)`                  |
| `pow`               | Raises the base to the power of the exponent.                                               | `base`, `exponent`            | `pow(2, 3)`                                     |
| `index`             | Finds the first occurrence of a substring in the given string.                              | `string`, `substring`         | `index("Hello, World!", "World")`               |
| `len`               | Returns the length of the given string or array.                                            | `value`                       | `len("Hello")`                                  |
| `rindex`            | Finds the last occurrence of a substring in the given string.                               | `string`, `substring`         | `rindex("Hello, World!", "o")`                  |
| `nth`               | Gets the element at the specified index in the array or string.                             | `array_or_string`, `index`    | `nth([1, 2, 3], 1)`                             |
| `join`              | Joins the elements of an array into a string with the given separator.                      | `array`, `separator`          | `join([1, 2, 3], ",")`                          |
| `reverse`           | Reverses the given string or array.                                                         | `value`                       | `reverse("Hello")`                              |
| `sort`              | Sorts the elements of the given array.                                                      | `array`                       | `sort([3, 1, 2])`                               |
| `compact`           | Removes None values from the given array.                                                   | `array`                       | `compact([1, None, 2])`                         |
| `range`             | Creates an array of numbers within the specified range.                                     | `start`, `end`                | `range(1, 5)`                                   |
| `split`             | Splits the given string by the specified separator.                                         | `string`, `separator`         | `split("Hello, World!", ",")`                   |
| `uniq`              | Removes duplicate elements from the given array.                                            | `array`                       | `uniq([1, 2, 2, 3])`                            |
| `eq`                | Checks if two values are equal.                                                             | `value1`, `value2`            | `eq(1, 1)`                                      |
| `ne`                | Checks if two values are not equal.                                                         | `value1`, `value2`            | `ne(1, 2)`                                      |
| `gt`                | Checks if the first value is greater than the second value.                                 | `value1`, `value2`            | `gt(2, 1)`                                      |
| `gte`               | Checks if the first value is greater than or equal to the second value.                     | `value1`, `value2`            | `gte(2, 2)`                                     |
| `lt`                | Checks if the first value is less than the second value.                                    | `value1`, `value2`            | `lt(1, 2)`                                      |
| `lte`               | Checks if the first value is less than or equal to the second value.                        | `value1`, `value2`            | `lte(2, 2)`                                     |
| `add`               | Adds two values.                                                                            | `value1`, `value2`            | `add(1, 2)`                                     |
| `sub`               | Subtracts the second value from the first value.                                            | `value1`, `value2`            | `sub(2, 1)`                                     |
| `div`               | Divides the first value by the second value.                                                | `value1`, `value2`            | `div(4, 2)`                                     |
| `mul`               | Multiplies two values.                                                                      | `value1`, `value2`            | `mul(2, 3)`                                     |
| `mod`               | Calculates the remainder of the division of the first value by the second value.            | `value1`, `value2`            | `mod(5, 2)`                                     |
| `and`               | Performs a logical AND operation on two boolean values.                                     | `value1`, `value2`            | `and(true, false)`                              |
| `or`                | Performs a logical OR operation on two boolean values.                                      | `value1`, `value2`            | `or(true, false)`                               |
| `not`               | Performs a logical NOT operation on a boolean value.                                        | `value`                       | `not(true)`                                     |
| `round`             | Rounds the given number to the nearest integer.                                             | `number`                      | `round(1.5)`                                    |
| `trunc`             | Truncates the given number to an integer by removing the fractional part.                   | `number`                      | `trunc(1.5)`                                    |
| `ceil`              | Rounds the given number up to the nearest integer.                                          | `number`                      | `ceil(1.1)`                                     |
| `floor`             | Rounds the given number down to the nearest integer.                                        | `number`                      | `floor(1.9)`                                    |
| `del`               | Deletes the element at the specified index in the array or string.                          | `array_or_string`, `index`    | `del([1, 2, 3], 1)`                             |
| `abs`               | Returns the absolute value of the given number.                                             | `number`                      | `abs(-1)`                                       |
| `to_md_name`        | Returns the name of the given markdown node.                                                | `markdown`                    | `to_md_name(node)`                              |
| `to_md_text`        | Creates a markdown text node with the given value.                                          | `value`                       | `to_md_text("Hello")`                           |
| `to_image`          | Creates a markdown image node with the given URL, alt text, and title.                      | `url`, `alt`, `title`         | `to_image("url", "alt", "title")`               |
| `to_code`           | Creates a markdown code block with the given value and language.                            | `value`, `language`           | `to_code("code", "rust")`                       |
| `to_code_inline`    | Creates an inline markdown code node with the given value.                                  | `value`                       | `to_code_inline("code")`                        |
| `to_h`              | Creates a markdown heading node with the given value and depth.                             | `value`, `depth`              | `to_h("Heading", 1)`                            |
| `to_math`           | Creates a markdown math block with the given value.                                         | `value`                       | `to_math("E=mc^2")`                             |
| `to_math_inline`    | Creates an inline markdown math node with the given value.                                  | `value`                       | `to_math_inline("E=mc^2")`                      |
| `to_strong`         | Creates a markdown strong (bold) node with the given value.                                 | `value`                       | `to_strong("bold")`                             |
| `to_em`             | Creates a markdown emphasis (italic) node with the given value.                             | `value`                       | `to_em("italic")`                               |
| `to_hr`             | Creates a markdown horizontal rule node.                                                    | None                          | `to_hr()`                                       |
| `get_md_list_level` | Returns the indent level of a markdown list node.                                           | `list`                        | `get_md_list_level(list)`                       |
| `get_title`         | Returns the title of a markdown node.                                                       | `node`                        | `get_title(node)`                               |
| `to_md_list`        | Creates a markdown list node with the given value and indent level.                         | `value`, `indent`             | `to_md_list("item", 1)`                         |
| `to_md_table_row`   | Creates a markdown table row node with the given values.                                    | `cells`                       | `to_md_table_row("item", "item2", array(1, 2))` |
| `set_md_check`      | Creates a markdown list node with the given checked state.                                  | `list`, `checked`             | `set_md_check(list, true)`                      |
| `halt_error`        | Halts execution with error code 5                                                           | None                          | `halt_error()`                                  |
| `is_array`          | Checks if the input is an array                                                             | `a`                           | `is_array([1,2,3])`                             |
| `is_markdown`       | Checks if the input is markdown                                                             | `m`                           | `is_markdown(md"# Title")`                      |
| `is_bool`           | Checks if the input is a boolean                                                            | `b`                           | `is_bool(true)`                                 |
| `is_number`         | Checks if the input is a number                                                             | `n`                           | `is_number(42)`                                 |
| `is_string`         | Checks if the input is a string                                                             | `s`                           | `is_string("hello")`                            |
| `is_none`           | Checks if the input is None                                                                 | `n`                           | `is_none(None)`                                 |
| `contains`          | Checks if string contains a substring                                                       | `haystack`, `needle`          | `contains("hello", "ell")`                      |
| `ltrimstr`          | Removes prefix from string if it exists                                                     | `s`, `left`                   | `ltrimstr("hello", "he")`                       |
| `rtrimstr`          | Removes suffix from string if it exists                                                     | `s`, `right`                  | `rtrimstr("hello", "lo")`                       |
| `is_empty`          | Checks if string/array is empty                                                             | `s`                           | `is_empty("")`                                  |
| `test`              | Checks if string matches a regex pattern                                                    | `s`, `pattern`                | `test("abc", "a.c")`                            |
| `select`            | Returns v if f is true, otherwise None                                                      | `v`, `f`                      | `select(5, true)`                               |
| `arrays`            | Returns a if it's an array, else None                                                       | `a`                           | `arrays([1,2])`                                 |
| `markdowns`         | Returns m if it's markdown, else None                                                       | `m`                           | `markdowns(md"# Title")`                        |
| `booleans`          | Returns b if it's a boolean, else None                                                      | `b`                           | `booleans(true)`                                |
| `numbers`           | Returns n if it's a number, else None                                                       | `n`                           | `numbers(42)`                                   |
| `to_array`          | Converts input to array if not already                                                      | `a`                           | `to_array("hi")`                                |
| `map`               | Applies function to each array element                                                      | `v`, `f`                      | `map([1,2,3], (x) => mul(x, 2))`                |
| `filter`            | Filters array elements using function                                                       | `v`, `f`                      | `filter([1,2,3], (x) => gt(x, 1))`              |
| `first`             | Returns first element of array or None                                                      | `arr`                         | `first([1,2,3])`                                |
| `last`              | Returns last element of array or None                                                       | `arr`                         | `last([1,2,3])`                                 |
| `is_h`              | Checks if markdown is heading                                                               | `md`                          | `is_h(md"# Title")`                             |
| `is_h1`             | Checks if markdown is h1 heading                                                            | `md`                          | `is_h1(md"# Title")`                            |
| `is_h2`             | Checks if markdown is h2 heading                                                            | `md`                          | `is_h2(md"## Title")`                           |
| `is_h3`             | Checks if markdown is h3 heading                                                            | `md`                          | `is_h3(md"### Title")`                          |
| `is_h4`             | Checks if markdown is h4 heading                                                            | `md`                          | `is_h4(md"#### Title")`                         |
| `is_h5`             | Checks if markdown is h5 heading                                                            | `md`                          | `is_h5(md"##### Title")`                        |
| `is_em`             | Checks if markdown is emphasis                                                              | `md`                          | `is_em(md"*emphasis*")`                         |
| `is_html`           | Checks if markdown is html                                                                  | `md`                          | `is_html(md"<div>HTML</div>")`                  |
| `is_yaml`           | Checks if markdown is yaml                                                                  | `md`                          | `is_yaml(md"---\nkey: value\n---")`             |
| `is_toml`           | Checks if markdown is toml                                                                  | `md`                          | `is_toml(md"+++\nkey = 'value'\n+++")`          |
| `is_code`           | Checks if markdown is code block                                                            | `md`                          | ` is_code(md"```\ncode\n```") `                 |
| `is_list`           | Checks if markdown is list                                                                  | `md`                          | ` is_list(md"```- list```") `                   |
| `is_list1`          | Checks if markdown is list with indentation level 1                                         | `md`                          | ` is_list1(md"```- list```") `                  |
| `is_list2`          | Checks if markdown is list with indentation level 2                                         | `md`                          | ` is_list2(md"```- list```") `                  |
| `is_list3`          | Checks if markdown is list with indentation level 3                                         | `md`                          | ` is_list3(md"```- list```") `                  |
