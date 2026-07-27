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

Downloads the latest mq binary for your platform, installs it to `~/.local/bin/`, and updates your shell profile to add mq to your PATH.

### Package Managers

| Method                 | Command                                     |
| ---------------------- | ------------------------------------------- |
| Homebrew (macOS/Linux) | `brew install mq`                           |
| Arch (yay)             | `yay -S mq-bin`                             |
| Cargo (crates.io)      | `cargo install mq-run`                      |
| Docker                 | `docker run --rm ghcr.io/harehare/mq:0.7.0` |

<details>
<summary>More install options: cargo variants, binstall, pre-built binaries</summary>

```sh
# Install from Github
cargo install --git https://github.com/harehare/mq.git mq-run --tag v0.7.0
# Latest Development Version
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq
# Install the debugger
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
# Install using binstall
cargo binstall mq-run@0.7.0
```

Pre-built binaries for macOS, Linux, and Windows are also available on the [GitHub releases page](https://github.com/harehare/mq/releases).

</details>

### Editor & CI Integrations

| Integration    | Link                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| VS Code        | [![Visual Studio Marketplace Version](https://vsmarketplacebadges.dev/version/harehare.vscode-mq.svg?style=flat-square&logo=visualstudiocode)](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq) [![Open VSX Version](https://img.shields.io/open-vsx/v/harehare/vscode-mq?style=flat-square&logo=eclipseide)](https://open-vsx.org/extension/harehare/vscode-mq) |
| Neovim         | [![Neovim README](https://img.shields.io/badge/neovim-README-57A143?style=flat-square&logo=neovim&logoColor=white)](https://github.com/harehare/mq/blob/main/editors/neovim/README.md)                                                                                                                                                                                                  |
| Zed            | [![Zed README](https://img.shields.io/badge/zed-README-084CCF?style=flat-square&logo=zed&logoColor=white)](https://github.com/harehare/mq/blob/main/editors/zed/README.md)                                                                                                                                                                                                              |
| GitHub Actions | [![Setup mq](https://img.shields.io/badge/marketplace-Setup%20mq-2088FF?style=flat-square&logo=githubactions&logoColor=white)](https://github.com/marketplace/actions/setup-mq)                                                                                                                                                                                                         |

```yaml
steps:
  - uses: actions/checkout@v7
  - uses: harehare/setup-mq@v1
  - run: mq '.code' README.md
```

## Packages

mq is a Rust + TypeScript monorepo.

<details>
<summary>Rust crates and npm packages (click to show)</summary>

| Name                                  | Description                                           | Version                                                                                                                |
| -------------------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| [`mq-run`](crates/mq-run)             | Command-line interface for mq                         | [![Crates.io](https://img.shields.io/crates/v/mq-run?style=flat-square)](https://crates.io/crates/mq-run)             |
| [`mq-lang`](crates/mq-lang)           | Core language implementation                          | [![Crates.io](https://img.shields.io/crates/v/mq-lang?style=flat-square)](https://crates.io/crates/mq-lang)           |
| [`mq-markdown`](crates/mq-markdown)   | Markdown parsing and manipulation utilities           | [![Crates.io](https://img.shields.io/crates/v/mq-markdown?style=flat-square)](https://crates.io/crates/mq-markdown)   |
| [`mq-lsp`](crates/mq-lsp)             | Language Server Protocol implementation               | [![Crates.io](https://img.shields.io/crates/v/mq-lsp?style=flat-square)](https://crates.io/crates/mq-lsp)             |
| [`mq-repl`](crates/mq-repl)           | Interactive REPL                                      | [![Crates.io](https://img.shields.io/crates/v/mq-repl?style=flat-square)](https://crates.io/crates/mq-repl)           |
| [`mq-formatter`](crates/mq-formatter) | Code formatter for the mq query language              | [![Crates.io](https://img.shields.io/crates/v/mq-formatter?style=flat-square)](https://crates.io/crates/mq-formatter) |
| [`mq-lint`](crates/mq-lint)           | Static analysis linter                                | [![Crates.io](https://img.shields.io/crates/v/mq-lint?style=flat-square)](https://crates.io/crates/mq-lint)           |
| [`mq-check`](crates/mq-check)         | Type checker                                          | [![Crates.io](https://img.shields.io/crates/v/mq-check?style=flat-square)](https://crates.io/crates/mq-check)         |
| [`mq-hir`](crates/mq-hir)             | High-level Internal Representation (HIR)              | [![Crates.io](https://img.shields.io/crates/v/mq-hir?style=flat-square)](https://crates.io/crates/mq-hir)             |
| [`mq-dap`](crates/mq-dap)             | Debug Adapter Protocol implementation                 | [![Crates.io](https://img.shields.io/crates/v/mq-dap?style=flat-square)](https://crates.io/crates/mq-dap)             |
| [`mq-crawler`](crates/mq-crawler)     | Directory crawler for batch Markdown processing       | [![Crates.io](https://img.shields.io/crates/v/mq-crawler?style=flat-square)](https://crates.io/crates/mq-crawler)     |
| [`mq-web-api`](crates/mq-web-api)     | HTTP/REST server exposing mq queries over the network | [![Crates.io](https://img.shields.io/crates/v/mq-web-api?style=flat-square)](https://crates.io/crates/mq-web-api)     |
| [`mq-macros`](crates/mq-macros)       | Procedural macros for builtin function registration   | [![Crates.io](https://img.shields.io/crates/v/mq-macros?style=flat-square)](https://crates.io/crates/mq-macros)       |
| [`mq-test`](crates/mq-test)           | Test runner for mq                                    | [![Crates.io](https://img.shields.io/crates/v/mq-test?style=flat-square)](https://crates.io/crates/mq-test)           |
| [`mq-ffi`](crates/mq-ffi)             | C API for integrating mq into C applications          | —                                                                                                                     |
| [`mq-wasm`](crates/mq-wasm)           | WebAssembly bindings                                  | —                                                                                                                     |
| [`mq-web`](packages/mq-web)           | Official WebAssembly build for the browser            | [![npm](https://img.shields.io/npm/v/mq-web?style=flat-square)](https://www.npmjs.com/package/mq-web)                 |
| [`mq-nodejs`](packages/mq-nodejs)     | Node.js bindings                                      | [![npm](https://img.shields.io/npm/v/mq-nodejs?style=flat-square)](https://www.npmjs.com/package/mq-nodejs)           |

</details>

mq is also available as a hosted REST API (see [`mq-web-api`](crates/mq-web-api) above). See the [REST API docs](https://mqlang.org/book/start/web_api) or try it in the [Playground](https://mqlang.org/playground).

```bash
curl --data-binary @doc.md https://api.mqlang.org/.h1
```

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
      --eval-all
          Evaluate the query once against all input files combined (like yq's `eval-all`), instead of once per file. Enables cross-file aggregation in a single query
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
          Update matching Markdown nodes and write the result to stdout
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
