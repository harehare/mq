# Parse LTSV logs

Goal: Read a [LTSV](http://ltsv.org/) (Labeled Tab-Separated Values) log, such as an nginx access log, and filter its records by field.

Prerequisites: The [ltsv.mq](https://github.com/harehare/ltsv.mq) extension module. Copy `ltsv.mq` into your module directory, or import it over HTTP with `--allow-http-import` and `import "github.com/harehare/ltsv.mq"`. LTSV has no built-in input format, so read the file with `-I raw` to get its text as a single string.

## Query

```bash
$ mq -I raw -F json 'import "ltsv" | ltsv::ltsv_parse(.) | filter(fn(r): r["status"] == "404";)' access.ltsv
```

## Input (`access.ltsv`)

Fields are separated by tabs.

```
host:127.0.0.1	req:GET /	status:200	time:2013-01-01T12:00:00
host:10.0.0.1	req:GET /api	status:404	time:2013-01-01T12:00:05
host:10.0.0.2	req:POST /login	status:500	time:2013-01-01T12:00:09
```

## Output

```json
[
  {
    "host": "10.0.0.1",
    "req": "GET /api",
    "status": "404",
    "time": "2013-01-01T12:00:05"
  }
]
```

## Notes

- `ltsv_parse` returns an array with one dict per line, keyed by label. Blank lines are skipped, and a field without a `:` raises an error.
- Values stay strings, so convert before comparing numbers: `filter(fn(r): to_number(r["status"]) >= 400;) | map(fn(r): r["req"];)` returns `["GET /api", "POST /login"]`.
- Only the first `:` in a field separates the label from the value, so a value such as `2013-01-01T12:00:00` keeps its colons.
- To review the log as a table, pipe the records into `csv::csv_to_markdown_table()`. See [Convert CSV to a Markdown table](convert-csv-to-markdown-table.md).
