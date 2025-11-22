<h1 align="center">mq-cli</h1>

Command-line interface for the mq Markdown processing tool.

> [!NOTE]
> This crate provides the main `mq` binary. For library usage, see the individual crates in the mq ecosystem.

## Why mq?

The command-line interface makes Markdown processing accessible and efficient:

- **Powerful Query Syntax**: jq-like syntax for querying and transforming Markdown
- **Multiple Input Formats**: Process Markdown, MDX, HTML, and plain text
- **Flexible Output**: Export to Markdown, HTML, JSON, or plain text
- **High Performance**: Built with Rust for speed and reliability
- **Extensibility**: Load custom modules and extend functionality
- **Interactive Development**: Built-in REPL and LSP support

## Features

- 🔍 **Query Language**: Use a jq-like syntax to query and transform Markdown documents
- 💡 **Format Conversion**: Convert between Markdown, HTML, JSON, and text formats
- 📖 **REPL Mode**: Interactive command-line environment for testing queries
- 🔧 **LSP Support**: Language Server Protocol integration for editor support
- 🎨 **Code Formatter**: Built-in formatter for mq query files
- ⚡ **Parallel Processing**: Automatically parallelize operations on multiple files
- 🔌 **Module System**: Load and extend functionality with custom modules
- 🐛 **Debugger**: Optional debugger support for stepping through queries

## Installation

### Quick Install

```bash
curl -sSL https://mqlang.org/install.sh | bash
```

### Homebrew

```sh
brew install harehare/tap/mq
```

### Cargo

```sh
# Latest release
cargo install mq-cli

# From git repository
cargo install --git https://github.com/harehare/mq.git mq-cli --tag v0.5.1

# Latest development version
cargo install --git https://github.com/harehare/mq.git mq-cli --bin mq

# Install with debugger support
cargo install --git https://github.com/harehare/mq.git mq-cli --bin mq-dbg --features="debugger"
```

### Docker

```sh
docker run --rm ghcr.io/harehare/mq:0.5.1
```

### Pre-built Binaries

Download from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Intel)
curl -L https://github.com/harehare/mq/releases/download/v0.5.0/mq-x86_64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# macOS (Apple Silicon)
curl -L https://github.com/harehare/mq/releases/download/v0.5.0/mq-aarch64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux x86_64
curl -L https://github.com/harehare/mq/releases/download/v0.5.0/mq-x86_64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux arm64
curl -L https://github.com/harehare/mq/releases/download/v0.5.0/mq-aarch64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.5.0/mq-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq.exe"
```

## Usage

### Basic Examples

```sh
# Extract all headings from a markdown file
mq '.h' input.md

# Extract and convert to text
mq '.h | to_text()' input.md

# Filter headings by level
mq '.h1' input.md

# Extract code blocks with language info
mq '.code | select(contains("name"))' README.md

# Extract code block languages
mq '.code.lang' README.md

# Extract URLs from links
mq '.link.url' README.md

# Filter table cells
mq '.[][] | select(contains("name"))' data.md

# Exclude JavaScript code blocks
mq 'select(!.code("js"))' README.md
```

### Input/Output Formats

```sh
# Process HTML input
mq --input-format html '.h' input.html

# Output as JSON
mq --output-format json '.h' input.md

# Output as HTML
mq --output-format html '.p' input.md

# Output as plain text
mq --output-format text '.h | to_text()' input.md
```

### Advanced Operations

```sh
# Generate table of contents
mq '.h | let link = to_link("#" + to_text(self), to_text(self), "") | let level = .h.level | if (!is_none(level)): to_md_list(link, level)' docs/books/**/*.md

# String interpolation
mq 'let name = "Alice" | let age = 30 | s"Hello, my name is ${name} and I am ${age} years old."'

# Merge multiple files with separators
mq -S 's"\n${__FILE__}\n"' 'identity()' docs/books/**/**.md

# Process multiple files in parallel
mq --parallel-threshold 5 '.code' *.md

# Aggregate all files into a single array
mq --aggregate '.h' file1.md file2.md file3.md
```

### Runtime Arguments

```sh
# Set string arguments
mq --args key value '.h' input.md

# Load file contents
mq --rawfile data data.json '.h' input.md
```

### REPL Mode

Start an interactive REPL session for testing queries:

```sh
mq repl
```

In REPL mode, you can:
- Execute queries interactively
- Test and refine your mq code
- Navigate command history
- See results immediately

### Language Server

Start the LSP server for editor integration:

```sh
mq lsp
```

Use with editors that support LSP (VSCode, Vim, Emacs, etc.) for:
- Syntax highlighting
- Code completion
- Error diagnostics
- Go to definition

### Code Formatting

```sh
# Format a query file
mq fmt query.mq

# Check formatting without modifying
mq fmt --check query.mq
```

### Debugger

When built with the `debugger` feature:

```sh
# Install debugger
cargo install mq-cli --features debugger --bin mq-dbg

# Use debugger
mq-dbg query.mq input.md
```

### CSV Processing

```sh
# Convert CSV to Markdown table
mq 'include "csv" | csv_parse(true) | csv_to_markdown_table()' example.csv
```

### Streaming Mode

For large files, use streaming mode to process line by line:

```sh
mq --stream '.p' large-file.md
```

### Output Options

```sh
# Unbuffered output
mq --unbuffered '.h' input.md

# Save to file
mq -o output.md '.h' input.md

# Update input file in place
mq --update '.h | upcase()' input.md

# Customize Markdown output style
mq --list-style star '.[]' input.md
mq --link-title-style single '.link' input.md
mq --link-url-style angle '.link' input.md
```

## Development

### Building from Source

```sh
git clone https://github.com/harehare/mq
cd mq
cargo build --release
```

The binary will be available at `target/release/mq`.

### Running Tests

```sh
just test
```

### Running Benchmarks

```sh
cargo bench
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
