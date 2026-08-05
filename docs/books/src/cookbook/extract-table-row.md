# Extract a specific row from a table

**Goal**: Pull a single row out of a Markdown table by its position.

**Prerequisites**: None.

## Query

```mq
.[2][]
```

```bash
$ mq '.[2][]' README.md
```

## Input

```markdown
| Name  | Age | City |
| ----- | --- | ---- |
| Alice | 30  | NYC  |
| Bob   | 25  | LA   |
```

## Output

```markdown
| Bob | 25 | LA |
```

## Notes

- Row indexing starts at the header: index `0` is the header row, `1` is the first data row (Alice), `2` is the second (Bob).
- To get the cells as plain values instead of a rendered row, add `-F json` or `-F csv` to the command.
