# Convert CSV to a Markdown table

**Goal**: Turn a `.csv` file into a formatted Markdown table for documentation.

**Prerequisites**: None — `.csv` files are parsed automatically.

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
| Name  | Age | City |
| ----- | --- | ---- |
| Alice | 30  | NYC  |
| Bob   | 25  | LA   |
```

## Notes

- The first row is treated as the header.
- Going the other direction? See [Convert a Markdown table to CSV](convert-table-to-csv.md).
