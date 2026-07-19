<div align="center">
  <img src="assets/logo.svg" style="width: 128px; height: 128px;"/>

<a href="https://mqlang.org">Visit the site 🌐</a>
—
<a href="https://mqlang.org/book">Read the book 📖</a>
—
<a href="https://mqlang.org/playground">Playground 🎮</a>

<h1>mq</h1>

**Query. Filter. Transform Markdown.**

[![ci](https://img.shields.io/github/actions/workflow/status/harehare/mq/ci.yml?style=flat-square&logo=github-actions&label=ci)](https://github.com/harehare/mq/actions/workflows/ci.yml)
[![audit](https://img.shields.io/github/actions/workflow/status/harehare/mq/audit.yml?style=flat-square&logo=github-actions&label=audit)](https://github.com/harehare/mq/actions/workflows/audit.yml)
[![crates.io](https://img.shields.io/crates/v/mq-markdown?logo=rust&style=flat-square)](https://crates.io/crates/mq-markdown)
[![codecov](https://img.shields.io/codecov/c/github/harehare/mq?logo=codecov&style=flat-square)](https://codecov.io/gh/harehare/mq)
[![codspeed badge](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json&style=flat-square)](https://codspeed.io/harehare/mq)
[![LICENCE](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)

mq is a command-line tool that processes Markdown using a syntax similar to jq.

It's written in Rust, allowing you to easily slice, filter, map, and transform structured data.

</div>

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
cargo install --git https://github.com/harehare/mq.git mq-run --tag v0.6.5
# Latest Development Version
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq
# Install the debugger
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
# Install using binstall
cargo binstall mq-run@0.6.5
```

### Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Apple Silicon)
curl -L https://github.com/harehare/mq/releases/download/v0.6.5/mq-aarch64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Linux x86_64
curl -L https://github.com/harehare/mq/releases/download/v0.6.5/mq-x86_64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Linux arm64
curl -L https://github.com/harehare/mq/releases/download/v0.6.5/mq-aarch64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq
# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.6.5/mq-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq.exe"
```

### Homebrew

```sh
# Using Homebrew (macOS and Linux)
brew install mq
```

### Arch

```sh
# Using yay (ArchLinux)
yay -S mq-bin
```

### Docker

```sh
$ docker run --rm ghcr.io/harehare/mq:0.6.5
```

### Visual Studio Code Extension

[![Visual Studio Marketplace Version](https://vsmarketplacebadges.dev/version/harehare.vscode-mq.svg?style=flat-square&logo=visualstudiocode)](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq)
[![Open VSX Version](https://img.shields.io/open-vsx/v/harehare/vscode-mq?style=flat-square&logo=eclipseide)](https://open-vsx.org/extension/harehare/vscode-mq)

You can install the VSCode extension from the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq).

For VS Code compatible editors, it is also available on the [Open VSX Registry](https://open-vsx.org/extension/harehare/vscode-mq).

### Neovim

You can install the Neovim plugin by following the instructions in the [mq.nvim README](https://github.com/harehare/mq/blob/main/editors/neovim/README.md).

### Zed

You can install the Zed extension from the [zed-mq](https://github.com/harehare/mq/blob/main/editors/zed/README.md) repository.

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

### mq-web (npm)

[mq-web](https://www.npmjs.com/package/mq-web) is the official WebAssembly build for browser.

## Language Bindings

Language bindings are available for Elixir, Python, Ruby, Java, and Go. See the [Language Bindings documentation](https://mqlang.org/book/start/language_bindings.html) for details.

## Usage

For more detailed usage and examples, refer to the [documentation](https://mqlang.org/book/).

For a comprehensive collection of practical examples, see the [Example Guide](https://mqlang.org/book/start/example/).

### Basic usage

<details>
<summary>Complete list of options (click to show)</summary>

```sh
Usage: mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl        Start a REPL session for interactive query execution
  completion  Generate a shell completion script and print it to stdout
  help        Print this message or the help of the given subcommand(s)

Arguments:
  [QUERY OR FILE]  
  [FILES]...       

Options:
  -A, --aggregate
          Aggregate all input files/content into a single array
  -f, --from-file
          load filter from the file
  -I, --input-format <INPUT_FORMAT>
          Set input format [possible values: markdown, mdx, html, text, null, raw, bytes, cbor, csv, json, psv, toml, toon, tsv, xml, yaml]
  -L, --directory <MODULE_DIRECTORIES>
          Search modules from the directory
  -M, --module-names <MODULE_NAMES>
          Load additional modules from specified files
  -m, --import-module-names <IMPORT_MODULE_NAMES>
          Import modules by name, making them available as `name::fn()` in queries
      --args <NAME> <VALUE>
          Sets a named string argument. NAME is accessible directly in queries, and also via ARGS."named" when --args or --argv is given
      --argjson <NAME> <JSON_VALUE>
          Sets a named JSON argument. NAME is accessible directly in queries
      --rawfile <NAME> <FILE>
          Sets file contents that can be referenced at runtime
      --slurpfile <NAME> <FILE>
          Sets a named argument from a JSON file. NAME is bound to an array of every JSON value found in FILE (jq --slurpfile compatible), so a file containing a single JSON value becomes a one-element array
      --stream
          Enable streaming mode for processing large files line by line
      --allowed-domain <ALLOWED_DOMAINS>
          Allow HTTP imports from additional domain(s) beyond the default. By default only `raw.githubusercontent.com/harehare` is permitted. Use `github.com/{user}/{repo}` to allow a specific repository (expanded automatically), or a plain domain like `example.com` to allow any path under that host. Repeat to allow multiple extra domains
      --refresh-modules
          Force re-fetch of mutable-ref (HEAD/branch) HTTP-imported modules, ignoring the local cache. Versioned (tagged) modules are never re-fetched regardless of this flag
      --clear-cache
          Remove all HTTP module cache including versioned (tagged) modules and lock files. Use this to fully reset the cache when something goes wrong
      --no-lockfile
          Disable the mq.lock integrity check for HTTP imports. By default a fetched URL's content is checked against mq.lock, and a mismatch is rejected unless --refresh-modules is also passed
      --lockfile <PATH>
          Path to the mq.lock file used for HTTP import integrity checks. Defaults to ./mq.lock (relative to the current directory)
      --allow-net
          Allow the `http` function to make outbound HTTPS requests. Disabled by default; requests are HTTPS-only and blocked from reaching loopback/private/link-local addresses regardless of this flag
      --allow-read
          Allow the `read_file`/`read_file_bytes`/`collection`/`file_exists` functions to read from the filesystem. Disabled by default
      --allow-write
          Allow the `write_file` function to write to the filesystem. Disabled by default
  -F, --output-format <OUTPUT_FORMAT>
          Set output format [default: markdown] [possible values: markdown, html, text, json, table, grep, raw, csv, toml, xml, yaml, none]
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
  -B, --before-context <NUM>
          Show NUM nodes before each match. Only effective with -F grep
      --after-context <NUM>
          Show NUM nodes after each match. Only effective with -F grep
      --context <NUM>
          Show NUM nodes before and after each match. Only effective with -F grep
  -e, --exit-status
          Exit with code 1 if the last output value is false, null, or the output is empty. Mirrors jq's --exit-status / -e flag
  -c, --count
          Output only the count of matching (non-None) results. Mirrors grep -c. With multiple files, prints "filename: N" per file and "total: N" at the end
      --skip <N>
          Skip the first N matching results before outputting
      --limit <N>
          Limit output to at most N results
      --list
          List all available subcommands (built-in and external)
      --doc
          Use the built-in reference document as input instead of a file
  -P <PARALLEL_THRESHOLD>
          Number of files to process before switching to parallel processing [default: 10]
      --argv [<ARGV>...]
          Positional string arguments, available as ARGS."positional" in queries
  -O, --optimize-level <OPTIMIZE_LEVEL>
          Optimization level for AST transformations (none = no changes, basic = constant folding and dead-branch elimination, full = all passes) [default: none] [possible values: none, basic, full]
      --timeout <SECONDS>
          Maximum time in seconds allowed for query evaluation before aborting (e.g. 0.5, 5). No timeout by default
  -h, --help
          Print help
  -V, --version
          Print version

# Examples

mq 'query' file.md
mq -f 'file' file.md        # read query from file
mq repl                     # start a REPL session

# Auto-parsing by file extension or -I flag

mq automatically imports the matching module based on the file extension.
Use -I <format> to force a specific format:

.cbor / -I cbor  import "cbor" | cbor::cbor_parse()  (reads as bytes)
.csv  / -I csv   import "csv"  | csv::csv_parse(true)
.json / -I json  import "json" | json::json_parse()
.psv  / -I psv   import "csv"  | csv::psv_parse(true)
.toml / -I toml  import "toml" | toml::toml_parse()
.toon / -I toon  import "toon" | toon::toon_parse()
.tsv  / -I tsv   import "csv"  | csv::tsv_parse(true)
.xml  / -I xml   import "xml"  | xml::xml_parse()
.yaml / -I yaml  import "yaml" | yaml::yaml_parse()

Use -I raw   to disable auto-parsing and receive the raw string.
Use -I bytes to read input as raw bytes without parsing.

# Passing arguments to queries (ARGS)

When --args or --argv is given, ARGS = {"positional": [...], "named": {...}}

mq -I null 'name' --args name Alice
mq -I null 'ARGS | ."named"' --args name Alice
# => {"name": "Alice"}

mq -I null 'ARGS | ."positional"' --argv x y z  # must come after query and files
# => ["x", "y", "z"]

mq -I null 'ARGS' file.md --args name Alice --argv x y z
# => {"positional": ["x","y","z"], "named": {"name": "Alice"}}
```

</details>

Here's a basic example of how to use `mq`:

```sh
# Extract all headings from a document
mq '.h' README.md

# Extract only h1 headings
mq '.h(1)' README.md

# Extract h1 and h2 headings
mq '.h(1, 2)' README.md

# Extract headings from level 1 to 3 using a range
mq '.h(1..3)' README.md

# Extract only Rust code blocks
mq '.code("rust")' example.md

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
mq 'csv::csv_to_markdown_table' example.csv

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

See the [External Subcommands documentation](https://mqlang.org/book/start/external_subcommands) for the full list and details.

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues/new?template=bug_report.yml)
- 💡 [Request features](https://github.com/harehare/mq/issues/new?template=feature_request.yml)
- ⭐ [Star the project](https://github.com/harehare/mq) if you find it useful!

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding conventions, and how to submit changes. Please also review our [Code of Conduct](.github/CODE_OF_CONDUCT.md).

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
