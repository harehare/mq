<h1 align="center">mq-lsp</h1>

Language Server Protocol (LSP) implementation for the [mq](https://mqlang.org/) query language, providing rich IDE features for mq development.

## Features

- 🔍 **Diagnostics**: Real-time syntax and semantic error reporting
- 🧹 **Linting**: Optional `mq-lint` diagnostics (correctness, style, complexity, selector, and module rules), toggled with `--enable-lint`
- 💡 **Code Completion**: Intelligent suggestions for selectors, functions, and variables
- 📖 **Hover Information**: Inline documentation and type information
- ✍️ **Signature Help**: Inline parameter hints while typing a function or macro call
- 🎯 **Go To Definition**: Navigate to symbol definitions with a single click
- 🔗 **Find References**: Locate all usages of a symbol across your workspace
- 🗂️ **Document Symbols**: Outline view of all symbols in the current file
- 🗃️ **Workspace Symbols**: Search for symbols by name across all loaded files/modules
- 🎨 **Semantic Tokens**: Enhanced syntax highlighting based on semantic analysis
- ✨ **Code Formatting**: Automatic code formatting following mq style guidelines
- 🛠️ **Code Actions**: Quick fixes such as adding a missing `include`/`import` for an unresolved function or module reference
- ✏️ **Rename**: Rename a symbol and update all of its references across files

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

### CLI Options

| Option                          | Description                                                |
| -------------------------------- | ------------------------------------------------------------ |
| `-M, --module-path <DIR>`        | Search modules from the directory (repeatable)              |
| `-T, --enable-type-checking`     | Enable type checking for mq queries                         |
| `--strict-array`                 | Reject heterogeneous arrays (requires `--enable-type-checking`) |
| `-L, --enable-lint`              | Enable `mq-lint` diagnostics                                 |
| `--disable-lint-rule <RULE_ID>`  | Disable a specific lint rule by ID (repeatable, requires `--enable-lint`) |

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
