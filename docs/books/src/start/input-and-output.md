# Input and Output Behavior

How mq reads Markdown and writes it back, and the options that change it. For the full list of flags, see the [CLI reference](../reference/cli.md).

## Directory input

Passing a directory reads every `.md`, `.markdown` and `.mdx` file under it, recursively and in path order. Hidden and `.gitignore`d entries are skipped, and it is an error if no Markdown file is found.

```sh
mq '.h1' docs/
```

For other file types or custom filtering, use a shell glob or `walk_files`.

## Output formats

`-F` / `--output-format` selects how results are written. The default is `markdown`. Each result is a Markdown node, a string, a number, a boolean, `None`, an array or a dict. The examples use `-I null` to show non-Markdown values.

| Format | Writes | Separator between results |
| ------ | ------ | ------------------------- |
| `markdown` | Markdown source of the nodes. Other values are written as their text | Block spacing of the Markdown renderer |
| `text` | Plain text of the nodes, with formatting removed | Each node ends with a newline, so empty nodes leave blank lines |
| `html` | [HTML](#html-output) | Newline |
| `json` | A JSON array of the results. A single non-Markdown result is written as itself | None, one document |
| `jsonl` | One JSON value per line | Newline |
| `table` | A boxed table of the results, nested values as nested tables | None, one table |
| `grep` | `file:line:` followed by the node for Markdown, `path: value` for other values | Newline |
| `gron` | One `json.path = value;` assignment per line | Newline |
| `raw` | The string form of each value, byte for byte | Newline after each value, except `Bytes` |
| `csv` | A header row, then one row per result | Newline |
| `toml` | A TOML document | None, one document |
| `toon` | [TOON](https://github.com/toon-format/toon) | None, one document |
| `xml` | An XML document with a `root` element | None, one document |
| `yaml` | A YAML document | None, one document |
| `shell` | `name=value` lines for `eval` | Newline |
| `none` | Nothing. Use it for side effects such as `write_file` | |

### Scalars, `None` and nested values

`None` is written as `null` by `json`, `jsonl`, `gron` and `toon`, as `~` by `yaml`, as an empty element by `xml`, and as an empty string by `markdown`, `text`, `html`, `raw`, `grep`, `csv` and `shell`.

Arrays and dicts are written as JSON-like text by `markdown`, `text`, `html` and `raw`. `json`, `jsonl`, `toml`, `toon`, `xml` and `yaml` keep the structure. `csv` and `table` use dict keys as columns, and `csv` writes a nested value as a JSON string in one cell. `gron` and `shell` flatten the structure into paths (`json.l[0]` and `MQ_l_0`).

```sh
$ mq -I null -F json '{"k": "v", "n": None, "l": [1, 2]}'
{
  "k": "v",
  "n": null,
  "l": [
    1,
    2
  ]
}
$ mq -I null -F yaml '{"k": "v", "n": None, "l": [1, 2]}'
k: v
"n": ~
l:
  - 1
  - 2
$ mq -I null -F gron '{"k": "v", "l": [1, 2]}'
json = {};
json.k = "v";
json.l = [];
json.l[0] = 1;
json.l[1] = 2;
$ mq -I null -F shell '{"k": "v", "l": [1, 2]}'
MQ_k=v
MQ_l_0=1
MQ_l_1=2
```

`-F shell` prefixes every name with `MQ_` (change it with `--shell-prefix`), so untrusted keys such as `PATH` or `IFS` cannot overwrite the environment when the output is passed to `eval`. Two keys that flatten to the same name (`a b` and `a-b`) are an error. An empty `--shell-prefix` is rejected. Values are always quoted, so `eval "$(mq -F shell ...)"` is safe for untrusted input.

`toml` needs a dict at the top level and fails on anything else, because a TOML document is a table.

### `raw`

`-F raw` is the equivalent of `jq -r`. A string is written without quotes, and each value is followed by a newline. Empty nodes, such as the non-matches of a selector, are skipped. A `Bytes` value (see `to_bytes`) is written as is, with no newline, so binary output stays exact.

```sh
$ mq -F raw '.h' doc.md
# One
## Two
$ mq -I null -F raw 'to_bytes("a")' | od -c
0000000   a
```

A Markdown node is written as its Markdown source. For its plain text, use `-F text`.

### Options

| Option | Applies to |
| ------ | ---------- |
| `-c`, `--compact` | `json` |
| `--indent`, `--tab` | `json`, `xml` |
| `-C`, `--color-output` | `markdown`, `json`, `jsonl`, `table`, `csv` |
| `--list-style`, `--link-title-style`, `--link-url-style` | `markdown`, `text`, `html`, `grep` |
| `-o`, `--output` | all formats |

## HTML output

`-F html` renders the result as HTML.

### Callouts

A block quote that starts with `[!TYPE]` is a callout. It becomes a `div` with the class `callout` and the type in `data-callout`. A fold marker (`-` or `+`) makes it a `details` element instead, open when the marker is `+`.

```markdown
> [!NOTE] Title
> body

> [!TIP]- Folded
> hidden
```

```html
<div class="callout" data-callout="note">
  <div class="callout-title">Title</div>
  <div class="callout-content">
    <p>body</p>
  </div>
</div>
<details class="callout" data-callout="tip">
  <summary class="callout-title">Folded</summary>
  <div class="callout-content">
    <p>hidden</p>
  </div>
</details>
```

### Wikilinks and embeds

`[[target]]` and `[[target|text]]` become links with the class `internal-link`. The target is used as the `href` as written. `![[target]]` becomes an `img`, `audio`, `video` or `embed` element when the file is an image, audio, video or PDF, and a link with the classes `internal-link embed` for a note.

### Raw HTML

Raw HTML in the input is kept, except for the tags that GFM filters: `<script>`, `<style>`, `<textarea>` and a few others. These are escaped and show up as text.

```markdown
<script>alert(1)</script>
```

```html
&lt;script>alert(1)&lt;/script>
```

## Color

`--color=WHEN` controls whether Markdown output is colorized. `WHEN` is one of:

| Value            | Behavior                                                                       |
| ---------------- | ------------------------------------------------------------------------------ |
| `auto` (default) | Color only when stdout is a terminal, `-o` is not set and `TERM` is not `dumb` |
| `always`         | Always color                                                                   |
| `never`          | Never color                                                                    |

`-C` alone is the same as `--color=always`, and `--color-output` is an alias of `--color`. `NO_COLOR` overrides `always`. See [Environment Variables](../reference/env.md#color-configuration) for `NO_COLOR` and `MQ_COLORS`.

```sh
mq '.h' README.md               # colored on a terminal, plain when piped
mq -C '.h' README.md | less -R  # force color
mq --color=never '.h' README.md # no color
```

## List style

Without `--list-style`, each list keeps the marker it was written with (`-`, `+` or `*`). Pass `--list-style dash`, `plus` or `star` to write every list with one marker.

```sh
$ printf '* a\n* b\n' | mq '.'
* a
* b
$ printf '* a\n* b\n' | mq --list-style dash '.'
- a
- b
```

The `listStyle` option of `mq-web` and `mq-nodejs` works the same way: leave it out to keep the markers.

## Frontmatter

A document that starts with a `---` line (or `+++`) and has a closing line is read with frontmatter. The frontmatter is not part of the rendered HTML, and `.yaml` and `.toml` select it.

Pass `--no-frontmatter` for a document that only happens to start with `---`, such as one that opens with a horizontal rule. The lines are then read as Markdown: the first `---` is a rule, and the text up to the next `---` is a paragraph or a heading.

```sh
$ cat doc.md
---
not: frontmatter
---
body
$ mq '.' doc.md
---
not: frontmatter
---
body
$ mq --no-frontmatter '.' doc.md
---
## not: frontmatter
body
```

`--no-frontmatter` applies to Markdown and MDX input.

## Exit codes

Exit codes follow jq. `halt(code)` exits with `code`.

| Code | Meaning                                                                             |
| ---- | ----------------------------------------------------------------------------------- |
| 0    | Success                                                                             |
| 2    | An input or query file could not be read, or the command-line arguments are invalid |
| 3    | The query has a syntax or module error                                              |
| 5    | A runtime error                                                                     |

With `-e` / `--exit-status` the exit code also reflects the result:

| Code | Meaning                                                     |
| ---- | ----------------------------------------------------------- |
| 0    | At least one result was not empty                           |
| 4    | Every result was empty, such as `select()` matching nothing |

`None`, `""`, `[]`, `{}` and a non-matching `select()` are empty. Other values, including `false` and `0`, are results.

```sh
mq -e 'select(.h2)' doc.md || echo "exit: $?"
```
