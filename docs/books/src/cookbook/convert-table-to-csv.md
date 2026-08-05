# Convert a Markdown table to CSV

**Goal**: Turn a table in a doc into CSV, e.g. to pull it into a spreadsheet.

**Prerequisites**: The `table` module, via `-A` or `nodes`.

## Query

```bash
$ mq -A 'import "table" | table::tables | first | table::to_csv' README.md
```

## Input

```markdown
| Name  | Age |
| ----- | --- |
| Alice | 30  |
```

## Output

```csv
Name,Age
Alice,30
```

## Notes

- Going the other direction? See [Convert CSV to a Markdown table](convert-csv-to-markdown-table.md).
