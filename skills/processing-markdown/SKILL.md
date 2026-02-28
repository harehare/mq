---
name: processing-markdown
description: Processes Markdown files using mq, a jq-like query language for Markdown. Use when the user mentions Markdown processing, content extraction, document transformation, or mq queries.
---

# Processing Markdown with mq

## Quick Reference

### Selectors

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
| `.html` or `.<>` | HTML nodes             |
| `.footnote`      | Footnotes              |
| `.math`          | Math blocks            |
| `.yaml`, `.toml` | Frontmatter            |

### Attribute Access

```mq
.h.level           # Heading level (1-6)
.h.depth           # Same as .h.level
.code.lang         # Code block language
.code.value        # Code block content
.link.url          # Link URL
.link.title        # Link title
.image.url         # Image URL
.image.alt         # Image alt text
.list.index        # List item index
.list.level        # Nesting level
.list.ordered      # Whether ordered list
.list.checked      # Checkbox state
.[0][0].row        # Table cell row
.[0][0].column     # Table cell column
```

## Common Patterns

### Extract Elements

```bash
mq '.h' file.md                         # All headings
mq '.h1' file.md                        # Only h1 headings
mq '.code' file.md                      # All code blocks
mq '.link.url' file.md                  # All URLs
mq '.image.alt' file.md                 # All image alt texts
```

### Filter with select

```bash
mq 'select(.code)' file.md              # Only code blocks
mq 'select(!.code)' file.md             # Everything except code blocks
mq 'select(.h.level <= 2)' file.md      # h1 and h2 only
mq 'select(.code.lang == "rust")' file.md  # Rust code blocks only
mq 'select(contains("TODO"))' file.md   # Nodes containing "TODO"
```

### Transform Content

```bash
mq '.h | to_text()' file.md             # Headings as plain text
mq '.code | to_text()' file.md          # Code block content as text
mq '.link | to_string()' file.md        # Links as strings
```

### Generate Table of Contents

```mq
.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, level - 1)
```

### Multi-File Processing

```bash
mq -A 'pluck(.code.value)' *.md         # Collect all code values
mq -S 's"\n---\n"' 'identity()' *.md    # Merge files with separator
```

## Important: HTML Input and Selectors

When using `-I html`, mq first converts HTML to Markdown, then queries the resulting Markdown AST. **You must always use Markdown selectors, not HTML element names.**

```bash
# WRONG: HTML element selectors do NOT work
curl -s https://example.com | mq -I html '.p | to_text()'        # Error: Unknown selector `.p`
curl -s https://example.com | mq -I html '.a | to_text()'        # Error: Unknown selector `.a`
curl -s https://example.com | mq -I html '.div | to_text()'      # Error: Unknown selector `.div`

# CORRECT: Use Markdown selectors regardless of input format
curl -s https://example.com | mq -I html '.text | to_text()'     # Extract text nodes
curl -s https://example.com | mq -I html '.link | to_text()'     # Extract links (HTML <a> → .link)
curl -s https://example.com | mq -I html '.h | to_text()'        # Extract headings (HTML <h1>-<h6> → .h)
curl -s https://example.com | mq -I html '.code | to_text()'     # Extract code blocks (HTML <pre><code> → .code)
curl -s https://example.com | mq -I html '.image'                # Extract images (HTML <img> → .image)
curl -s https://example.com | mq -I html '.strong'               # Extract bold text (HTML <strong>/<b> → .strong)
curl -s https://example.com | mq -I html '.emphasis'             # Extract italic text (HTML <em>/<i> → .emphasis)
curl -s https://example.com | mq -I html '.list'                 # Extract list items (HTML <ul>/<ol>/<li> → .list)
curl -s https://example.com | mq -I html '.blockquote'           # Extract blockquotes (HTML <blockquote> → .blockquote)
```

The `-I html` flag only changes the **input parser** — the query language and selectors remain the same as for Markdown input.

## CLI Options

| Flag                           | Purpose                                                        |
| ------------------------------ | -------------------------------------------------------------- |
| `-A, --aggregate`              | Combine all inputs into single array                           |
| `-I, --input-format`           | Input format: `markdown`, `mdx`, `html`, `text`, `null`, `raw` |
| `-F, --output-format`          | Output format: `markdown`, `html`, `text`, `json`, `none`      |
| `-U, --update`                 | Update file in place                                           |
| `-f, --from-file`              | Load query from `.mq` file                                     |
| `-o, --output`                 | Write to output file                                           |
| `-S, --separator`              | Insert query result between files                              |
| `--args NAME VALUE`            | Set runtime variable                                           |
| `--rawfile NAME FILE`          | Load file into variable                                        |
| `--stream`                     | Process line by line                                           |
| `--csv, --yaml, --toml, --xml` | Load built-in modules                                          |
| `-C, --color-output`           | Colorize output                                                |
| `-P THRESHOLD`                 | Parallel processing threshold                                  |

For complete function reference, see [REFERENCE.md](REFERENCE.md).
For advanced patterns and examples, see [EXAMPLES.md](EXAMPLES.md).

## When NOT to Use mq

- Binary file processing
- Non-Markdown structured data (use jq for JSON, yq for YAML instead)
- Simple `cat` or `echo` operations where no transformation is needed
