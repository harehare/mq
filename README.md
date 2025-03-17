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
Usage: mq [OPTIONS] [QUERY] [FILES]... [COMMAND]

Commands:
  repl        Start a REPL session for interactive query execution
  fmt         Format mq or markdown files based on specified formatting options
  completion  Generate shell completion scripts for supported shells
  docs
  help        Print this message or the help of the given subcommand(s)

Arguments:
  [QUERY]
  [FILES]...

Options:
  -f, --from-file <FROM_FILE>           load filter from the file
  -R, --raw-input                       Reads each line as a string
  -n, --null-input                      Use empty string as the single input value
  -L, --directory <MODULE_DIRECTORIES>  Search modules from the directory
  -M, --module-names <MODULE_NAMES>     Load additional modules from specified files
      --args <NAME> <VALUE>             Sets string that can be referenced at runtime
      --raw-file <NAME> <FILE>          Sets file contents that can be referenced at runtime
  -c, --compact-output                  pretty print
  -F, --output-format <OUTPUT_FORMAT>   Compact instead of pretty-printed output [default: markdown] [possible values: markdown, html, text]
  -U, --update                          Update the input markdown
      --unbuffered                      Unbuffered output
      --list-style <LIST_STYLE>         Set the list style for markdown output [default: dash] [possible values: dash, plus, star]
  -v, --verbose...                      Increase logging verbosity
  -q, --quiet...                        Decrease logging verbosity
  -h, --help                            Print help
  -V, --version                         Print version
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

This example how to generate a table of contents (TOC) from a markdown file.
You can chain multiple operations to perform complex transformations:

```sh
$ mq 'select(or(.h1, .h2, .h3)) | let link = to_link(add($__FILE__, add("#", to_text(self))), to_text(self)) | if (is_h1()): to_md_list(link, 1)  elif (is_h2()): to_md_list(link, 2) elif (is_h3()): to_md_list(link, 3) else: None' docs/book/*.md
```

For more detailed usage and examples, refer to the [documentation](https://harehare.github.io/mq/book/).

## Playground

An [Online Playground](https://harehare.github.io/mq/playground) is available, powered by WebAssembly.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
