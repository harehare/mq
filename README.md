<div align="center">
  <img src="assets/logo.svg" style="width: 128px; height: 128px;"/>
</div>

<div align="center">
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

<h1 align="center">mq</h1>

[![ci](https://github.com/harehare/mq/actions/workflows/ci.yml/badge.svg)](https://github.com/harehare/mq/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/harehare/mq/graph/badge.svg?token=E4UD7Q9NC3)](https://codecov.io/gh/harehare/mq)
[![CodSpeed Badge](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json?style=for-the-badge)](https://codspeed.io/harehare/mq)
[![audit](https://github.com/harehare/mq/actions/workflows/audit.yml/badge.svg)](https://github.com/harehare/mq/actions/workflows/audit.yml)
[![GitHub Release](https://img.shields.io/github/v/release/harehare/mq)](https://github.com/harehare/mq/releases)
[![Crates.io](https://img.shields.io/crates/v/mq-lang)](https://crates.io/crates/mq-lang)
[![npm](https://img.shields.io/npm/v/mq-web)](https://www.npmjs.com/package/mq-web)

mq is a command-line tool that processes Markdown using a syntax similar to jq.
It's written in Rust, allowing you to easily slice, filter, map, and transform structured data.

![demo](assets/demo.gif)

> [!IMPORTANT]
> This project is under active development.

## Why mq?

mq makes working with Markdown files as easy as jq makes working with JSON. It's especially useful for:

- **LLM Workflows**: Efficiently manipulate and process Markdown used in LLM prompts and outputs
- **LLM Input Generation**: Generate structured Markdown content optimized for LLM consumption, since Markdown serves as the primary input format for most language models
- **Documentation Management**: Extract, transform, and organize content across multiple documentation files
- **Content Analysis**: Quickly extract specific sections or patterns from Markdown documents
- **Batch Processing**: Apply consistent transformations across multiple Markdown files

Since LLM inputs are primarily in Markdown format, mq provides efficient tools for generating and processing the structured Markdown content that LLMs require.

## Features

- **Slice and Filter**: Extract specific parts of your Markdown documents with ease.
- **Map and Transform**: Apply transformations to your Markdown content.
- **Command-line Interface**: Simple and intuitive CLI for quick operations.
- **Extensibility**: Easily extendable with custom functions.
- **Built-in support**: Filter and transform content with many built-in functions and selectors.
- **REPL Support**: Interactive command-line REPL for testing and experimenting.
- **IDE Support**: VSCode Extension and Language Server **Protocol** (LSP) support for custom function development.
- **Debugger**: Includes an experimental debugger (`mq-dbg`) for inspecting and stepping through mq queries interactively.
- **External Subcommands**: Extend mq with custom subcommands by placing executable files starting with `mq-` in `~/.mq/bin/`.

## Installation

### Quick Install

```bash
curl -sSL https://mqlang.org/install.sh | bash
```

The installer will:

- Download the latest mq binary for your platform
- Install it to `~/.mq/bin/`
- Update your shell profile to add mq to your PATH

### Cargo

```sh
# Install from crates.io
cargo install mq-run
# Install from Github
cargo install --git https://github.com/harehare/mq.git mq-run --tag v0.5.14
# Latest Development Version
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq
# Install the debugger
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
# Install using binstall
cargo binstall mq-run@0.5.14
```

### Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Apple Silicon)
curl -L https://github.com/harehare/mq/releases/download/v0.5.14/mq-aarch64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Linux x86_64
curl -L https://github.com/harehare/mq/releases/download/v0.5.14/mq-x86_64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Linux arm64
curl -L https://github.com/harehare/mq/releases/download/v0.5.14/mq-aarch64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.5.14/mq-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq.exe"
```

### Homebrew

```sh
# Using Homebrew (macOS and Linux)
brew install mq
```

### Docker

```sh
$ docker run --rm ghcr.io/harehare/mq:0.5.14
```

### mq-lsp (Language Server)

The mq Language Server provides IDE features like completion, hover, and diagnostics for mq query files.

#### Quick Install

```bash
curl -sSL https://mqlang.org/install_lsp.sh | bash
```

#### Cargo

```sh
# Install from crates.io
cargo install mq-lsp
# Install from Github
cargo install --git https://github.com/harehare/mq.git mq-lsp --tag v0.5.14
# Latest Development Version
cargo install --git https://github.com/harehare/mq.git mq-lsp
# Install using binstall
cargo binstall mq-lsp@0.5.14
```

#### Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Apple Silicon)
curl -L https://github.com/harehare/mq/releases/download/v0.5.14/mq-lsp-aarch64-apple-darwin -o /usr/local/bin/mq-lsp && chmod +x /usr/local/bin/mq-lsp
# Linux x86_64
curl -L https://github.com/harehare/mq/releases/download/v0.5.14/mq-lsp-x86_64-unknown-linux-gnu -o /usr/local/bin/mq-lsp && chmod +x /usr/local/bin/mq-lsp
# Linux arm64
curl -L https://github.com/harehare/mq/releases/download/v0.5.14/mq-lsp-aarch64-unknown-linux-gnu -o /usr/local/bin/mq-lsp && chmod +x /usr/local/bin/mq-lsp
# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.5.14/mq-lsp-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq-lsp.exe"
```

### Visual Studio Code Extension

You can install the VSCode extension from the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq).

### Neovim

You can install the Neovim plugin by following the instructions in the [mq.nvim README](https://github.com/harehare/mq/blob/main/editors/neovim/README.md).

### Zed

You can install the Zed extension by following the instructions in the [zed-mq README](https://github.com/harehare/mq/blob/main/editors/zed/README.md).

### GitHub Actions

You can use mq in your GitHub Actions workflows with the [Setup mq](https://github.com/marketplace/actions/setup-mq) action:

```yaml
steps:
  - uses: actions/checkout@v5
  - uses: harehare/setup-mq@v1
  - run: mq '.code' README.md
```

## Language Bindings

Language bindings are available for the following programming languages:

- [mq_elixir](https://github.com/harehare/mq_elixir)
- [mq-python](https://github.com/harehare/mq/blob/main/crates/mq-python)
- [mq-ruby](https://github.com/harehare/mq-ruby)

## MCP (Model Context Protocol)

mq provides an MCP server implementation that allows AI assistants to process Markdown and HTML content using mq's query language.

- [mq-mcp documentation](https://github.com/harehare/mq-mcp)
- [Getting started with MCP](https://mqlang.org/book/start/mcp)

## Usage

For more detailed usage and examples, refer to the [documentation](https://mqlang.org/book/).

For a comprehensive collection of practical examples, see the [Example Guide](https://mqlang.org/book/start/example/).

### Basic usage

<details>
<summary>Complete list of options (click to show)</summary>

```sh
Usage: mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl   Start a REPL session for interactive query execution
  fmt    Format mq files based on specified formatting options
  docs   Show functions documentation for the query
  check  Check syntax errors in mq files
  help   Print this message or the help of the given subcommand(s)

Arguments:
  [QUERY OR FILE]  
  [FILES]...       

Options:
  -A, --aggregate
          Aggregate all input files/content into a single array
  -f, --from-file
          load filter from the file
  -I, --input-format <INPUT_FORMAT>
          Set input format [possible values: markdown, mdx, html, text, null, raw]
  -L, --directory <MODULE_DIRECTORIES>
          Search modules from the directory
  -M, --module-names <MODULE_NAMES>
          Load additional modules from specified files
      --args <NAME> <VALUE>
          Sets string that can be referenced at runtime
      --rawfile <NAME> <FILE>
          Sets file contents that can be referenced at runtime
      --stream
          Enable streaming mode for processing large files line by line
      --json
          
      --csv
          Include the built-in CSV module
      --fuzzy
          Include the built-in Fuzzy module
      --yaml
          Include the built-in YAML module
      --toml
          Include the built-in TOML module
      --xml
          Include the built-in XML module
      --test
          Include the built-in test module
  -F, --output-format <OUTPUT_FORMAT>
          Set output format [default: markdown] [possible values: markdown, html, text, json, none]
  -U, --update
          Update the input markdown (aliases: -i, --in-place, --inplace)
      --unbuffered
          Unbuffered output
      --list-style <LIST_STYLE>
          Set the list style for markdown output [default: dash] [possible values: dash, plus, star]
      --link-title-style <LINK_TITLE_STYLE>
          Set the link title surround style for markdown output [default: double] [possible values: double, single, paren]
      --link-url-style <LINK_URL_STYLE>
          Set the link URL surround style for markdown links [default: none] [possible values: none, angle]
  -S, --separator <QUERY>
          Specify a query to insert between files as a separator
  -o, --output <FILE>
          Output to the specified file
  -C, --color-output
          Colorize markdown output
      --list
          List all available subcommands (built-in and external)
  -P <PARALLEL_THRESHOLD>
          Number of files to process before switching to parallel processing [default: 10]
  -h, --help
          Print help
  -V, --version
          Print version

# Examples:

## To filter markdown nodes:
mq 'query' file.md

## To read query from file:
mq -f 'file' file.md

## To start a REPL session:
mq repl

## To format mq file:
mq fmt --check file.mq
```

</details>

Here's a basic example of how to use `mq`:

```sh
# Extract all headings from a document
mq '.h' README.md

# Extract code blocks containing "name"
mq '.code | select(contains("name"))' example.md

# Extract code values from code blocks
mq -A 'pluck(.code.value)' example.md

# Extract language names from code blocks
mq '.code.lang' documentation.md

# Extract URLs from all links
mq '.link.url' README.md

# Filter table cells containing "name"
mq '.[][] | select(contains("name"))' data.md

# Select lists or headers containing "name"
mq 'select(.[] || .h) | select(contains("name"))' docs.md

# Exclude JavaScript code blocks
mq '.code | select(.code.lang != "js")' examples.md

# Convert CSV to markdown table
mq 'include "csv" | csv_parse(true) | csv_to_markdown_table()' example.csv
```

### Advanced Usage

You can chain multiple operations to perform complex transformations:

```sh
# Generate a table of contents from headings
mq '.h | let link = to_link("#" + to_text(self), to_text(self), "") | let level = .h.level | if (!is_none(level)): to_md_list(link, level)' docs/books/**/*.md

# String interpolation
mq 'let name = "Alice" | let age = 30 | s"Hello, my name is ${name} and I am ${age} years old."'

# Merge multiple files with separators
mq -S 's"\n${__FILE__}\n"' 'identity()' docs/books/**/**.md

# Extract all code blocks from an HTML file
mq '.code' example.html

# Convert HTML to Markdown and filter headers
mq 'select(.h1 || .h2)' example.html

# Extract specific cell from a Markdown table
mq '.[1][2] | to_text()' data.md
```

### Using with markitdown

You can combine `mq` with [markitdown](https://github.com/microsoft/markitdown) for even more powerful Markdown processing workflows:

```sh
# Extract code blocks from markdown
markitdown https://github.com/harehare/mq | mq '.code'
# Extract table from markdown
markitdown test.xlsx | mq '.[][]'
```

### External Subcommands

You can extend `mq` with custom subcommands by creating executable files starting with `mq-` in `~/.mq/bin/`:

```sh
# Create a custom subcommand
cat > ~/.mq/bin/mq-hello << 'EOF'
#!/bin/bash
echo "Hello from mq-hello!"
echo "Arguments: $@"
EOF
chmod +x ~/.mq/bin/mq-hello

# Use the custom subcommand
mq hello world
# Output: Hello from mq-hello!
#         Arguments: world

# List all available subcommands
mq --list
```

This makes it easy to add your own tools and workflows to `mq` without modifying the core binary.

#### External Tools

The following external tools are available to extend mq's functionality:

- [mq-check](https://github.com/harehare/mq-check) - A syntax and semantic checker for mq files.
- [mq-docs](https://github.com/harehare/mq-docs) - A documentation generator for mq functions, macros, and selectors.
- [mq-mcp](https://github.com/harehare/mq-mcp) - Model Context Protocol (MCP) server implementation for AI assistants
- [mq-task](https://github.com/harehare/mq-task) - Task runner using mq for Markdown-based task definitions
- [mq-tui](https://github.com/harehare/mq-tui) - Terminal User Interface (TUI) for interactive mq query
- [mq-update](https://github.com/harehare/mq-update) - Update mq binary to the latest version
- [mq-view](https://github.com/harehare/mq-view) - viewer for Markdown content

## Color Configuration

Use `-C` (`--color-output`) to enable colorized markdown output:

```sh
mq -C '.h' README.md
```

Customize colors with the `MQ_COLORS` environment variable using `key=SGR` pairs separated by colons:

```sh
# Make headings bold red, code blocks blue
export MQ_COLORS="heading=1;31:code=34"
mq -C '.' README.md
```

Available keys: `heading`, `code`, `code_inline`, `emphasis`, `strong`, `link`, `link_url`, `image`, `blockquote`, `delete`, `hr`, `html`, `frontmatter`, `list`, `table`, `math`.

Set `NO_COLOR=1` to disable all colored output (see [no-color.org](https://no-color.org/)).

For the full color reference, see the [Environment Variables documentation](https://mqlang.org/book/reference/env/).

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- ⭐ [Star the project](https://github.com/harehare/mq) if you find it useful!

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
