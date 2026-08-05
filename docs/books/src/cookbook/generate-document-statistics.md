# Generate document statistics

**Goal**: Get a quick count of headers, paragraphs, code blocks, and links in a document — a lightweight completeness/complexity check.

**Prerequisites**: `-A`, since the counts are computed across the whole document at once.

## Query

```bash
$ mq -A 'let headers = count_by(fn(x): x | select(.h);)
| let paragraphs = count_by(fn(x): x | select(.text);)
| let code_blocks = count_by(fn(x): x | select(.code);)
| let links = count_by(fn(x): x | select(.link);)
| s"Headers: ${headers}, Paragraphs: ${paragraphs}, Code: ${code_blocks}, Links: ${links}"' README.md
```

## Output

```
Headers: 15, Paragraphs: 47, Code: 7, Links: 17
```

(Numbers above are from running this query against mq's own `README.md`.)

## Notes

- `count_by(fn(x): x | select(...);)` counts how many of the document's nodes match the predicate — swap the selector to count anything else (`.code.lang == "js"` for JS blocks specifically, etc.).
