# Count words in a document

Goal: Get a total word count for a Markdown file, e.g. to estimate reading time for a blog post.

Prerequisites: `-A`, since the total is a sum across every node.

## Query

```bash
$ mq -A 'nodes
| map(fn(n): to_text(n) | split(" ") | len;)
| fold(0, fn(acc, x): acc + x;)' post.md
```

## Input

```markdown
# Title

This is a short paragraph with some words in it.

## Section

Another paragraph here, with more words to count for the estimate.
```

## Output

```
23
```

## Notes

- Gotcha: calling `to_text(self)` directly on the whole `-A` array (instead of per-node inside `map`) does *not* give clean plain text. It keeps heading markers like `#`/`##` and joins nodes with commas. Convert each node to text individually with `to_text(n)` inside `map`, then sum the per-node word counts, as above.
- This is a rough count (splitting on single spaces), not a linguistically precise word count, good enough for a reading-time estimate, not for billing by the word.
