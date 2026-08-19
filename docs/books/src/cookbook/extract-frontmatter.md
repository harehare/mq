# Extract frontmatter metadata

Goal: Pull a document's YAML frontmatter out as structured data, e.g. to read `title`/`tags` for a static-site index.

Prerequisites: None.

## Query

```mq
.yaml | frontmatter
```

```bash
$ mq '.yaml | frontmatter' post.md
```

## Input

```markdown
---
title: Hello
tags: [a, b]
---

# Body
```

## Output

```json
{"title": "Hello", "tags": ["a", "b"]}
```

## Notes

- Grab a single field by chaining `get("title")`, e.g. `.yaml | frontmatter | get("title")`. A plain `.title` selector won't work, since that's a Markdown-node selector, not a dict-key accessor. Bracket access also works (`frontmatter()["title"]`), but only with the parentheses kept. `frontmatter["title"]` (no parens) tries to index the function itself and errors.
- Gotcha: `-F json`/`-F csv` don't give a clean metadata index here. Since the result is coming from a Markdown document, mq wraps the dict in a synthetic node before serializing it (e.g. `-F json` gives `{"type": "Text", "value": "{\"title\": ...}"}` instead of the dict's fields directly). Default output doesn't have this problem: `mq '.yaml | frontmatter' docs/**/*.md` prints one JSON object per file, one per line, which you can pipe into `jq -s` to build an array, or process as JSON Lines directly.
