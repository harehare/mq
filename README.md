<div align="center">
  <img src="docs/assets/logo.svg" style="width: 128px; height: 128px;"/>
</div>

<div align="center">
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

# mq

[![ci](https://github.com/harehare/mq/actions/workflows/ci.yml/badge.svg)](https://github.com/harehare/mq/actions/workflows/ci.yml)
![GitHub Release](https://img.shields.io/github/v/release/harehare/mq)
[![codecov](https://codecov.io/gh/harehare/mq/graph/badge.svg?token=E4UD7Q9NC3)](https://codecov.io/gh/harehare/mq)
[![CodSpeed Badge](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json?style=for-the-badge)](https://codspeed.io/harehare/mq)
![](https://tokei.rs/b1/github/harehare/mq?category=code)
[![DeepWiki](https://img.shields.io/badge/DeepWiki-harehare%2Fmq-blue.svg?logo=data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACwAAAAyCAYAAAAnWDnqAAAAAXNSR0IArs4c6QAAA05JREFUaEPtmUtyEzEQhtWTQyQLHNak2AB7ZnyXZMEjXMGeK/AIi+QuHrMnbChYY7MIh8g01fJoopFb0uhhEqqcbWTp06/uv1saEDv4O3n3dV60RfP947Mm9/SQc0ICFQgzfc4CYZoTPAswgSJCCUJUnAAoRHOAUOcATwbmVLWdGoH//PB8mnKqScAhsD0kYP3j/Yt5LPQe2KvcXmGvRHcDnpxfL2zOYJ1mFwrryWTz0advv1Ut4CJgf5uhDuDj5eUcAUoahrdY/56ebRWeraTjMt/00Sh3UDtjgHtQNHwcRGOC98BJEAEymycmYcWwOprTgcB6VZ5JK5TAJ+fXGLBm3FDAmn6oPPjR4rKCAoJCal2eAiQp2x0vxTPB3ALO2CRkwmDy5WohzBDwSEFKRwPbknEggCPB/imwrycgxX2NzoMCHhPkDwqYMr9tRcP5qNrMZHkVnOjRMWwLCcr8ohBVb1OMjxLwGCvjTikrsBOiA6fNyCrm8V1rP93iVPpwaE+gO0SsWmPiXB+jikdf6SizrT5qKasx5j8ABbHpFTx+vFXp9EnYQmLx02h1QTTrl6eDqxLnGjporxl3NL3agEvXdT0WmEost648sQOYAeJS9Q7bfUVoMGnjo4AZdUMQku50McDcMWcBPvr0SzbTAFDfvJqwLzgxwATnCgnp4wDl6Aa+Ax283gghmj+vj7feE2KBBRMW3FzOpLOADl0Isb5587h/U4gGvkt5v60Z1VLG8BhYjbzRwyQZemwAd6cCR5/XFWLYZRIMpX39AR0tjaGGiGzLVyhse5C9RKC6ai42ppWPKiBagOvaYk8lO7DajerabOZP46Lby5wKjw1HCRx7p9sVMOWGzb/vA1hwiWc6jm3MvQDTogQkiqIhJV0nBQBTU+3okKCFDy9WwferkHjtxib7t3xIUQtHxnIwtx4mpg26/HfwVNVDb4oI9RHmx5WGelRVlrtiw43zboCLaxv46AZeB3IlTkwouebTr1y2NjSpHz68WNFjHvupy3q8TFn3Hos2IAk4Ju5dCo8B3wP7VPr/FGaKiG+T+v+TQqIrOqMTL1VdWV1DdmcbO8KXBz6esmYWYKPwDL5b5FA1a0hwapHiom0r/cKaoqr+27/XcrS5UwSMbQAAAABJRU5ErkJggg==)](https://deepwiki.com/harehare/mq)


mq is a command-line tool that processes Markdown using a syntax similar to jq.
It's written in Rust, allowing you to easily slice, filter, map, and transform structured data.

![demo](assets/demo.gif)

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
- **IDE Support**: VSCode Extension and Language Server **Protocol** (LSP) support for custom function development.

## Installation

### Cargo

```sh
$ cargo install --git https://github.com/harehare/mq.git mq-cli --tag v0.2.22
# Latest Development Version
$ cargo install --git https://github.com/harehare/mq.git mq-cli --bin mq
```

### Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Intel)
$ curl -L https://github.com/harehare/mq/releases/download/v0.2.8/mq-x86_64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# macOS (Apple Silicon)
$ curl -L https://github.com/harehare/mq/releases/download/v0.2.8/mq-aarch64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux x86_64
$ curl -L https://github.com/harehare/mq/releases/download/v0.2.8/mq-x86_64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux arm64
$ curl -L https://github.com/harehare/mq/releases/download/v0.2.8/mq-aarch64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Windows (PowerShell)
$ Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.2.8/mq-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq.exe"
```

### Homebrew

```sh
# Using Homebrew (macOS and Linux)
$ brew install harehare/tap/mq
```

### Docker

```sh
$ docker run --rm ghcr.io/harehare/mq:0.2.22
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

## Python

You can use mq in Python through the `markdown-query` package:

```sh
# Install from PyPI
pip install markdown-query
```

## MCP (Model Context Protocol)

mq provides an MCP server implementation that allows AI assistants to process Markdown and HTML content using mq's query language.

- [mq-mcp documentation](crates/mq-mcp/README.md)
- [Getting started with MCP](https://mqlang.org/book/start/mcp)

## Usage

For more detailed usage and examples, refer to the [documentation](https://mqlang.org/book/).

### Basic usage

<details>
<summary>Complete list of options (click to show)</summary>

```sh
Usage: mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl  Start a REPL session for interactive query execution
  lsp   Start a language server for mq
  mcp   Start an MCP server for mq
  tui   Start a TUI for mq
  fmt   Format mq files based on specified formatting options
  docs  Show functions documentation for the query
  help  Print this message or the help of the given subcommand(s)

Arguments:
  [QUERY OR FILE]  
  [FILES]...       

Options:
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
      --yaml
          Include the built-in YAML module
      --toml
          Include the built-in TOML module
      --xml
          Include the built-in XML module
      --test
          Include the built-in test module
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
  -S, --separator <QUERY>
          Specify a query to insert between files as a separator
  -o, --output <FILE>
          Output to the specified file
  -P <PARALLEL_THRESHOLD>
          Number of files to process before switching to parallel processing [default: 10]
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
# Extracts the language name from code blocks.
$ mq '.code.lang'
# Extracts the url from link.
$ mq '.link.url'
# table
$ mq '.[][] | select(contains("name"))'
# list or header
$ mq 'select(.[] || .h) | select(contains("name"))'
# Exclude js code
$ mq 'select(!.code("js"))'
# CSV to markdown table
$ mq 'include "csv" | csv_parse(true) | csv_to_markdown_table()' example.csv
```

### Advanced Usage

You can chain multiple operations to perform complex transformations:

```sh
# Markdown TOC
$ mq '.h | let link = to_link("#" + to_text(self), to_text(self), "") | let level = .h.level | if (!is_none(level)): to_md_list(link, to_number(level))' docs/books/**/*.md
# String Interpolation
$ mq 'let name = "Alice" | let age = 30 | s"Hello, my name is ${name} and I am ${age} years old."'
# Merging Multiple Files
$ mq -S 's"\n${__FILE__}\n"' 'identity()' docs/books/**/**.md
# Extract all code blocks from an HTML file
$ mq '.code' example.html
# Convert HTML to Markdown and filter headers
$ mq 'select(.h1 || .h2)' example.html
```

### Using with markitdown

You can combine `mq` with [markitdown](https://github.com/microsoft/markitdown) for even more powerful Markdown processing workflows:

```sh
# Extract code blocks from markdown
$ markitdown https://github.com/harehare/mq | mq '.code'

# Extract table from markdown
$ markitdown test.xlsx | mq '.[][]'
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- ⭐ [Star the project](https://github.com/harehare/mq) if you find it useful!

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
