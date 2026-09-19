# Fill blank cells in a CSV column

Goal: Expand spreadsheet-style CSV where a repeated value is left blank ("same as above") so every row carries its value before you group or aggregate.

Prerequisites: None, `.csv` files are parsed automatically.

## Query

```bash
$ mq 'csv::forward_fill(["region"]) | csv::csv_stringify(",")' stores.csv
```

## Input (`stores.csv`)

```csv
region,product,owner
East,apple,Kim
,banana,
West,apple,
,banana,Lee
```

## Output

```csv
region,product,owner
East,apple,Kim
East,banana,
West,apple,
West,banana,Lee
```

Each blank `region` takes the last non-blank value above it.

## Fill within a group

A value should not leak across groups. Pass `group_by` columns as the third argument so each group keeps its own carried value:

```bash
$ mq 'csv::forward_fill(["region"]) | csv::forward_fill(self, ["owner"], ["region"]) | csv::csv_stringify(",")' stores.csv
```

```csv
region,product,owner
East,apple,Kim
East,banana,Kim
West,apple,
West,banana,Lee
```

`Kim` fills the East row but stops at the West boundary, so `West,apple` stays blank.

## Fill what is left with a constant

`constant_fill` takes a dict of column name to value. Chain it after `forward_fill` to cover cells that have nothing above them:

```bash
$ mq 'csv::forward_fill(["region"]) | csv::forward_fill(self, ["owner"], ["region"]) | csv::constant_fill(self, {"owner": "unassigned"}) | csv::csv_stringify(",")' stores.csv
```

```csv
region,product,owner
East,apple,Kim
East,banana,Kim
West,apple,unassigned
West,banana,Lee
```

## Fill from below, or limit the fill

`backward_fill` takes the same arguments and copies the next non-blank value below instead. `max_gap` (fifth argument) caps how many consecutive blanks are filled, starting from the ones nearest the source value:

```bash
$ mq 'csv::forward_fill(self, ["region"], [], "both", 1) | csv::csv_stringify(",")' stores.csv
```

## Notes

- By default both `""` and a missing key count as blank. Pass `"empty"` or `"none"` as the fourth argument (`missing`) to fill only one of them.
- A blank with no value above it (or below, for `backward_fill`) is left as is.
- Existing values are never changed, and the input is not modified: each function returns a new array.
- When you pass optional arguments, write `self` as the first argument, as in `csv::forward_fill(self, ["region"], ["dept"])`. With only the column list, `csv::forward_fill(["region"])` works directly in a pipe.
- Rows can also be arrays (`mq --no-header`). The first row is treated as the header and left untouched, and columns can be named by header text or by index.
- These functions fill from neighbouring cells only. They do not estimate values statistically.
- Going the other direction? See [Convert CSV to a Markdown table](convert-csv-to-markdown-table.md).
