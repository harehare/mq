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
mq '.yaml | to_text()' post.md              # Frontmatter

# Filter
mq 'select(.code)' file.md                  # Only code blocks
mq 'select(!.code)' file.md                 # Exclude code blocks
mq 'select(.h.level <= 2)' file.md          # h1 and h2 only
mq 'select(contains("TODO"))' file.md       # Nodes with "TODO"

# Transform
mq '.h | to_text()' file.md                 # Headings as plain text
mq -U '.code.lang |= "rust"' file.md        # Update in place

# Multi-file
mq -A 'pluck(.code.value)' *.md             # Collect all code values
mq -S 's"\n---\n"' 'identity()' *.md       # Merge with separator

# Format conversion
mq -F html 'identity()' file.md             # Markdown → HTML
mq -F json '.h | to_text()' file.md         # Headings → JSON
mq -I html 'identity()' page.html           # HTML → Markdown

# Streaming large files
mq --stream 'select(contains("ERROR"))' large.md
```

## HTML Input: Always Use Markdown Selectors

When using `-I html`, mq converts HTML to Markdown first — use Markdown selectors, not HTML tags.

```bash
# WRONG
curl -s https://example.com | mq -I html '.p | to_text()'

# CORRECT
curl -s https://example.com | mq -I html '.text | to_text()'
curl -s https://example.com | mq -I html '.link.url'
curl -s https://example.com | mq -I html '.h | to_text()'
```

## Essential CLI Flags

| Flag                  | Purpose                                               |
| --------------------- | ----------------------------------------------------- |
| `-A, --aggregate`     | Combine all inputs into single array                  |
| `-I, --input-format`  | Input format (see REFERENCE.md for full list)         |
| `-F, --output-format` | Output format: `markdown`, `html`, `text`, `json`, `table`, `grep`, `raw`, `none` |
| `-U, --update`        | Update file in place                                  |
| `-f, --from-file`     | Load query from `.mq` file                            |
| `-S, --separator`     | Insert query result between files                     |
| `--args NAME VALUE`   | Set runtime variable (aliases: `--arg`, `--define`)   |
| `--stream`            | Process line by line                                  |
| `-C, --color-output`  | Colorize output                                       |
| `-P THRESHOLD`        | Parallel processing threshold (default: 10)           |
| `mq repl`             | Start interactive REPL session                        |

For full CLI options, attribute reference, and function list, see [REFERENCE.md](REFERENCE.md).
For advanced examples, see [EXAMPLES.md](EXAMPLES.md).

## When NOT to Use mq

- Binary file processing
- Simple `cat` / `echo` with no transformation
- Non-Markdown data where jq (JSON) or yq (YAML) fits better
