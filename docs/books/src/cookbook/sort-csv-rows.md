# Sort CSV rows by column

Goal: Order CSV rows by one or more columns, comparing numeric cells by value instead of as text.

Prerequisites: None, `.csv` files are parsed automatically.

## Query

```bash
$ mq 'csv::sort_rows("age") | csv::csv_stringify(",")' people.csv
```

## Input (`people.csv`)

```csv
name,age,city
Alice,30,Tokyo
Bob,9,Osaka
Carol,100,Tokyo
Dave,-5,Kyoto
```

## Output

```csv
name,age,city
Dave,-5,Kyoto
Bob,9,Osaka
Alice,30,Tokyo
Carol,100,Tokyo
```

`9` sorts before `30` and `100`. A plain `sort_by` would compare the cells as text and put `100` first.

## Descending order

Pass `true` as the third argument. Give `self` explicitly when you pass more than one argument:

```bash
$ mq 'csv::sort_rows(self, "age", true) | csv::csv_stringify(",")' people.csv
```

Rows with equal keys keep their original order.

## Sort by several columns

Pass an array of columns. Later columns break ties:

```bash
$ mq 'csv::sort_rows(["city", "age"]) | csv::csv_stringify(",")' people.csv
```

## Mix ascending and descending

Pass a dict of column to `"asc"` or `"desc"` to order each column separately. Key order is the sort priority:

```bash
$ mq 'csv::sort_rows({"city": "asc", "age": "desc"}) | csv::csv_stringify(",")' people.csv
```

```csv
name,age,city
Dave,-5,Kyoto
Bob,9,Osaka
Carol,100,Tokyo
Alice,30,Tokyo
```

The third argument is ignored when `cols` is a dict.

## Color the result

`-C` colors the header and typed values when the output format is CSV:

```bash
$ mq -C -F csv 'csv::sort_rows("age")' people.csv
```

`NO_COLOR` disables it, and colors can be customized with `MQ_COLORS`.

## Notes

- Numeric cells sort by value, other cells as text, and empty cells last (first with descending order).
- Array rows keep their first row in place as the header and take column indexes, e.g. `csv::sort_rows(1)`. This applies to `--no-header` input too, where the first data row stays put.
