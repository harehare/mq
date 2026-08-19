# Get the Nth item from every list

Goal: Pull out, say, the second item of every list in a document, useful for sanity-checking that parallel lists stay in sync.

Prerequisites: None.

## Query

```mq
.[1]
```

```bash
$ mq '.[1]' README.md
```

## Input

```markdown
- A1
- A2
- A3

1. B1
2. B2
```

## Output

```markdown
- A2
2. B2
```

## Notes

- Indexing is 0-based and applies independently to each list node in the document. `.[1]` here returns the second bullet (`A2`) from the first list and the second ordered item (`B2`) from the second list.
- A list shorter than the requested index is skipped rather than erroring: `.[2]` against the input above would return only `- A3`, since the ordered list has no third item.
