# mq Examples

## Content Extraction

```bash
mq '.h | to_text()' README.md                              # Headings as text
mq 'select(.code.lang == "python") | .code.value' docs.md  # Python code blocks
mq '.link.url' file.md                                     # All URLs
mq '.yaml | to_text()' post.md                             # Frontmatter
```

## Transformation

```bash
mq '.h | increase_header_level(self)' file.md              # Increase heading levels
mq '.code | set_code_block_lang(self, "ts")' file.md       # Change code language
mq '.list | set_list_ordered(self, true)' file.md          # Make lists ordered
mq 'select(.list.checked == false)' todo.md                # Unchecked tasks
```

## Format Conversion

```bash
mq -F html 'identity()' file.md                            # Markdown → HTML
mq -F text 'identity()' file.md                            # Markdown → plain text
mq -F json '.h | to_text()' file.md                        # Headings → JSON
mq -I html 'identity()' page.html                          # HTML → Markdown
mq --csv 'include "csv" | csv_parse(true) | csv_to_markdown_table()' data.csv
```

## Multi-File & Aggregation

```bash
mq -A '.h | to_text()' docs/*.md                           # All headings across files
mq -S 'to_hr()' 'identity()' ch1.md ch2.md ch3.md         # Merge with separators
mq -P 10 '.h' docs/**/*.md                                 # Parallel processing
```

## Language Syntax

```mq
# Variables
let x = 42
var counter = 0 | counter = counter + 1

# Functions
def double(x): mul(x, 2);
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

# Error handling
try: risky_operation() catch: handle_error()

# Pipe chains
.h | select(.h.level == 2) | to_text() | upcase(self)
```
