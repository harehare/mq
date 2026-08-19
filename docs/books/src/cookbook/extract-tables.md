# Extract all tables from a document

Goal: Get every table in a document as structured table objects, ready for further transformation (add a row, convert to CSV, reshape, ...).

Prerequisites: The `table` module, via `-A` or `nodes`. Unlike the `section` module, `import "table"` must be written explicitly. It isn't auto-imported.

## Query

```bash
$ mq -A 'import "table" | table::tables' README.md
```

Or with `nodes`:

```mq
import "table"
| nodes
| table::tables
```

## Input

```markdown
| Name  | Age |
| ----- | --- |
| Alice | 30  |
```

## Output

```markdown
| Name  | Age |
| ----- | --- |
| Alice | 30  |
```

## Notes

- Table objects print back as Markdown automatically in CLI output, so `table::to_markdown` isn't needed here. It *is* needed when using the table module from Rust or other embedding code, where table objects stay plain dicts, and since `table::tables` returns an array (there can be more than one table), narrow to a single table first: `table::tables | first | table::to_markdown`.
- Chain `| first` after `table::tables` when you only want the first table, as the other table recipes in this Cookbook do.
