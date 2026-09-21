# Generate Markdown from data

Goal: Build a Markdown document (headings, a table, a list) from structured data such as JSON, without concatenating strings by hand.

Prerequisites: The built-in `md` module. Its builders return Markdown nodes, so `md::doc` can combine them and mq prints the result as Markdown. Pass the data in with `-I json` (or another input format), or start from nothing with `-I null`.

## Query

```bash
$ mq -I json 'import "md" | let users = get("users") | md::doc(md::h2("Team"), md::table(["Name", "Role"], map(users, fn(u): [u["name"], u["role"]];)), md::h3("Members"), map(users, fn(u): md::list(u["name"]);))' users.json
```

## Input (`users.json`)

```json
{
  "users": [
    { "id": 1, "name": "Alice", "role": "admin" },
    { "id": 2, "name": "Bob", "role": "dev" },
    { "id": 3, "name": "Carol", "role": "dev" }
  ]
}
```

## Output

```markdown
## Team
|Name|Role|
|---|---|
|Alice|admin|
|Bob|dev|
|Carol|dev|
### Members
- Alice
- Bob
- Carol
```

## Render it as HTML

Add `-F html` to the same query to get HTML instead of Markdown:

```html
<h2>Team</h2>
<table>
<thead>
<tr>
<th>Name</th>
<th>Role</th>
</tr>
</thead>
…
```

## Notes

- `md::doc(...)` accepts any number of nodes. Arrays are flattened into it, which is why a `map` that returns one node per item can be passed in directly.
- `md::table(header, rows, aligns)` takes an array of header cells and an array of rows, each an array of cells. Pass alignments such as `["left", "right"]` to get `:---` and `---:` in the separator row.
- Other builders follow the same pattern: `h1` to `h6`, `code(value, lang)`, `link(url, text)`, `image(url, alt)`, `blockquote`, `callout(value, kind)` for `> [!NOTE]`-style alerts, and `list(value, level, ordered, checked)`. `md::list("todo", 0, false, false)` gives `- [ ] todo` and passing `true` as the last argument gives `- [x] todo`.
- The nodes are printed on consecutive lines with no blank line between blocks, and table cells are not padded, as in the output above.
- If the data is already in a JSON file and you only need a table, see [Turn JSON into a Markdown table](json-to-markdown-table.md).
