<h1 align="center">mq-repl</h1>

Interactive REPL (Read-Eval-Print Loop) for mq query language.

## REPL Features

### Command History

- **Up/Down Arrows**: Navigate through previous commands
- **Ctrl+R**: Reverse search through history
- **History File**: Commands are saved between sessions

### Line Editing

- **Left/Right Arrows**: Move cursor within current line
- **Ctrl+A**: Move to beginning of line
- **Ctrl+E**: Move to end of line
- **Ctrl+K**: Delete from cursor to end of line
- **Ctrl+U**: Delete from cursor to beginning of line

### Tab Completion

- **Tab**: Auto-complete function names and keywords
- **Double Tab**: Show all available completions

## Development

### Building from Source

```sh
git clone https://github.com/harehare/mq
cd mq
cargo build --release -p mq-repl
```

### Running Tests

```sh
cargo test -p mq-repl
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
