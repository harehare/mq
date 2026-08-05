# Trim a document down to LLM-sized context

**Goal**: Pull a bounded, high-signal slice of a document — headings and code, capped at N matches — to fit into an LLM prompt without dumping the whole file.

**Prerequisites**: `-A` (or `nodes` in a script), so the result can be collected into one array before capping its length.

## Query

```bash
$ mq -A 'nodes | filter(fn(n): n | select(.h || .code) | !is_none();) | take(5)' README.md
```

## Output

```markdown
## Why mq?

## Features

## Installation

### Quick Install

```bash
curl -sSL https://mqlang.org/install.sh | bash
```
```

## Notes

- **Gotcha**: `select(.h || .code)` filters correctly when streaming per-node (mq's default, one query run per node), but *not* when applied directly to an already-collected array — apply it inside a `filter(fn(n): n | select(...) | !is_none();)` lambda instead once you've called `nodes`.
- `take(n)` (implicitly `take(self, n)`) caps the result at the first `n` elements; `self[:n]` (array slicing — note this needs `self`, not `.`, before the brackets) does the same thing here and can be used interchangeably.
- Swap the predicate for whatever signal matters for the prompt — e.g. `n | select(.h || .link) | !is_none()` to extract structure plus references instead of code.
