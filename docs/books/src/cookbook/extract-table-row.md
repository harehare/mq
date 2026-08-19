# Extract a specific row from a table

Goal: Pull a single row out of a Markdown table by its position.

Prerequisites: None.

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
- To get the cells as plain text instead of a rendered row, pipe through `to_text`: `mq '.[2][] | to_text' README.md` prints `Bob`, `25`, `LA` on separate lines. `-F json` dumps the full node structure (position info and all), not just the values, so it's rarely what you want here.
- Need the row as a keyed record instead (`{"Name": "Bob", "Age": "25", "City": "LA"}`)? See [Extract all tables from a document](extract-tables.md) and use `table::to_array`, which returns rows in that shape without the header-offset indexing above.
