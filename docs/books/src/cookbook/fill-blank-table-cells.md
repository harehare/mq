# Fill blank cells in a Markdown table

Goal: Expand a Markdown table where a repeated value is left blank ("same as above") so every row carries its value before you group or aggregate. This is the Markdown table version of [Fill blank cells in a CSV column](fill-blank-csv-cells.md).

Prerequisites: The `table` module, via `-A` or `nodes`.

## Query

```bash
$ mq -A 'import "table" | table::tables | first | table::forward_fill(["Region"])' stores.md
```

## Input (`stores.md`)

```markdown
| Region | Product | Owner |
| :----- | ------- | ----: |
| East   | apple   | Kim   |
|        | banana  |       |
| West   | apple   |       |
|        | banana  | Lee   |
```

## Output

```markdown
| Region | Product | Owner |
| :----- | ------- | ----: |
| East   | apple   |   Kim |
| East   | banana  |       |
| West   | apple   |       |
| West   | banana  |   Lee |
```

Each blank `Region` takes the last non-blank value above. The header, column alignment and every untouched cell are kept as they were.

## Fill within a group, then fill what is left

`table::forward_fill`, `table::backward_fill` and `table::constant_fill` take the same arguments as their `csv::` counterparts, with the table as the first argument. Once you pass more than the table itself, write `self` explicitly:

```bash
$ mq -A 'import "table" | table::tables | first | table::forward_fill(["Region"]) | table::forward_fill(self, ["Owner"], ["Region"]) | table::constant_fill(self, {"Owner": "unassigned"})' stores.md
```

```markdown
| Region | Product |      Owner |
| :----- | ------- | ---------: |
| East   | apple   |        Kim |
| East   | banana  |        Kim |
| West   | apple   | unassigned |
| West   | banana  |        Lee |
```

`Kim` fills the East row but stops at the West boundary. The leftover blank then takes the constant.

## Notes

- Columns are header names, or column indexes (0-based). Use an index when two columns share the same header text.
- A cell counts as blank when it is empty. Table cells are never missing, so the `missing` argument (`"empty"`, `"none"` or `"both"`) only changes anything if you pass `"none"`, which fills nothing.
- Given the same records and options, a table gives the same values as `csv::forward_fill` and friends, so you can switch between CSV and Markdown input without changing the fill options.
- `constant_fill` converts the value to a string, so `{"Owner": 0}` writes `0`.
