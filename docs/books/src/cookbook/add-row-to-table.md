# Add a row to a table

**Goal**: Append a new row of data to an existing Markdown table.

**Prerequisites**: The `table` module, via `-A` or `nodes`.

## Query

```bash
$ mq -A 'import "table" | table::tables | first | table::add_row(["Charlie", "35"])' README.md
```

## Input

```markdown
| Name  | Age |
| ----- | --- |
| Alice | 30  |
```

## Output

```markdown
| Name    | Age |
| ------- | --- |
| Alice   | 30  |
| Charlie | 35  |
```

## Notes

- `add_row` takes a plain array of cell values, in column order — it will error if the array's length doesn't match the table's column count.
- Column widths are re-computed to fit the new data, which is why `Alice` gets re-padded alongside `Charlie`.
- No `table::to_markdown` call needed here — mq expands table objects to Markdown automatically in query output. See [Extract all tables from a document](extract-tables.md) for when `to_markdown` actually is required.
