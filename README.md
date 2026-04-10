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
[![audit](https://github.com/harehare/mq/actions/workflows/audit.yml/badge.svg)](https://github.com/harehare/mq/actions/workflows/audit.yml)
[![crates.io](https://img.shields.io/crates/v/mq-lang)](https://crates.io/crates/mq-lang)
[![codecov](https://codecov.io/gh/harehare/mq/graph/badge.svg?token=E4UD7Q9NC3)](https://codecov.io/gh/harehare/mq)
[![codspeed badge](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json?style=for-the-badge)](https://codspeed.io/harehare/mq)

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
- **External Subcommands**: Extend mq with custom subcommands by placing executable files starting with `mq-` in `~/.local/bin/`.

## Installation

### Quick Install

```bash
curl -sSL https://mqlang.org/install.sh | bash
```

The installer will:

- Download the latest mq binary for your platform
- Install it to `~/.local/bin/`
- Update your shell profile to add mq to your PATH

### Cargo

```sh
# Install from crates.io
cargo install mq-run
# Install from Github
cargo install --git https://github.com/harehare/mq.git mq-run --tag v0.5.25
# Latest Development Version
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq
# Install the debugger
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
# Install using binstall
cargo binstall mq-run@0.5.25
```

### Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Apple Silicon)
curl -L https://github.com/harehare/mq/releases/download/v0.5.25/mq-aarch64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Linux x86_64
curl -L https://github.com/harehare/mq/releases/download/v0.5.25/mq-x86_64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Linux arm64
curl -L https://github.com/harehare/mq/releases/download/v0.5.25/mq-aarch64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.5.25/mq-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq.exe"
```

### Homebrew

```sh
# Using Homebrew (macOS and Linux)
brew install mq
```

### Docker

```sh
$ docker run --rm ghcr.io/harehare/mq:0.5.25
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
cargo install --git https://github.com/harehare/mq.git mq-lsp --tag v0.5.25
# Latest Development Version
cargo install --git https://github.com/harehare/mq.git mq-lsp
# Install using binstall
cargo binstall mq-lsp@0.5.25
```

#### Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Apple Silicon)
curl -L https://github.com/harehare/mq/releases/download/v0.5.25/mq-lsp-aarch64-apple-darwin -o /usr/local/bin/mq-lsp && chmod +x /usr/local/bin/mq-lsp
# Linux x86_64
curl -L https://github.com/harehare/mq/releases/download/v0.5.25/mq-lsp-x86_64-unknown-linux-gnu -o /usr/local/bin/mq-lsp && chmod +x /usr/local/bin/mq-lsp
# Linux arm64
curl -L https://github.com/harehare/mq/releases/download/v0.5.25/mq-lsp-aarch64-unknown-linux-gnu -o /usr/local/bin/mq-lsp && chmod +x /usr/local/bin/mq-lsp
# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.5.25/mq-lsp-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq-lsp.exe"
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
  - uses: actions/checkout@v6
  - uses: harehare/setup-mq@v1
  - run: mq '.code' README.md
```

## Web

### Playground

The [Playground](https://mqlang.org/playground) lets you run mq queries in the browser with no install.

### Web API

You can try mq without installing anything via the hosted REST API at https://api.mqlang.org.

The interactive API documentation is available at [Swagger UI](https://api.mqlang.org/docs).

### mq-web (npm)

[mq-web](https://www.npmjs.com/package/mq-web) is the official WebAssembly build for browser.

## Language Bindings

Language bindings are available for the following programming languages:

- [mq_elixir](https://github.com/harehare/mq_elixir)
- [mq-python](https://github.com/harehare/mq-python)
- [mq-ruby](https://github.com/harehare/mq-ruby)
- [mq-java](https://github.com/harehare/mq-java)
- [mq-go](https://github.com/harehare/mq-go)

## Usage

For more detailed usage and examples, refer to the [documentation](https://mqlang.org/book/).

For a comprehensive collection of practical examples, see the [Example Guide](https://mqlang.org/book/start/example/).

### Basic usage

<details>
<summary>Complete list of options (click to show)</summary>

```sh
Usage: mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl  Start a REPL session for interactive query execution
  fmt   Format mq files based on specified formatting options
  help  Print this message or the help of the given subcommand(s)

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
  -m, --import-module-names <IMPORT_MODULE_NAMES>
          Import modules by name, making them available as `name::fn()` in queries
      --args <NAME> <VALUE>
          Sets string that can be referenced at runtime
      --rawfile <NAME> <FILE>
          Sets file contents that can be referenced at runtime
      --stream
          Enable streaming mode for processing large files line by line
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

# Extract a section by title
mq -A 'section::section("Installation")' README.md

# Filter sections by heading level (scalar or range)
mq -A 'section::sections() | section::by_level(2)' README.md
mq -A 'section::sections() | section::by_level(1..2)' README.md
```

### Composing Workflows with Subcommands

`mq` subcommands are designed to work together via Unix pipes.

```sh
# Convert Excel report to Markdown, then extract all headings
mq conv report.xlsx | mq '.h'

# Convert a Word document and extract a specific section
mq conv document.docx | mq -A 'section::section("Summary")'

# Convert and view Markdown directly in the terminal
mq conv slides.pdf | mq view
```

Run `mq --list` to see all available subcommands (built-in and external).

## External Subcommands

You can extend `mq` with custom subcommands by placing executable files starting with `mq-` in `~/.local/bin/` or anywhere in your `PATH`.
This makes it easy to add your own tools and workflows to `mq` without modifying the core binary.

### External Tools

The following external tools are available to extend mq's functionality:

- [mq-check](https://github.com/harehare/mq/blob/main/crates/mq-check/README.md) - A syntax and semantic checker for mq files
- [mq-conv](https://github.com/harehare/mq-conv) - A CLI tool for converting various file formats to Markdown
- [mq-crawler](https://github.com/harehare/mq/blob/main/crates/mq-crawler/README.md) - A web crawler that extracts structured data from websites and outputs it in Markdown format
- [mq-docs](https://github.com/harehare/mq-docs) - A documentation generator for mq functions, macros, and selectors
- [mq-edit](https://github.com/harehare/mq-edit) - A terminal-based Markdown and code editor with WYSIWYG rendering and LSP support
- [mq-lsp](https://github.com/harehare/mq/tree/main/crates/mq-lsp/README.md) - Language Server Protocol (LSP) implementation for mq query files, providing IDE features like completion, hover, and diagnostics
- [mq-mcp](https://github.com/harehare/mq-mcp) - Model Context Protocol (MCP) server implementation for AI assistants
- [mq-open](https://github.com/harehare/mq-open) - Graphical previewer for mq
- [mq-task](https://github.com/harehare/mq-task) - Task runner using mq for Markdown-based task definitions
- [mq-tui](https://github.com/harehare/mq-tui) - Terminal User Interface (TUI) for interactive mq query
- [mq-update](https://github.com/harehare/mq-update) - Update mq binary to the latest version
- [mq-view](https://github.com/harehare/mq-view) - viewer for Markdown content

### AI Assistant Integration

- MCP: [mq-mcp](https://github.com/harehare/mq-mcp) provides a Model Context Protocol server, enabling mq to be used from any MCP-compatible AI assistant.
- Skill: The [processing-markdown](skills/processing-markdown/SKILL.md) skill adds mq-aware assistance directly to your AI coding workflow.

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- ⭐ [Star the project](https://github.com/harehare/mq) if you find it useful!

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
