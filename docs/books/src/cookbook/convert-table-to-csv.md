# Convert a Markdown table to CSV

Goal: Turn a table in a doc into CSV, e.g. to pull it into a spreadsheet.

Prerequisites: The `table` module, via `-A` or `nodes`.

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

## Using a different delimiter

`to_csv` takes an optional delimiter, so the same function produces TSV or PSV output too:

```bash
$ mq -A 'import "table" | table::tables | first | table::to_csv(self, "\t")' README.md
```

```tsv
Name	Age
Alice	30
```

## Notes

- Fields containing the delimiter, a quote, or a newline are quoted and escaped per RFC 4180 automatically. A cell holding `"Hi, there"` becomes `"""Hi, there"""` in the output, no manual escaping needed.
- A document with more than one table? `table::tables` returns all of them as an array; pick the one you want with `first`, or by index with `(table::tables())[1]` for the second table.
- Want structured data instead of a CSV string, e.g. to feed into `join_by` or another array builtin? Use `table::to_array` instead, which returns an array of dicts keyed by header text.
- Going the other direction? See [Convert CSV to a Markdown table](convert-csv-to-markdown-table.md).
