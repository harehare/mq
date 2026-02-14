# mq Function Reference

- All function calls require parentheses `()`.
- If a function is called with missing arguments, the value passed through the pipe (`|`) will be used as the first argument.

## Table of Contents
- String Functions
- Array & Collection Functions
- Numeric Functions
- Dictionary Functions
- Markdown Creation & Manipulation Functions
- Type, I/O & Utility Functions

## String Functions

`upcase(input)`, `downcase(input)`, `split(s, sep)`, `join(arr, sep)`, `trim(input)`, `ltrimstr(s, prefix)`, `rtrimstr(s, suffix)`, `starts_with(s, prefix)`, `ends_with(s, suffix)`, `contains(haystack, needle)`, `index(s, sub)`, `rindex(s, sub)`, `slice(s, start, end)`, `replace(s, old, new)`, `gsub(s, pattern, rep)`, `regex_match(s, pat)`, `capture(s, pat)`, `repeat(s, n)`, `explode(s)`, `implode(arr)`, `url_encode(s)`, `base64(s)`, `base64d(s)`

## Array & Collection Functions

`len`, `reverse`, `sort`, `sort_by(arr, fn)`, `uniq`, `unique_by(arr, fn)`, `compact`, `flatten`, `first`, `last`, `min`, `max`, `group_by(arr, fn)`, `pluck(arr, key)`, `any(arr, fn)`, `all(arr, fn)`, `map(arr, fn)`, `filter(arr, fn)`, `fold(arr, init, fn)`, `select(condition)`, `range(start, end, step)`

## Numeric Functions

`add`, `sub`, `mul`, `div`, `mod`, `pow`, `abs`, `round`, `ceil`, `floor`, `trunc`, `negate`, `to_number`

## Dictionary Functions

`dict()`, `get(d, key)`, `set(d, key, val)`, `keys`, `values`, `entries`, `update(d1, d2)`

## Markdown Creation & Manipulation Functions

**Creation**: `to_h(text, depth)`, `to_code(text, lang)`, `to_code_inline(text)`, `to_link(url, text, title)`, `to_image(url, alt, title)`, `to_strong(text)`, `to_em(text)`, `to_hr()`, `to_math(text)`, `to_math_inline(text)`, `to_md_text(text)`, `to_md_list(val, level)`, `to_md_table_row(cells...)`, `to_md_table_cell(val, row, col)`

**Manipulation**: `set_attr(node, attr, val)`, `attr(node, attr)`, `set_check(list, checked)`, `set_ref(node, ref_id)`, `set_code_block_lang(code, lang)`, `set_list_ordered(list, ordered)`, `increase_header_level(h)`, `decrease_header_level(h)`, `to_text(node)`, `to_markdown_string(node)`, `to_html(node)`, `to_md_name(node)`

## Type, I/O & Utility Functions

**Type**: `type`, `to_string`, `to_number`, `to_array`, `is_none`, `is_empty`, `coalesce(a, b)`

**I/O**: `print`, `stderr`, `input`, `read_file(path)`

**Utility**: `identity`, `error(msg)`, `halt(code)`, `assert(a, b)`, `now`, `from_date(str)`, `to_date(ts, fmt)`, `all_symbols`

**Comparison**: `eq`, `ne`, `lt`, `lte`, `gt`, `gte`, `and`, `or`, `not`

**Modules**: `include "csv"`, `include "yaml"`, `include "fuzzy"`, `include "test"`

