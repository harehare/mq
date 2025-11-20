# mq-cli

Command-line interface for the mq Markdown processing tool.

## Overview

`mq-cli` provides the main command-line executable for [mq](https://github.com/harehare/mq), a powerful Markdown query and transformation tool with a jq-like syntax. This crate includes the `mq` binary and optional debugger support.

## Installation

### Using Homebrew (macOS and Linux)

```bash
brew install harehare/tap/mq
```

### Using Cargo

```bash
cargo install mq-cli
```

### From Source

```bash
git clone https://github.com/harehare/mq
cd mq
cargo build --release
```

## Usage

### Basic Query

```bash
# Extract all headings from a markdown file
mq '.h' input.md

# Extract and convert to text
mq '.h | to_text()' input.md

# Filter headings by level
mq '.h1' input.md
```

### Reading from stdin

```bash
echo '# Title\n\nParagraph' | mq '.h'
```

### Multiple Files

```bash
mq '.code' file1.md file2.md file3.md
```

### HTML Input

```bash
mq --input-format html '.h' input.html
```

### Output Formats

```bash
# Output as JSON
mq --output-format json '.h' input.md

# Output as HTML
mq --output-format html '.p' input.md
```

## REPL Mode

Start an interactive REPL session:

```bash
mq repl
```

In REPL mode, you can interactively test queries and see results immediately.

## Language Server

Start the Language Server Protocol (LSP) server:

```bash
mq lsp
```

## Features

- **Powerful Query Syntax**: jq-like syntax for querying and transforming Markdown
- **Multiple Input/Output Formats**: Support for Markdown, HTML, and JSON
- **REPL Mode**: Interactive mode for testing queries
- **LSP Support**: Language server for editor integration
- **Debugger**: Optional debugging support with the `debugger` feature
- **Performance**: Built with Rust for speed and reliability

## Documentation

For detailed documentation and query syntax, visit:
- [Official Documentation](https://mqlang.org/book/)
- [Online Playground](https://mqlang.org/playground)

## License

Licensed under the MIT License.
