<div align="center">
    <img src="docs/assets/logo.svg" style="width: 128px; height: 128px; margin-right: 10px;"/>
</div>

# `mq` - jq like tool for markdown processing

[![ci](https://github.com/harehare/mq/actions/workflows/ci.yml/badge.svg)](https://github.com/harehare/mq/actions/workflows/ci.yml)
![GitHub Release](https://img.shields.io/github/v/release/harehare/mq)
[![codecov](https://codecov.io/gh/harehare/mq/graph/badge.svg?token=E4UD7Q9NC3)](https://codecov.io/gh/harehare/mq)
[![CodSpeed Badge](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://codspeed.io/harehare/mq)

mq is a command-line tool that processes Markdown using a syntax similar to jq.
It's written in Rust, allowing you to easily slice, filter, map, and transform structured data.

> [!IMPORTANT]
> This project is under active development.

## Why mq?

mq makes working with Markdown files as easy as jq makes working with JSON. It's especially useful for:

- **LLM Workflows**: Efficiently manipulate and process Markdown used in LLM prompts and outputs
- **Documentation Management**: Extract, transform, and organize content across multiple documentation files
- **Content Analysis**: Quickly extract specific sections or patterns from Markdown documents
- **Batch Processing**: Apply consistent transformations across multiple Markdown files

## Features

- **Slice and Filter**: Extract specific parts of your Markdown documents with ease.
- **Map and Transform**: Apply transformations to your Markdown content.
- **Command-line Interface**: Simple and intuitive CLI for quick operations.
- **Extensibility**: Easily extendable with custom functions.
- **Built-in support**: Filter and transform content with many built-in functions and selectors.
- **REPL Support**: Interactive command-line REPL for testing and experimenting.
- **IDE Support**: VSCode Extension and Language Server Protocol (LSP) support for custom function development.

## Installation

### Cargo

```sh
$ cargo install --git https://github.com/harehare/mq.git mq-cli --tag v0.1.1
# Latest Development Version
$ cargo install --git https://github.com/harehare/mq.git mq-cli
```

### Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Intel)
$ curl -L https://github.com/harehare/mq/releases/download/v0.1.1/mq-x86_64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# macOS (Apple Silicon)
$ curl -L https://github.com/harehare/mq/releases/download/v0.1.1/mq-aarch64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux x86_64
$ curl -L https://github.com/harehare/mq/releases/download/v0.1.1/mq-x86_64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux arm64
$ curl -L https://github.com/harehare/mq/releases/download/v0.1.1/mq-aarch64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Windows (PowerShell)
$ Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.1.1/mq-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq.exe"
```

### Homebrew

```sh
# Using Homebrew (macOS and Linux)
$ brew install harehare/tap/mq
```

### Docker

```sh
$ docker run --rm ghcr.io/harehare/mq:0.1.1
```

### Visual Studio Code Extension

You can install the VSCode extension from the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq).

### GitHub Actions

You can use mq in your GitHub Actions workflows with the [Setup mq](https://github.com/marketplace/actions/setup-mq) action:

```yaml
steps:
  - uses: actions/checkout@v4
  - uses: harehare/setup-mq@v1
  - run: mq '.code' README.md
```

## MCP (Model Context Protocol) server

mq supports an MCP server for integration with LLM applications.

See the [MCP documentation](https://github.com/harehare/mq/blob/main/crates/mq-mcp/README.md) for more information.

## Usage

For more detailed usage and examples, refer to the [documentation](https://mqlang.org/book/).

### Basic usage

<details>
<summary>Complete list of options (click to show)</summary>

```sh
Usage: mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl        Start a REPL session for interactive query execution
  fmt         Format mq files based on specified formatting options
  completion  Generate shell completion scripts for supported shells
  docs        Show functions documentation for the query
  help        Print this message or the help of the given subcommand(s)

Arguments:
  [QUERY OR FILE]
  [FILES]...

Options:
  -f, --from-file
          load filter from the file
  -I, --input-format <INPUT_FORMAT>
          Set input format [default: markdown] [possible values: markdown, html, text, null]
  -L, --directory <MODULE_DIRECTORIES>
          Search modules from the directory
  -M, --module-names <MODULE_NAMES>
          Load additional modules from specified files
      --args <NAME> <VALUE>
          Sets string that can be referenced at runtime
      --rawfile <NAME> <FILE>
          Sets file contents that can be referenced at runtime
      --mdx
          Enable MDX parsing
  -F, --output-format <OUTPUT_FORMAT>
          Set output format [default: markdown] [possible values: markdown, html, text, json]
  -U, --update
          Update the input markdown
      --unbuffered
          Unbuffered output
      --list-style <LIST_STYLE>
          Set the list style for markdown output [default: dash] [possible values: dash, plus, star]
      --link-title-style <LINK_TITLE_STYLE>
          Set the link title surround style for markdown output [default: double] [possible values: double, single, paren]
      --link-url-style <LINK_URL_STYLE>
          Set the link URL surround style for markdown links [default: none] [possible values: none, angle]
  -o, --output <FILE>
          Output to the specified file
  -v, --verbose...
          Increase logging verbosity
  -q, --quiet...
          Decrease logging verbosity
  -h, --help
          Print help
  -V, --version
          Print version

Examples:

To filter markdown nodes:
$ mq 'query' file.md

To read query from file:
$ mq -f 'file' file.md

To start a REPL session:
$ mq repl

To format mq file:
$ mq fmt --check file.mq
```

</details>

Here's a basic example of how to use `mq`:

```sh
# code
$ mq '.code | select(contains("name"))'
# table
$ mq '.[][] | select(contains("name"))'
# list or header
$ mq 'or(.[], .h) | select(contains("name"))'
# Exclude js code
$ mq 'select(not(.code("js")))'
```

### Advanced Usage

You can chain multiple operations to perform complex transformations:

```sh
# Markdown TOC
$ mq 'select(or(.h1, .h2, .h3)) | let link = to_link(add($__FILE__, add("#", to_text(self))), to_text(self), "") | if (is_h1()): to_md_list(link, 1)  elif (is_h2()): to_md_list(link, 2) elif (is_h3()): to_md_list(link, 3) else: None' docs/book/*.md
# String Interpolation
$ mq 'let name = "Alice" | let age = 30 | s"Hello, my name is ${name} and I am ${age} years old."'
```

### Using with markitdown

You can combine `mq` with [markitdown](https://github.com/microsoft/markitdown) for even more powerful Markdown processing workflows:

```sh
# Extract code blocks from markdown
$ markitdown https://github.com/harehare/mq | mq '.code'

# Extract table from markdown
$ markitdown test.xlsx | mq '.[][]'

```

## Development

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [just](https://github.com/casey/just) - a command runner
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) (optional, for WebAssembly support)

### Setting up the development environment

Clone the repository:

```sh
git clone https://github.com/harehare/mq.git
cd mq
```

Install development dependencies:

```sh
# Using cargo
cargo install just wasm-pack
```

Or if you prefer using asdf:

```sh
# Using asdf
asdf install
```

### Common development tasks

Here are some useful commands to help you during development:

```sh
# Run the CLI with the provided arguments
just run '.code'

# Run formatting, linting and all tests
just test

# Run formatter and linter
just lint

# Build the project in release mode
just build

# Update documentation
just docs
```

Check the `just --list` for more available commands and build options.

## Playground

An [Online Playground](https://mqlang.org/playground) is available, powered by WebAssembly.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
