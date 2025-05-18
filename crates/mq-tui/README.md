# mq-tui

TUI (Text-based User Interface) for the mq Markdown processor. This crate provides an interactive terminal interface for querying and manipulating Markdown content using the mq query language.

## Features

- Interactive terminal UI powered by [Ratatui](https://github.com/ratatui-org/ratatui)
- Real-time Markdown querying and filtering
- Similar UI experience to [fx](https://github.com/antonmedv/fx), but for Markdown
- Support for all mq query functionality

## Usage

```bash
# Simple usage from inside main mq CLI
mq tui file.md
```

## Key Bindings

- `Esc` / `q`: Quit the application
- `Up/k` / `Down/j`: Navigate through the result list
- `PageUp/PageDown`: Page navigation through results
- `Home/End`: Jump to first/last result
- `:`: Enter query mode
- `Enter`: Execute query
- `Esc`: Exit query mode
- `d`: Toggle detailed view of selected item
- `?` / `F1`: Show help screen
- `Ctrl+L`: Clear current query
