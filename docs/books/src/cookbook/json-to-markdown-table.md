# Turn JSON into a Markdown table

Goal: Read a JSON file, keep only the records you care about, and render them as a Markdown table.

Prerequisites: The built-in `json` module. JSON has no built-in input format, so read the file with `-I raw` to get its text as a single string.

## Query

```bash
$ mq -I raw 'import "json" | json::json_parse | get("users") | filter(fn(u): u["active"];) | json::json_to_markdown_table' users.json
```

## Input (`users.json`)

```json
{
  "users": [
    { "name": "Alice", "role": "admin", "active": true },
    { "name": "Bob", "role": "dev", "active": false },
    { "name": "Carol", "role": "dev", "active": true }
  ]
}
```

## Output

```markdown
| name | role | active |
| --- | --- | --- |
| Alice | admin | true |
| Carol | dev | true |
```

## Notes

- `json_parse` returns dicts and arrays, so the usual functions apply: `get("users")` reads a key, `filter` and `map` work on the array.
- Drop the `filter` to render every record. `json_to_markdown_table` takes the column names from the first record's keys.
- To get JSON back instead of a table, use `-F json`. This lists the names: `mq -I raw -F json 'import "json" | json::json_parse | get("users") | map(fn(u): u["name"];)' users.json` returns `["Alice", "Bob", "Carol"]`.
- `json::json_stringify(data)` turns a value back into a JSON string, for example to embed it in a larger string.
