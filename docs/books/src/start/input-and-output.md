# Input and Output Behavior

How mq reads Markdown and writes it back, and the options that change it. For the full list of flags, see the [CLI reference](../reference/cli.md).

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
