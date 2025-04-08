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

> [!WARNING]
> This project is under active development and is not yet production-ready.

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
- **HTML to Markdown**: Easily convert HTML content to Markdown format using [html2md-rs](https://github.com/izyuumi/html2md-rs), making it simple to integrate HTML content into your Markdown workflows.

## Installation

To install `mq`, you can use `cargo`:

```sh
cargo install --git https://github.com/harehare/mq.git mq-cli
# Installing from cargo is under preparation.
cargo install mq-cli
```

### Docker

```sh
$ docker run --rm ghcr.io/harehare/mq:0.1.0-preview
```

### Visual Studio Code Extension

You can install the VSCode extension from the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq).

## Usage

### Basic usage

<details>
<summary>Complete list of options (click to show)</summary>

```sh
Usage: mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl        Start a REPL session for interactive query execution
  fmt         Format mq or markdown files based on specified formatting options
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

For more detailed usage and examples, refer to the [documentation](https://harehare.github.io/mq/book/).

## Playground

An [Online Playground](https://harehare.github.io/mq/playground) is available, powered by WebAssembly.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
