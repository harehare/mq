# Builtin functions
| Function Name | Description | Parameters | Example |
| --- | --- | --- | --- |
|`halt_error`| Halts execution with error code 5||halt_error()|
|`is_array`| Checks if input is an array|`a`|is_array(a)|
|`is_markdown`| Checks if input is markdown|`m`|is_markdown(m)|
|`is_bool`| Checks if input is a boolean|`b`|is_bool(b)|
|`is_number`| Checks if input is a number|`n`|is_number(n)|
|`is_string`| Checks if input is a string|`s`|is_string(s)|
|`is_none`| Checks if input is None|`n`|is_none(n)|
|`contains`| Checks if string contains a substring|`haystack`, `needle`|contains(haystack, needle)|
|`ltrimstr`| Removes prefix string from input if it exists|`s`, `left`|ltrimstr(s, left)|
|`rtrimstr`| Removes suffix string from input if it exists|`s`, `right`|rtrimstr(s, right)|
|`is_empty`| Checks if string or array is empty|`s`|is_empty(s)|
|`test`| Tests if string matches a pattern|`s`, `pattern`|test(s, pattern)|
|`select`| Returns value if condition is true, None otherwise|`v`, `f`|select(v, f)|
|`arrays`| Returns array if input is array, None otherwise|`a`|arrays(a)|
|`markdowns`| Returns markdown if input is markdown, None otherwise|`m`|markdowns(m)|
|`booleans`| Returns boolean if input is boolean, None otherwise|`b`|booleans(b)|
|`numbers`| Returns number if input is number, None otherwise|`n`|numbers(n)|
|`to_date_iso8601`| Formats a date to ISO 8601 format (YYYY-MM-DDTHH:MM:SSZ)|`d`|to_date_iso8601(d)|
|`to_array`| Converts input to an array|`a`|to_array(a)|
|`map`| Applies a given function to each element of the provided array and returns a new array with the results.|`v`, `f`|map(v, f)|
|`filter`| Filters the elements of an array based on a provided callback function.|`v`, `f`|filter(v, f)|
|`first`| Returns the first element of an array|`arr`|first(arr)|
|`last`| Returns the last element of an array|`arr`|last(arr)|
|`is_h`| Checks if markdown is heading|`md`|is_h(md)|
|`is_h1`| Checks if markdown is h1 heading|`md`|is_h1(md)|
|`is_h2`| Checks if markdown is h2 heading|`md`|is_h2(md)|
|`is_h3`| Checks if markdown is h3 heading|`md`|is_h3(md)|
|`is_h4`| Checks if markdown is h4 heading|`md`|is_h4(md)|
|`is_h5`| Checks if markdown is h5 heading|`md`|is_h5(md)|
|`is_em`| Checks if markdown is emphasis|`md`|is_em(md)|
|`is_html`| Checks if markdown is html|`md`|is_html(md)|
|`is_yaml`| Checks if markdown is yaml|`md`|is_yaml(md)|
|`is_toml`| Checks if markdown is toml|`md`|is_toml(md)|
|`is_code`| Checks if markdown is code block|`md`|is_code(md)|
|`is_text`| Checks if markdown is text|`text`|is_text(text)|
|`is_list`| Checks if markdown is list|`list`|is_list(list)|
|`is_mdx`| Checks if markdown is MDX|`mdx`|is_mdx(mdx)|
|`is_mdx_flow_expression`| Checks if markdown is MDX Flow Expression|`mdx`|is_mdx_flow_expression(mdx)|
|`is_mdx_jsx_flow_element`| Checks if markdown is MDX Jsx Flow Element|`mdx`|is_mdx_jsx_flow_element(mdx)|
|`is_mdx_jsx_text_element`| Checks if markdown is MDX Jsx Text Element|`mdx`|is_mdx_jsx_text_element(mdx)|
|`is_mdx_text_expression`| Checks if markdown is MDX Text Expression|`mdx`|is_mdx_text_expression(mdx)|
|`is_mdx_js_esm`| Checks if markdown is MDX Js Esm|`mdx`|is_mdx_js_esm(mdx)|
|`is_list1`| Checks if markdown is list with indentation level 1|`list`|is_list1(list)|
|`is_list2`| Checks if markdown is list with indentation level 2|`list`|is_list2(list)|
|`is_list3`| Checks if markdown is list with indentation level 3|`list`|is_list3(list)|
|`csv2table`| Convert csv string to markdown table|`csv`|csv2table(csv)|
|`tsv2table`| Convert tsv string to markdown table|`tsv`|tsv2table(tsv)|
|`to_math_inline`|Creates an inline markdown math node with the given value.|`value`|to_math_inline(value)|
|`explode`|Splits the given string into an array of characters.|`string`|explode(string)|
|`to_html`|Converts the given markdown string to HTML.|`markdown`|to_html(markdown)|
|`len`|Returns the length of the given string or array.|`value`|len(value)|
|`base64`|Encodes the given string to base64.|`input`|base64(input)|
|`gt`|Checks if the first value is greater than the second value.|`value1`, `value2`|gt(value1, value2)|
|`gte`|Checks if the first value is greater than or equal to the second value.|`value1`, `value2`|gte(value1, value2)|
|`update`|Update the value with specified value.|`target_value`, `source_value`|update(target_value, source_value)|
|`add`|Adds two values.|`value1`, `value2`|add(value1, value2)|
|`to_text`|Converts the given markdown node to plain text.|`markdown`|to_text(markdown)|
|`not`|Performs a logical NOT operation on a boolean value.|`value`|not(value)|
|`trim`|Trims whitespace from both ends of the given string.|`input`|trim(input)|
|`replace`|Replaces all occurrences of a substring with another substring.|`string`, `from`, `to`|replace(string, from, to)|
|`or`|Performs a logical OR operation on two boolean values.|`value1`, `value2`|or(value1, value2)|
|`from_date`|Converts a date string to a timestamp.|`date_str`|from_date(date_str)|
|`slice`|Extracts a substring from the given string.|`string`, `start`, `end`|slice(string, start, end)|
|`ne`|Checks if two values are not equal.|`value1`, `value2`|ne(value1, value2)|
|`lt`|Checks if the first value is less than the second value.|`value1`, `value2`|lt(value1, value2)|
|`min`|Returns the minimum of two values.|`value1`, `value2`|min(value1, value2)|
|`max`|Returns the maximum of two values.|`value1`, `value2`|max(value1, value2)|
|`compact`|Removes None values from the given array.|`array`|compact(array)|
|`div`|Divides the first value by the second value.|`value1`, `value2`|div(value1, value2)|
|`mod`|Calculates the remainder of the division of the first value by the second value.|`value1`, `value2`|mod(value1, value2)|
|`lte`|Checks if the first value is less than or equal to the second value.|`value1`, `value2`|lte(value1, value2)|
|`sort`|Sorts the elements of the given array.|`array`|sort(array)|
|`mul`|Multiplies two values.|`value1`, `value2`|mul(value1, value2)|
|`sub`|Subtracts the second value from the first value.|`value1`, `value2`|sub(value1, value2)|
|`nth`|Gets the element at the specified index in the array or string.|`array_or_string`, `index`|nth(array_or_string, index)|
|`rindex`|Finds the last occurrence of a substring in the given string.|`string`, `substring`|rindex(string, substring)|
|`to_csv`|Converts the given value to a CSV.|`value`|to_csv(value)|
|`to_image`|Creates a markdown image node with the given URL, alt text, and title.|`url`, `alt`, `title`|to_image(url, alt, title)|
|`to_math`|Creates a markdown math block with the given value.|`value`|to_math(value)|
|`array`|Creates an array from the given values.|`values`|array(values)|
|`abs`|Returns the absolute value of the given number.|`number`|abs(number)|
|`to_link`|Creates a markdown link node  with the given  url and title.|`url`, `value`, `title`|to_link(url, value, title)|
|`downcase`|Converts the given string to lowercase.|`input`|downcase(input)|
|`to_code_inline`|Creates an inline markdown code node with the given value.|`value`|to_code_inline(value)|
|`ends_with`|Checks if the given string ends with the specified substring.|`string`, `substring`|ends_with(string, substring)|
|`and`|Performs a logical AND operation on two boolean values.|`value1`, `value2`|and(value1, value2)|
|`starts_with`|Checks if the given string starts with the specified substring.|`string`, `substring`|starts_with(string, substring)|
|`to_number`|Converts the given value to a number.|`value`|to_number(value)|
|`to_md_text`|Creates a markdown text node with the given value.|`value`|to_md_text(value)|
|`trunc`|Truncates the given number to an integer by removing the fractional part.|`number`|trunc(number)|
|`uniq`|Removes duplicate elements from the given array.|`array`|uniq(array)|
|`repeat`|Repeats the given string a specified number of times.|`string`, `count`|repeat(string, count)|
|`halt`|Terminates the program with the given exit code.|`exit_code`|halt(exit_code)|
|`upcase`|Converts the given string to uppercase.|`input`|upcase(input)|
|`get_title`|Returns the title of a markdown node.|`node`|get_title(node)|
|`to_em`|Creates a markdown emphasis (italic) node with the given value.|`value`|to_em(value)|
|`match`|Finds all matches of the given pattern in the string.|`string`, `pattern`|match(string, pattern)|
|`to_hr`|Creates a markdown horizontal rule node.||to_hr()|
|`to_date`|Converts a timestamp to a date string with the given format.|`timestamp`, `format`|to_date(timestamp, format)|
|`gsub`|Replaces all occurrences matching a regular expression pattern with the replacement string.|`pattern`, `from`, `to`|gsub(pattern, from, to)|
|`to_string`|Converts the given value to a string.|`value`|to_string(value)|
|`round`|Rounds the given number to the nearest integer.|`number`|round(number)|
|`url_encode`|URL-encodes the given string.|`input`|url_encode(input)|
|`implode`|Joins an array of characters into a string.|`array`|implode(array)|
|`debug`|Prints the debug information of the given value.|`value`|debug(value)|
|`to_md_name`|Returns the name of the given markdown node.|`markdown`|to_md_name(markdown)|
|`floor`|Rounds the given number down to the nearest integer.|`number`|floor(number)|
|`type`|Returns the type of the given value.|`value`|type(value)|
|`to_h`|Creates a markdown heading node with the given value and depth.|`value`, `depth`|to_h(value, depth)|
|`to_strong`|Creates a markdown strong (bold) node with the given value.|`value`|to_strong(value)|
|`to_md_list`|Creates a markdown list node with the given value and indent level.|`value`, `indent`|to_md_list(value, indent)|
|`index`|Finds the first occurrence of a substring in the given string.|`string`, `substring`|index(string, substring)|
|`to_md_table_row`|Creates a markdown table row node with the given values.|`cells`|to_md_table_row(cells)|
|`to_code`|Creates a markdown code block with the given value and language.|`value`, `language`|to_code(value, language)|
|`base64d`|Decodes the given base64 string.|`input`|base64d(input)|
|`reverse`|Reverses the given string or array.|`value`|reverse(value)|
|`get_md_list_level`|Returns the indent level of a markdown list node.|`list`|get_md_list_level(list)|
|`now`|Returns the current timestamp.||now()|
|`range`|Creates an array of numbers within the specified range.|`start`, `end`|range(start, end)|
|`split`|Splits the given string by the specified separator.|`string`, `separator`|split(string, separator)|
|`to_tsv`|Converts the given value to a TSV.|`value`|to_tsv(value)|
|`eq`|Checks if two values are equal.|`value1`, `value2`|eq(value1, value2)|
|`ceil`|Rounds the given number up to the nearest integer.|`number`|ceil(number)|
|`pow`|Raises the base to the power of the exponent.|`base`, `exponent`|pow(base, exponent)|
|`join`|Joins the elements of an array into a string with the given separator.|`array`, `separator`|join(array, separator)|
|`del`|Deletes the element at the specified index in the array or string.|`array_or_string`, `index`|del(array_or_string, index)|
|`set_md_check`|Creates a markdown list node with the given checked state.|`list`, `checked`|set_md_check(list, checked)|
