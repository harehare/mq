# Extract frontmatter metadata

**Goal**: Pull a document's YAML frontmatter out as structured data, e.g. to read `title`/`tags` for a static-site index.

**Prerequisites**: None.

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

- Grab a single field by chaining `get("title")`, e.g. `.yaml | frontmatter | get("title")` — a plain `.title` selector won't work, since that's a Markdown-node selector, not a dict-key accessor. Bracket access also works (`frontmatter()["title"]`), but only with the parentheses kept — `frontmatter["title"]` (no parens) tries to index the function itself and errors.
- Combine with `-F csv` or `-F json` across multiple files to build a metadata index for a whole docs directory.
