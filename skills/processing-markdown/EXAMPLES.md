# mq Examples

## Content Extraction

```bash
mq '.h | to_text()' README.md                               # Headings as text
mq '.h(2) | to_text()' README.md                            # h2 headings only
mq 'select(.code.lang == "python") | .code.value' docs.md   # Python code blocks
mq '.link.url' file.md                                      # All URLs
mq '.yaml | to_text()' post.md                              # Frontmatter
mq 'select(.list.checked == false)' todo.md                 # Unchecked tasks
```

## Transformation

```bash
mq '.h | increase_header_level(self)' file.md               # Increase heading levels
mq '.code | set_code_block_lang(self, "ts")' file.md        # Change code language
mq '.list | set_list_ordered(self, true)' file.md           # Make lists ordered
mq -U '.code.lang |= "rust"' file.md                        # Update code lang in place
mq -U '.link.url |= replace(self, "http://", "https://")' file.md  # Upgrade links
```

## Format Conversion

```bash
mq -F html 'identity()' file.md                             # Markdown → HTML
mq -F text 'identity()' file.md                             # Markdown → plain text
mq -F json '.h | to_text()' file.md                         # Headings → JSON array
mq -F json -C '.h | to_text()' file.md                      # JSON with color output
mq -I html 'identity()' page.html                           # HTML → Markdown
mq -F table '.[][]' data.md                                 # Table cells as table
mq -F grep 'select(contains("TODO"))' file.md               # Grep-style output
```

## Auto-Parsing Structured Files

```bash
mq '."name"' package.json                                   # JSON field (auto-detected)
mq '."version"' Cargo.toml                                  # TOML field (auto-detected)
mq '."dependencies" | keys' package.json                    # JSON object keys
mq --csv 'include "csv" | csv_parse(true) | csv_to_markdown_table()' data.csv
mq -I raw 'identity()' data.json                            # Disable auto-parse
```

## Multi-File & Aggregation

```bash
mq -A '.h | to_text()' docs/*.md                            # All headings across files
mq -S 'to_hr()' 'identity()' ch1.md ch2.md ch3.md          # Merge with <hr> separator
mq -P 10 '.h' docs/**/*.md                                  # Parallel processing
```

## Streaming Large Files

```bash
mq --stream 'select(contains("ERROR"))' large.md            # Filter matching lines
mq --stream '.text | to_text()' huge.md                     # Extract text line by line
```

## Grep Context

```bash
mq -F grep -B 1 --after-context 2 'select(contains("TODO"))' file.md
mq -F grep --context 3 'select(.h)' file.md
```

## Bytes Operations

```bash
mq 'b"hello" | len' /dev/null                               # Byte length
mq 'b"\xf0\x9f\x99\x82" | base64(self)' /dev/null          # Base64 encode bytes
mq 'b"abc" | type' /dev/null                                # Returns "bytes"
```

## Language Syntax

```mq
# Variables
let x = 42
var counter = 0 | counter = counter + 1

# Functions
def double(x): x * 2;
map([1,2,3], fn(x): x * 2;)

# Control flow
if (x > 0): "positive" elif (x < 0): "negative" else: "zero"

# Pattern matching
match (value):
  | 1: "one"
  | [x, y]: add(x, y)
  | _: "other"
end

# Loops
foreach (x, [1, 2, 3]): add(x, 1) end

# String interpolation
let name = "Alice" | s"Hello, ${name}!"

# Byte literals
b"abc"               # Raw bytes
b"\xc3\xa9"         # Non-ASCII via hex escape

# Error handling
try: risky_operation() catch: handle_error()

# Pipe chains
.h | select(.h.level == 2) | to_text() | upcase()

# Recursive descent
..value              # Select all .value keys in nested structures
```

## Generate Table of Contents

```mq
.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, level - 1)
```

## Runtime Variables

```bash
mq --args TARGET "rust" 'select(.code.lang == $TARGET)' file.md
mq --rawfile TEMPLATE header.md '.h | to_text()' file.md
```
