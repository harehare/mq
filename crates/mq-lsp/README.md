<h1 align="center">mq-lsp</h1>

Language Server Protocol (LSP) implementation for the [mq](https://mqlang.org/) query language, providing rich IDE features for mq development.

## Features

- 🔍 **Diagnostics**: Real-time syntax and semantic error reporting
- 💡 **Code Completion**: Intelligent suggestions for selectors, functions, and variables
- 📖 **Hover Information**: Inline documentation and type information
- 🎯 **Go To Definition**: Navigate to symbol definitions with a single click
- 🔗 **Find References**: Locate all usages of a symbol across your workspace
- 🗂️ **Document Symbols**: Outline view of all symbols in the current file
- 🎨 **Semantic Tokens**: Enhanced syntax highlighting based on semantic analysis
- ✨ **Code Formatting**: Automatic code formatting following mq style guidelines

## Installation

### Using with VSCode Extension

The easiest way to use `mq-lsp` is through the VSCode extension:

1. Install the [mq VSCode extension](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq)
2. The LSP server is included and configured automatically

### Standalone Installation

#### Using Cargo

```bash
cargo install mq-lsp
```

#### From Source

```bash
git clone https://github.com/harehare/mq
cd mq/crates/mq-lsp
cargo build --release
```

The binary will be available at `target/release/mq-lsp`.

## Usage

### Running the LSP Server

The LSP server communicates via stdin/stdout following the LSP protocol:

```bash
mq-lsp
```

## Development

### Building from Source

```bash
git clone https://github.com/harehare/mq
cd mq
cargo build -p mq-lsp
```

### Running Tests

```bash
cargo test -p mq-lsp
```

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
