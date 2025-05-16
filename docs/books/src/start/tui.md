# Interactive TUI

The Text-based User Interface (TUI) provides an interactive way to explore and query Markdown files directly in your terminal.

```sh
$ mq tui file.md
```

## TUI Features

- **Interactive Querying**: Enter and edit queries in real-time with immediate feedback
- **Detail View**: Examine the structure of selected markdown nodes in depth
- **Navigation**: Browse through query results with keyboard shortcuts
- **Query History**: Access and reuse previous queries

## TUI Key Bindings

| Key              | Action                     |
| ---------------- | -------------------------- |
| `:` (colon)      | Enter query mode           |
| `Enter`          | Execute query              |
| `Esc` / `q`      | Exit query mode / Exit app |
| `↑`/`k`, `↓`/`j` | Navigate results           |
| `d`              | Toggle detail view         |
| `?` / `F1`       | Show help screen           |
| `Ctrl+l`         | Clear query                |
| `PgUp`/`PgDn`    | Page through results       |
| `Home`/`End`     | Jump to first/last result  |
