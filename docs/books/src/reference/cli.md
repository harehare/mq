# CLI

The mq command-line interface provides tools for querying and manipulating markdown content.
Below is the complete reference for all available commands and options.

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
          Set input format [possible values: markdown, mdx, html, text, null, raw, bytes, cbor, csv, hcl, json, psv, toml, toon, tsv, xml, yaml]
  -L, --directory <MODULE_DIRECTORIES>
          Search modules from the directory
  -M, --module-names <MODULE_NAMES>
          Load additional modules from specified files
  -m, --import-module-names <IMPORT_MODULE_NAMES>
          Import modules by name, making them available as `name::fn()` in queries
      --args <NAME> <VALUE>
          Sets a named string argument. NAME is accessible directly in queries, and also via ARGS."named" when --args or --argv is given
      --rawfile <NAME> <FILE>
          Sets file contents that can be referenced at runtime
      --stream
          Enable streaming mode for processing large files line by line
      --allowed-domain <ALLOWED_DOMAINS>
          Allow HTTP imports from additional domain(s) beyond the default. By default only `raw.githubusercontent.com/harehare` is permitted. Use `github.com/{user}/{repo}` to allow a specific repository (expanded automatically), or a plain domain like `example.com` to allow any path under that host. Repeat to allow multiple extra domains
      --refresh-modules
          Force re-fetch of mutable-ref (HEAD/branch) HTTP-imported modules, ignoring the local cache. Versioned (tagged) modules are never re-fetched regardless of this flag
      --clear-cache
          Remove all HTTP module cache including versioned (tagged) modules and lock files. Use this to fully reset the cache when something goes wrong
      --allow-net
          Allow the `http_get`/`http_post` functions to make outbound HTTPS requests. Disabled by default; requests are HTTPS-only and blocked from reaching loopback/private/link-local addresses regardless of this flag
      --allow-write
          Allow the `write_file` function to write to the filesystem. Disabled by default
  -F, --output-format <OUTPUT_FORMAT>
          Set output format [default: markdown] [possible values: markdown, html, text, json, table, grep, raw, none]
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
.hcl  / -I hcl   import "hcl"  | hcl::hcl_parse()
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
