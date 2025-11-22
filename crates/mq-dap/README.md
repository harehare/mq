<h1 align="center">mq-dap</h1>

Debug Adapter Protocol implementation for mq.

## Usage

### Command Line

Start the DAP server (typically done automatically by your editor):

```bash
# Start the DAP server
mq-dbg

# Debug a specific query file
mq-dbg query.mq input.md
```

### Debugging Features

Once connected to a DAP client:

1. **Set Breakpoints**: Click in the gutter or use your editor's breakpoint command
2. **Start Debugging**: Launch the debugger with your query file
3. **Step Through Code**: Use step over, step in, and step out commands
4. **Inspect Variables**: Hover over variables or view them in the variables pane
5. **View Call Stack**: See the current execution stack in the call stack pane

### Example Debug Session

```sh
# Create a query file
echo '.h | let y = to_text() | breakpoint() | y' > query.mq

# Start debugging
mq-dbg query.mq input.md
```

## Development

### Building from Source

```sh
git clone https://github.com/harehare/mq
cd mq
cargo build --release -p mq-dap
```

### Running Tests

```sh
cargo test -p mq-dap
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
