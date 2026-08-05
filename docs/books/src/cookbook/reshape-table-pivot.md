# Reshape a table between wide and long form

**Goal**: Pivot a table from one-column-per-category (wide) to one-row-per-category (long), or back — the same reshape spreadsheet tools call "unpivot"/"pivot".

**Prerequisites**: The `table` module, via `-A` or `nodes`.

## Wide to long (`pivot_longer`)

Unpivot a set of columns into `name`/`value` row pairs, keeping the remaining columns as identifiers. `value_columns` is an array of column indices; `names_to` and `values_to` (both optional) name the two new columns.

```bash
$ mq -A 'import "table" | let t = first(table::tables()) | table::pivot_longer(t, [1, 2, 3], "quarter", "score")' README.md
```

**Input**:

```markdown
| Name  | Q1 | Q2 | Q3 |
| ----- | -- | -- | -- |
| Alice | 10 | 20 | 30 |
| Bob   | 5  | 15 | 25 |
```

**Output**:

```markdown
| Name  | quarter | score |
| ----- | ------- | ----- |
| Alice | Q1      | 10    |
| Alice | Q2      | 20    |
| Alice | Q3      | 30    |
| Bob   | Q1      | 5     |
| Bob   | Q2      | 15    |
| Bob   | Q3      | 25    |
```

## Long to wide (`pivot_wider`)

The inverse: spread a key/value column pair back out into one column per distinct key, grouping rows by the remaining identifier columns. `names_from` is the column index whose distinct values become new headers; `values_from` supplies the values. Combinations missing from the input become empty cells.

```bash
$ mq -A 'import "table" | table::tables | first | table::pivot_wider(1, 2)' README.md
```

Feeding the long output above back in with `pivot_wider(1, 2)` reproduces the original wide table.

## Notes

- `pivot_longer` and `pivot_wider` are exact inverses of each other for well-formed input, so use whichever direction matches the shape you're starting from.
- No `table::to_markdown` call needed here — mq expands table objects to Markdown automatically in query output. See [Extract all tables from a document](extract-tables.md) for when `to_markdown` actually is required.
