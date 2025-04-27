# mq-lsp

`mq-lsp` is a Language Server Protocol (LSP) implementation for the [mq](https://mqlang.org/). It provides various language features such as syntax highlighting, code completion, go-to-definition, and more.

## Features

- **Initialization**: Handles the initialization of the LSP server and sets up the server capabilities.
- **Diagnostics**: Publishes diagnostics information to the client.
- **Hover**: Provides hover information for symbols.
- **Completion**: Offers code completion suggestions.
- **Go To Definition**: Allows navigation to the definition of symbols.
- **References**: Finds all references to a symbol.
- **Document Symbols**: Lists all symbols in a document.
- **Semantic Tokens**: Provides semantic tokens for syntax highlighting.
- **Formatting**: Formats the document according to the MDQ language formatting rules.

## Usage

To use this LSP server, you need to integrate it with an LSP client. The server reads from stdin and writes to stdout, making it compatible with various editors and IDEs that support LSP.

### Installation

You can install `mq-lsp` using Cargo, the Rust package manager:

```bash
$ cargo install --git https://github.com/harehare/mq.git mq-lsp
# Installing from cargo is under preparation.
$ cargo install mq-lsp
```

#### Homebrew
```bash
# Using Homebrew (macOS and Linux)
$ brew install harehare/tap/mq-lsp
```

Make sure you have Rust and Cargo installed on your system. After installation, the `mq-lsp` binary will be available in your system path.
