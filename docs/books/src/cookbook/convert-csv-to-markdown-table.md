# Convert CSV to a Markdown table

Goal: Turn a `.csv` file into a formatted Markdown table for documentation.

Prerequisites: None, `.csv` files are parsed automatically.

## Query

```bash
$ mq 'csv::csv_to_markdown_table' example.csv
```

## Input (`example.csv`)

```csv
Name,Age,City
Alice,30,NYC
Bob,25,LA
```

## Output

```markdown
| Name | Age | City |
| --- | --- | --- |
| Alice | 30 | NYC |
| Bob | 25 | LA |
```

## Notes

- Columns aren't padded to equal width. That's still valid Markdown, and most renderers display it identically to a hand-aligned table.
- `csv_to_markdown_table` also accepts data already built in a query, either an array of dicts (`[{"name": "Alice", "age": 30}]`, keys become headers) or an array of arrays with the header as the first row (`[["name", "age"], ["Alice", 30]]`).
- Quoted fields in the source CSV, including ones containing the delimiter, are unquoted correctly before being placed in the table.
- Going the other direction? See [Convert a Markdown table to CSV](convert-table-to-csv.md).
