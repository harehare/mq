# mq Reference

- All function calls require parentheses `()`.
- If a function is called with missing arguments, the piped value (`|`) is used as the first argument.

## Full CLI Options

```
mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl    Start interactive REPL session
```

| Flag                              | Purpose                                                                         |
| --------------------------------- | ------------------------------------------------------------------------------- |
| `-A, --aggregate`                 | Combine all inputs into single array                                            |
| `-I, --input-format`              | `markdown`, `mdx`, `html`, `text`, `null`, `raw`, `json`, `toml`, `yaml`, `xml`, `csv`, `tsv`, `psv`, `bytes`, `cbor` |
| `-F, --output-format`             | `markdown` (default), `html`, `text`, `json`, `table`, `grep`, `raw`, `none`   |
| `-U, --update`                    | Update file in place (aliases: `-i`, `--in-place`, `--inplace`)                 |
| `-f, --from-file`                 | Load query from `.mq` file                                                      |
| `-o, --output FILE`               | Write to output file                                                            |
| `-S, --separator QUERY`           | Insert query result between files                                               |
| `--args NAME VALUE`               | Set runtime variable (aliases: `--arg`, `--define`)                             |
| `--rawfile NAME FILE`             | Load file contents into variable                                                |
| `--stream`                        | Process line by line (streaming mode)                                           |
| `--unbuffered`                    | Unbuffered output                                                               |
| `-C, --color-output`              | Colorize output (supports JSON format too)                                      |
| `-P THRESHOLD`                    | Parallel processing threshold (default: 10)                                     |
| `-B, --before-context NUM`        | Show NUM nodes before each match (grep mode only)                               |
| `--after-context NUM`             | Show NUM nodes after each match (grep mode only)                                |
| `--context NUM`                   | Show NUM nodes before and after each match (grep mode only)                     |
| `--list-style`                    | List style: `dash` (default), `plus`, `star`                                    |
| `--link-title-style`              | Link title style: `double` (default), `single`, `paren`                         |
| `--link-url-style`                | Link URL style: `none` (default), `angle`                                       |
| `-L, --directory DIR`             | Search modules from directory                                                   |
| `-M, --module-names NAMES`        | Load additional modules from files                                              |
| `-m, --import-module-names NAMES` | Import modules as `name::fn()` in queries                                       |
| `--csv`, `--yaml`, `--toml`, `--xml` | Load built-in modules                                                        |
| `--list`                          | List all available subcommands (built-in and external)                          |

## Auto-Parsing by File Extension

When no `-I` flag is given, mq automatically imports based on file extension:

| Extension        | Behavior                                   |
| ---------------- | ------------------------------------------ |
| `.json`          | `import "json" \| json::json_parse()`      |
| `.yaml` / `.yml` | `import "yaml" \| yaml::yaml_parse()`      |
| `.toml`          | `import "toml" \| toml::toml_parse()`      |
| `.xml`           | `import "xml" \| xml::xml_parse()`         |
| `.csv`           | `import "csv" \| csv::csv_parse(true)`     |
| `.tsv`           | `import "csv" \| csv::tsv_parse(true)`     |
| `.psv`           | `import "csv" \| csv::psv_parse(true)`     |

Use `-I raw` to disable auto-parsing and receive the raw string.

## Full Attribute Reference

### Heading (`.h`)
| Attribute        | Type    | Description                |
| ---------------- | ------- | -------------------------- |
| `level`, `depth` | Integer | Heading level (1–6)        |
| `value`          | String  | Text content               |

### Code Block (`.code`)
| Attribute          | Type    | Description                 |
| ------------------ | ------- | --------------------------- |
| `lang`, `language` | String  | Language identifier         |
| `value`            | String  | Code content                |
| `meta`             | String  | Metadata string             |
| `fence`            | Boolean | Whether fenced              |

### Link (`.link`) / Image (`.image`)
| Attribute | Type   | `.link`      | `.image`      |
| --------- | ------ | ------------ | ------------- |
| `url`     | String | Link URL     | Image URL     |
| `title`   | String | Link title   | Image title   |
| `value`   | String | Link text    | —             |
| `alt`     | String | —            | Alt text      |

### List (`.list`)
| Attribute | Type    | Description            |
| --------- | ------- | ---------------------- |
| `index`   | Integer | Item index             |
| `level`   | Integer | Nesting level          |
| `ordered` | Boolean | Whether ordered        |
| `checked` | Boolean | Checkbox state         |
| `value`   | String  | Text content           |

