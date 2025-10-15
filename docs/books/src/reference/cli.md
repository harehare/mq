# CLI

The mq command-line interface provides tools for querying and manipulating markdown content.
Below is the complete reference for all available commands and options.

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
