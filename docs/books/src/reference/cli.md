# CLI

The mq command-line interface provides tools for querying and manipulating markdown content.
Below is the complete reference for all available commands and options.

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
  -R, --raw-input
          Reads each line as a string
  -n, --null-input
          Use empty string as the single input value
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
          Compact instead of pretty-printed output [default: markdown] [possible values: markdown, html, text, json]
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