### Table Cell (`.[row][col]`)
| Attribute               | Type    | Description                  |
| ----------------------- | ------- | ---------------------------- |
| `row`                   | Integer | Row number                   |
| `column`                | Integer | Column number                |
| `last_cell_in_row`      | Boolean | Last cell in row             |
| `last_cell_of_in_table` | Boolean | Last cell in table           |
| `value`                 | String  | Text content                 |

### Reference Nodes
| Node            | Attributes                       |
| --------------- | -------------------------------- |
| `.link_ref`     | `ident`, `label`                 |
| `.image_ref`    | `ident`, `label`, `alt`          |
| `.footnote_ref` | `ident`, `label`                 |
| `.footnote`     | `ident`, `text`                  |
| `.definition`   | `ident`, `url`, `title`, `label` |

### MDX Nodes
| Node                      | Attribute | Description      |
| ------------------------- | --------- | ---------------- |
| `.mdx_jsx_flow_element`   | `name`    | Element name     |
| `.mdx_flow_expression`    | `value`   | Expression value |

## Function Reference

### String
`upcase()`, `downcase()`, `split(s, sep)`, `join(arr, sep)`, `trim()`, `ltrimstr(s, prefix)`, `rtrimstr(s, suffix)`, `starts_with(s, prefix)`, `ends_with(s, suffix)`, `contains(haystack, needle)`, `index(s, sub)`, `rindex(s, sub)`, `slice(s, start, end)`, `replace(s, old, new)`, `gsub(s, pattern, rep)`, `regex_match(s, pat)`, `capture(s, pat)`, `repeat(s, n)`, `explode(s)`, `implode(arr)`, `url_encode(s)`, `base64(s)`, `base64d(s)`

### Array & Collection
`len`, `reverse`, `sort`, `sort_by(arr, fn)`, `uniq`, `unique_by(arr, fn)`, `compact`, `flatten`, `first`, `last`, `min`, `max`, `group_by(arr, fn)`, `pluck(arr, key)`, `any(arr, fn)`, `all(arr, fn)`, `map(arr, fn)`, `filter(arr, fn)`, `fold(arr, init, fn)`, `select(condition)`, `range(start, end, step)`

### Numeric
`add`, `sub`, `mul`, `div`, `mod`, `pow`, `abs`, `round`, `ceil`, `floor`, `trunc`, `negate`, `to_number`

### Bytes
`len`, `type`, `is_empty`, `==`, `base64(b)`, `base64d(s)` — byte sequences created with `b"..."` literals

### Dictionary
`dict()`, `get(d, key)`, `set(d, key, val)`, `keys`, `values`, `entries`, `update(d1, d2)`

### Markdown Creation
`to_h(text, depth)`, `to_code(text, lang)`, `to_code_inline(text)`, `to_link(url, text, title)`, `to_image(url, alt, title)`, `to_strong(text)`, `to_em(text)`, `to_hr()`, `to_math(text)`, `to_math_inline(text)`, `to_md_text(text)`, `to_md_list(val, level)`, `to_md_table_row(cells...)`, `to_md_table_cell(val, row, col)`

### Markdown Manipulation
`set_attr(node, attr, val)`, `attr(node, attr)`, `set_check(list, checked)`, `set_ref(node, ref_id)`, `set_code_block_lang(code, lang)`, `set_list_ordered(list, ordered)`, `increase_header_level(h)`, `decrease_header_level(h)`, `to_text(node)`, `to_markdown_string(node)`, `to_html(node)`, `to_md_name(node)`

### Type, I/O & Utility
**Type**: `type`, `to_string()`, `to_number()`, `to_array()`, `is_none()`, `is_empty()`, `coalesce(a, b)`

**I/O**: `print`, `stderr`, `input`, `read_file(path)`

**Utility**: `identity()`, `error(msg)`, `halt(code)`, `assert(a, b)`, `now`, `from_date(str)`, `to_date(ts, fmt)`, `all_symbols`

**Comparison**: `eq`, `ne`, `lt`, `lte`, `gt`, `gte`, `and`, `or`, `not`

**Modules**: `include "csv"`, `include "yaml"`, `include "fuzzy"`, `include "test"`

## Environment Variables

- `__FILE__` — full path to the file being processed
- `__FILE_NAME__` — filename without path
- `__FILE_STEM__` — filename without extension
