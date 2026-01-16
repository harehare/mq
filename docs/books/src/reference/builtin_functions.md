# Builtin functions
| Function Name | Description | Parameters | Example |
| --- | --- | --- | --- |
| `abs` | Returns the absolute value of the given number. | `number` | abs(number) |
| `add` | Adds two values. | `value1`, `value2` | add(value1, value2) |
| `all` |  Returns true if all element in the array satisfies the provided function. | `v`, `f` | all(v, f) |
| `all_symbols` | Returns an array of all interned symbols. |  | all_symbols() |
| `and` | Performs a logical AND operation on two boolean values. | `value1`, `value2` | and(value1, value2) |
| `any` |  Returns true if any element in the array satisfies the provided function. | `v`, `f` | any(v, f) |
| `array` | Creates an array from the given values. | `values` | array(values) |
| `arrays` |  Returns array if input is array, None otherwise | `a` | arrays(a) |
| `assert` | Asserts that two values are equal, returns the value if true, otherwise raises an error. | `value1`, `value2` | assert(value1, value2) |
| `attr` | Retrieves the value of the specified attribute from a markdown node. | `markdown`, `attribute` | attr(markdown, attribute) |
| `base64` | Encodes the given string to base64. | `input` | base64(input) |
| `base64d` | Decodes the given base64 string. | `input` | base64d(input) |
| `between` |  Checks if a value is between min and max (inclusive). | `value`, `min`, `max` | between(value, min, max) |
| `booleans` |  Returns boolean if input is boolean, None otherwise | `b` | booleans(b) |
| `breakpoint` | Sets a breakpoint for debugging; execution will pause at this point if a debugger is attached. |  | breakpoint() |
| `ceil` | Rounds the given number up to the nearest integer. | `number` | ceil(number) |
| `coalesce` | Returns the first non-None value from the two provided arguments. | `value1`, `value2` | coalesce(value1, value2) |
| `compact` | Removes None values from the given array. | `array` | compact(array) |
| `compact_map` |  Maps over an array and removes None values from the result. | `arr`, `f` | compact_map(arr, f) |
| `contains` |  Checks if string contains a substring | `haystack`, `needle` | contains(haystack, needle) |
| `count_by` |  Returns the count of elements in the array that satisfy the provided function. | `arr`, `f` | count_by(arr, f) |
| `debug` |  Prints the debug information of the given value. | `msg` | debug(msg) |
| `decrease_header_level` | Decreases the level of a markdown heading node by one, down to a minimum of 1. | `heading_node` | decrease_header_level(heading_node) |
| `default_to` |  Returns a default value if the input is None or empty. | `value`, `default` | default_to(value, default) |
| `del` | Deletes the element at the specified index in the array or string. | `array_or_string`, `index` | del(array_or_string, index) |
| `dict` | Creates a new, empty dict. |  | dict() |
| `div` | Divides the first value by the second value. | `value1`, `value2` | div(value1, value2) |
| `downcase` | Converts the given string to lowercase. | `input` | downcase(input) |
| `ends_with` | Checks if the given string ends with the specified substring. | `string`, `substring` | ends_with(string, substring) |
| `entries` | Returns an array of key-value pairs from the dict as arrays. | `dict` | entries(dict) |
| `eq` | Checks if two values are equal. | `value1`, `value2` | eq(value1, value2) |
| `error` | Raises a user-defined error with the specified message. | `message` | error(message) |
| `explode` | Splits the given string into an array of characters. | `string` | explode(string) |
| `fill` |  Returns an array of length n filled with the given value. | `value`, `n` | fill(value, n) |
| `filter` |  Filters the elements of an array based on a provided callback function. | `v`, `f` | filter(v, f) |
| `find_index` |  Returns the index of the first element in an array that satisfies the provided function. | `arr`, `f` | find_index(arr, f) |
| `first` |  Returns the first element of an array | `arr` | first(arr) |
| `flat_map` |  Applies a function to each element and flattens the result into a single array | `v`, `f` | flat_map(v, f) |
| `flatten` | Flattens a nested array into a single level array. | `array` | flatten(array) |
| `floor` | Rounds the given number down to the nearest integer. | `number` | floor(number) |
| `fold` |  Reduces an array to a single value by applying a function, starting from an initial value. | `arr`, `init`, `f` | fold(arr, init, f) |
| `from_date` | Converts a date string to a timestamp. | `date_str` | from_date(date_str) |
| `get` | Retrieves a value from a dict by its key. Returns None if the key is not found. | `obj`, `key` | get(obj, key) |
| `get_args` |  Gets the arguments of an AST node | `node` | get_args(node) |
| `get_or` |  Safely gets a value from a dict with a default if the key doesn't exist. | `dict`, `key`, `default` | get_or(dict, key, default) |
| `get_title` | Returns the title of a markdown node. | `node` | get_title(node) |
| `get_url` | Returns the url of a markdown node. | `node` | get_url(node) |
| `get_variable` | Retrieves the value of a symbol or variable from the current environment. | `symbol_or_string` | get_variable(symbol_or_string) |
| `group_by` |  Groups elements of an array by the result of applying a function to each element | `arr`, `f` | group_by(arr, f) |
| `gsub` | Replaces all occurrences matching a regular expression pattern with the replacement string. | `from`, `pattern`, `to` | gsub(from, pattern, to) |
| `gt` | Checks if the first value is greater than the second value. | `value1`, `value2` | gt(value1, value2) |
| `gte` | Checks if the first value is greater than or equal to the second value. | `value1`, `value2` | gte(value1, value2) |
| `halt` | Terminates the program with the given exit code. | `exit_code` | halt(exit_code) |
| `halt_error` |  Halts execution with error code 5 |  | halt_error() |
| `identity` |  Returns the input value unchanged. | `x` | identity(x) |
| `implode` | Joins an array of characters into a string. | `array` | implode(array) |
| `in` |  Returns true if the element is in the array. | `v`, `elem` | in(v, elem) |
| `increase_header_level` | Increases the level of a markdown heading node by one, up to a maximum of 6. | `heading_node` | increase_header_level(heading_node) |
| `index` | Finds the first occurrence of a substring in the given string. | `string`, `substring` | index(string, substring) |
| `index_by` |  Creates a dictionary indexed by a key extracted from each element. | `arr`, `f` | index_by(arr, f) |
| `infinite` | Returns an infinite number value. |  | infinite() |
| `input` | Reads a line from standard input and returns it as a string. |  | input() |
| `insert` | Inserts a value into an array or string at the specified index, or into a dict with the specified key. | `target`, `index_or_key`, `value` | insert(target, index_or_key, value) |
| `inspect` |  Inspects a value by printing its string representation and returning the value. | `value` | inspect(value) |
| `intern` | Interns the given string, returning a canonical reference for efficient comparison. | `string` | intern(string) |
| `is_array` |  Checks if input is an array | `a` | is_array(a) |
| `is_bool` |  Checks if input is a boolean | `b` | is_bool(b) |
| `is_code` |  Checks if markdown is code block | `md` | is_code(md) |
| `is_debug_mode` | Checks if the runtime is currently in debug mode, returning true if a debugger is attached. |  | is_debug_mode() |
| `is_dict` |  Checks if input is a dictionary | `d` | is_dict(d) |
| `is_em` |  Checks if markdown is emphasis | `md` | is_em(md) |
| `is_empty` |  Checks if string, array or dict is empty | `s` | is_empty(s) |
| `is_h` |  Checks if markdown is heading | `md` | is_h(md) |
| `is_h1` |  Checks if markdown is h1 heading | `md` | is_h1(md) |
| `is_h2` |  Checks if markdown is h2 heading | `md` | is_h2(md) |
| `is_h3` |  Checks if markdown is h3 heading | `md` | is_h3(md) |
| `is_h4` |  Checks if markdown is h4 heading | `md` | is_h4(md) |
| `is_h5` |  Checks if markdown is h5 heading | `md` | is_h5(md) |
| `is_h6` |  Checks if markdown is h6 heading | `md` | is_h6(md) |
| `is_h_level` |  Checks if markdown is a heading of the specified level (1-6) | `md`, `level` | is_h_level(md, level) |
| `is_html` |  Checks if markdown is html | `md` | is_html(md) |
| `is_list` |  Checks if markdown is list | `list` | is_list(list) |
| `is_markdown` |  Checks if input is markdown | `m` | is_markdown(m) |
| `is_mdx` |  Checks if markdown is MDX | `mdx` | is_mdx(mdx) |
| `is_mdx_flow_expression` |  Checks if markdown is MDX Flow Expression | `mdx` | is_mdx_flow_expression(mdx) |
| `is_mdx_js_esm` |  Checks if markdown is MDX Js Esm | `mdx` | is_mdx_js_esm(mdx) |
| `is_mdx_jsx_flow_element` |  Checks if markdown is MDX Jsx Flow Element | `mdx` | is_mdx_jsx_flow_element(mdx) |
| `is_mdx_jsx_text_element` |  Checks if markdown is MDX Jsx Text Element | `mdx` | is_mdx_jsx_text_element(mdx) |
| `is_mdx_text_expression` |  Checks if markdown is MDX Text Expression | `mdx` | is_mdx_text_expression(mdx) |
| `is_none` |  Checks if input is None | `n` | is_none(n) |
| `is_number` |  Checks if input is a number | `n` | is_number(n) |
| `is_string` |  Checks if input is a string | `s` | is_string(s) |
| `is_table_header` |  Checks if markdown is table header | `md` | is_table_header(md) |
| `is_text` |  Checks if markdown is text | `text` | is_text(text) |
| `is_toml` |  Checks if markdown is toml | `md` | is_toml(md) |
| `is_yaml` |  Checks if markdown is yaml | `md` | is_yaml(md) |
| `join` | Joins the elements of an array into a string with the given separator. | `array`, `separator` | join(array, separator) |
| `keys` | Returns an array of keys from the dict. | `dict` | keys(dict) |
| `last` |  Returns the last element of an array | `arr` | last(arr) |
| `len` | Returns the length of the given string or array. | `value` | len(value) |
| `lt` | Checks if the first value is less than the second value. | `value1`, `value2` | lt(value1, value2) |
| `lte` | Checks if the first value is less than or equal to the second value. | `value1`, `value2` | lte(value1, value2) |
| `ltrimstr` |  Removes prefix string from input if it exists | `s`, `left` | ltrimstr(s, left) |
| `map` |  Applies a given function to each element of the provided array and returns a new array with the results. | `v`, `f` | map(v, f) |
| `markdowns` |  Returns markdown if input is markdown, None otherwise | `m` | markdowns(m) |
| `matches_url` |  Checks if markdown node's URL matches a specified URL | `node`, `url` | matches_url(node, url) |
| `max` | Returns the maximum of two values. | `value1`, `value2` | max(value1, value2) |
| `min` | Returns the minimum of two values. | `value1`, `value2` | min(value1, value2) |
| `mod` | Calculates the remainder of the division of the first value by the second value. | `value1`, `value2` | mod(value1, value2) |
| `mul` | Multiplies two values. | `value1`, `value2` | mul(value1, value2) |
| `nan` | Returns a Not-a-Number (NaN) value. |  | nan() |
| `ne` | Checks if two values are not equal. | `value1`, `value2` | ne(value1, value2) |
| `negate` | Returns the negation of the given number. | `number` | negate(number) |
| `not` | Performs a logical NOT operation on a boolean value. | `value` | not(value) |
| `now` | Returns the current timestamp. |  | now() |
| `numbers` |  Returns number if input is number, None otherwise | `n` | numbers(n) |
| `or` | Performs a logical OR operation on two boolean values. | `value1`, `value2` | or(value1, value2) |
| `partition` |  Splits an array into two arrays: [matching, not_matching] based on a condition. | `arr`, `f` | partition(arr, f) |
| `pluck` |  Extracts values from an array of objects based on a specified selector. | `pluck_obj`, `selector` | pluck(pluck_obj, selector) |
| `pow` | Raises the base to the power of the exponent. | `base`, `exponent` | pow(base, exponent) |
| `print` | Prints a message to standard output and returns the current value. | `message` | print(message) |
| `range` | Creates an array from start to end with an optional step. | `start`, `end`, `step` | range(start, end, step) |
| `read_file` | Reads the contents of a file at the given path and returns it as a string. | `path` | read_file(path) |
| `regex_match` | Finds all matches of the given pattern in the string. | `string`, `pattern` | regex_match(string, pattern) |
| `reject` |  Filters out elements that match the condition (opposite of filter). | `arr`, `f` | reject(arr, f) |
| `repeat` | Repeats the given string a specified number of times. | `string`, `count` | repeat(string, count) |
| `replace` | Replaces all occurrences of a substring with another substring. | `from`, `pattern`, `to` | replace(from, pattern, to) |
| `reverse` | Reverses the given string or array. | `value` | reverse(value) |
| `rindex` | Finds the last occurrence of a substring in the given string. | `string`, `substring` | rindex(string, substring) |
| `round` | Rounds the given number to the nearest integer. | `number` | round(number) |
| `rtrimstr` |  Removes suffix string from input if it exists | `s`, `right` | rtrimstr(s, right) |
| `second` |  Returns the second element of an array | `arr` | second(arr) |
| ~~`sections`~~ |  deprecated: Use the section module instead of the sections function. |
|  Returns an array of sections, each section is an array of markdown nodes between the specified header and the next header of the same level. | `md_nodes`, `level` | sections(md_nodes, level) |
| `select` |  Returns value if condition is true, None otherwise | `v`, `f` | select(v, f) |
| `set` | Sets a key-value pair in a dict. If the key exists, its value is updated. Returns the modified map. | `obj`, `key`, `value` | set(obj, key, value) |
| `set_attr` | Sets the value of the specified attribute on a markdown node. | `markdown`, `attribute`, `value` | set_attr(markdown, attribute, value) |
| `set_check` | Creates a markdown list node with the given checked state. | `list`, `checked` | set_check(list, checked) |
| `set_code_block_lang` | Sets the language of a markdown code block node. | `code_block`, `language` | set_code_block_lang(code_block, language) |
| `set_list_ordered` | Sets the ordered property of a markdown list node. | `list`, `ordered` | set_list_ordered(list, ordered) |
| `set_ref` | Sets the reference identifier for markdown nodes that support references (e.g., Definition, LinkRef, ImageRef, Footnote, FootnoteRef). | `node`, `reference_id` | set_ref(node, reference_id) |
| `set_variable` | Sets a symbol or variable in the current environment with the given value. | `symbol_or_string`, `value` | set_variable(symbol_or_string, value) |
| `skip` |  Skips the first n elements of an array and returns the rest | `arr`, `n` | skip(arr, n) |
| `skip_while` |  Skips elements from the beginning of an array while the provided function returns true | `arr`, `f` | skip_while(arr, f) |
| `slice` | Extracts a substring from the given string. | `string`, `start`, `end` | slice(string, start, end) |
| `sort` | Sorts the elements of the given array. | `array` | sort(array) |
| `sort_by` |  Sorts an array using a key function that extracts a comparable value for each element. | `arr`, `f` | sort_by(arr, f) |
| `split` | Splits the given string by the specified separator. | `string`, `separator` | split(string, separator) |
| `starts_with` | Checks if the given string starts with the specified substring. | `string`, `substring` | starts_with(string, substring) |
| `stderr` | Prints a message to standard error and returns the current value. | `message` | stderr(message) |
| `sub` | Subtracts the second value from the first value. | `value1`, `value2` | sub(value1, value2) |
| `sum_by` |  Sums elements of an array after applying a transformation function. | `arr`, `f` | sum_by(arr, f) |
| `take` |  Takes the first n elements of an array | `arr`, `n` | take(arr, n) |
| `take_while` |  Takes elements from the beginning of an array while the provided function returns true | `arr`, `f` | take_while(arr, f) |
| `tap` |  Applies a function to a value and returns the value (useful for debugging or side effects). | `tap_value`, `tap_expr` | tap(tap_value, tap_expr) |
| `test` |  Tests if string matches a pattern | `s`, `pattern` | test(s, pattern) |
| `times` |  Executes an expression n times and returns an array of results. | `t_n`, `t_expr` | times(t_n, t_expr) |
| `to_array` | Converts the given value to an array. | `value` | to_array(value) |
| `to_code` |  Converts an AST node back to code | `node` | to_code(node) |
| `to_code` | Creates a markdown code block with the given value and language. | `value`, `language` | to_code(value, language) |
| `to_code_inline` | Creates an inline markdown code node with the given value. | `value` | to_code_inline(value) |
| `to_csv` |  Converts the given value to a CSV. | `v` | to_csv(v) |
| `to_date` | Converts a timestamp to a date string with the given format. | `timestamp`, `format` | to_date(timestamp, format) |
| `to_date_iso8601` |  Formats a date to ISO 8601 format (YYYY-MM-DDTHH:MM:SSZ) | `d` | to_date_iso8601(d) |
| `to_em` | Creates a markdown emphasis (italic) node with the given value. | `value` | to_em(value) |
| `to_h` | Creates a markdown heading node with the given value and depth. | `value`, `depth` | to_h(value, depth) |
| `to_hr` | Creates a markdown horizontal rule node. |  | to_hr() |
| `to_html` | Converts the given markdown string to HTML. | `markdown` | to_html(markdown) |
| `to_image` | Creates a markdown image node with the given URL, alt text, and title. | `url`, `alt`, `title` | to_image(url, alt, title) |
| `to_link` | Creates a markdown link node  with the given  url and title. | `url`, `value`, `title` | to_link(url, value, title) |
| `to_markdown` | Parses a markdown string and returns an array of markdown nodes. | `markdown_string` | to_markdown(markdown_string) |
| `to_markdown_string` | Converts the given value(s) to a markdown string representation. | `value` | to_markdown_string(value) |
| `to_math` | Creates a markdown math block with the given value. | `value` | to_math(value) |
| `to_math_inline` | Creates an inline markdown math node with the given value. | `value` | to_math_inline(value) |
| `to_md_list` | Creates a markdown list node with the given value and indent level. | `value`, `indent` | to_md_list(value, indent) |
| `to_md_name` | Returns the name of the given markdown node. | `markdown` | to_md_name(markdown) |
| `to_md_table_row` | Creates a markdown table row node with the given values. | `cells` | to_md_table_row(cells) |
| `to_md_text` | Creates a markdown text node with the given value. | `value` | to_md_text(value) |
| `to_mdx` | Parses an MDX string and returns an array of MDX nodes. | `mdx_string` | to_mdx(mdx_string) |
| `to_number` | Converts the given value to a number. | `value` | to_number(value) |
| `to_string` | Converts the given value to a string. | `value` | to_string(value) |
| `to_strong` | Creates a markdown strong (bold) node with the given value. | `value` | to_strong(value) |
| `to_text` | Converts the given markdown node to plain text. | `markdown` | to_text(markdown) |
| `to_tsv` |  Converts the given value to a TSV. | `v` | to_tsv(v) |
| `transpose` |  Transposes a 2D array (matrix), swapping rows and columns. | `matrix` | transpose(matrix) |
| `trim` | Trims whitespace from both ends of the given string. | `input` | trim(input) |
| `trunc` | Truncates the given number to an integer by removing the fractional part. | `number` | trunc(number) |
| `type` | Returns the type of the given value. | `value` | type(value) |
| `uniq` | Removes duplicate elements from the given array. | `array` | uniq(array) |
| `unique_by` |  Returns a new array with duplicate elements removed, comparing by the result of the provided function. | `arr`, `f` | unique_by(arr, f) |
| `unless` |  Executes the expression only if the condition is false. | `unless_cond`, `unless_expr` | unless(unless_cond, unless_expr) |
| `until` |  Executes the expression repeatedly until the condition is true. | `until_cond`, `until_expr` | until(until_cond, until_expr) |
| `upcase` | Converts the given string to uppercase. | `input` | upcase(input) |
| `update` | Update the value with specified value. | `target_value`, `source_value` | update(target_value, source_value) |
| `url_encode` | URL-encodes the given string. | `input` | url_encode(input) |
| `values` | Returns an array of values from the dict. | `dict` | values(dict) |
