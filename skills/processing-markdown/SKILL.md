---
name: processing-markdown
description: Processes Markdown files using mq, a jq-like query language for Markdown. Use when the user mentions Markdown processing, content extraction, document transformation, or mq queries.
---

# Processing Markdown with mq

## Core Selectors


| Selector         | Description            |
| ---------------- | ---------------------- |
| `.h`             | All headings           |
| `.h1`–`.h6`      | Specific heading level |
| `.text`          | Text nodes             |
| `.code`          | Code blocks            |
| `.code_inline`   | Inline code            |
| `.strong`        | Bold text              |
| `.emphasis`      | Italic text            |
| `.delete`        | Strikethrough          |
| `.link`          | Links                  |
| `.image`         | Images                 |
| `.list`          | List items             |
| `.blockquote`    | Block quotes           |
| `.[][]`          | Table cells            |
| `.html` / `.<>`  | HTML nodes             |
| `.footnote`      | Footnotes              |
| `.math`          | Math blocks            |
| `.yaml`, `.toml` | Frontmatter            |
| `.link_ref`      | Link references        |
| `.image_ref`     | Image references       |
| `.definition`    | Link/image definitions |

### Selector Calls (Filtered Matching)

```mq
.h(1)          # Only h1 headings
.h(2, 3)       # h2 and h3 headings
.h(1..3)       # h1 through h3 (range)
.code("rust")  # Only Rust code blocks
```

### Key Attribute Access

```mq
.h.level / .h.depth   # Heading level (1–6)
.h.value              # Heading text
.code.lang            # Code language
.code.value           # Code content
.link.url             # Link URL
.image.alt            # Image alt text
.list.checked         # Checkbox state (boolean)
."key"                # Dict key access (property selector)
```

### Update Operator

```mq
.code.lang |= "rust"           # Change code language in-place
.link.url  |= "https://new"    # Update link URL
```

### Recursive Descent

```mq
..ident    # Recursively select matching nodes in nested structures
```

## Common Patterns

```bash
# Extract
mq '.h' file.md                              # All headings
mq '.h(2)' file.md                           # h2 only
mq '.code("rust")' file.md                  # Rust code blocks
mq '.link.url' file.md                       # All URLs
mq '.yaml | to_text' post.md              # Frontmatter

# Filter
mq 'select(.code)' file.md                  # Only code blocks
mq 'select(!.code)' file.md                 # Exclude code blocks
mq 'select(.h.level <= 2)' file.md          # h1 and h2 only
mq 'select(contains("TODO"))' file.md       # Nodes with "TODO"

# Transform
mq '.h | to_text' file.md                 # Headings as plain text
mq -U '.code.lang |= "rust"' file.md        # Update in place

# Multi-file
mq -A 'pluck(.code.value)' *.md             # Collect code values, per file
mq --eval-all -A '.h | to_text' *.md        # Combine all files into one query (e.g. cross-file TOC)
mq -S 's"\n---\n"' 'identity' *.md       # Merge with separator

# mq accepts multiple file args directly (shell glob expansion) —
# no need to loop over files in bash:
mq '.h | to_text' *.md work/*.md docs/*.md

# Format conversion
mq -F html 'identity' file.md             # Markdown → HTML
mq -F json '.h | to_text' file.md         # Headings → JSON
mq -I html 'identity' page.html           # HTML → Markdown

# Streaming large files
mq --stream 'select(contains("ERROR"))' large.md
```

## HTML Input: Always Use Markdown Selectors

When using `-I html`, mq converts HTML to Markdown first — use Markdown selectors, not HTML tags.

```bash
# WRONG
curl -s https://example.com | mq -I html '.p | to_text'

# CORRECT
curl -s https://example.com | mq -I html '.text | to_text'
curl -s https://example.com | mq -I html '.link.url'
curl -s https://example.com | mq -I html '.h | to_text'
```

## Essential CLI Flags

A small, stable cheat sheet — not exhaustive. See below for everything else.

| Flag                   | Purpose                       |
| ----------------------- | ------------------------------ |
| `-A, --aggregate`       | Combine inputs into one array  |
| `-F, --output-format`   | Set output format              |
| `-I, --input-format`    | Set input format               |
| `-U, --update`          | Update file in place           |
| `-S, --separator`       | Insert separator between files |
| `--stream`              | Process line by line           |
| `--eval-all`            | Evaluate once against all files combined |
| `mq repl`               | Interactive REPL session       |

For the full CLI option list (all flags, possible format values, auto-parsing by file extension, `ARGS` handling), run `mq --help`.
For the full built-in function reference (300+ functions with descriptions), run `mq --doc`.

Note: `--args` also accepts the hidden aliases `--arg` and `--define` (not shown in `mq --help`).

## Node Attribute Reference

These attributes are Markdown-selector-specific and are not covered by `mq --doc` / `mq --help`.

| Node                                       | Attributes                                                       |
| ------------------------------------------ | ------------------------------------------------------------------ |
| `.h`                                        | `level`/`depth` (1–6), `value`                                    |
| `.code`                                     | `lang`/`language`, `value`, `meta`, `fence` (bool)                 |
| `.link`                                     | `url`, `title`, `value`                                           |
| `.image`                                    | `url`, `title`, `alt`                                             |
| `.list`                                     | `index`, `level`, `ordered` (bool), `checked` (bool), `value`     |
| `.[row][col]` (table cell)                  | `row`, `column`, `last_cell_in_row` (bool), `last_cell_of_in_table` (bool), `value` |
| `.link_ref`                                 | `ident`, `label`                                                   |
| `.image_ref`                                | `ident`, `label`, `alt`                                            |
| `.footnote_ref`                             | `ident`, `label`                                                   |
| `.footnote`                                 | `ident`, `text`                                                    |
| `.definition`                               | `ident`, `url`, `title`, `label`                                   |
| `.mdx_jsx_flow_element`                     | `name`                                                              |
| `.mdx_flow_expression`                      | `value`                                                             |

## Function Call Syntax

- All function calls require parentheses `()`.
- If a function is called with missing arguments, the piped value (`|`) is used as the first argument.

## Environment Variables

- `__FILE__` — full path to the file being processed
- `__FILE_NAME__` — filename without path
- `__FILE_STEM__` — filename without extension

For advanced examples, see [EXAMPLES.md](EXAMPLES.md).

## When NOT to Use mq

- Binary file processing
- Simple `cat` / `echo` with no transformation
- Non-Markdown data where jq (JSON) or yq (YAML) fits better
