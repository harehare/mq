# CLI

The mq command-line interface provides tools for querying and manipulating markdown content.
Below is the complete reference for all available commands and options.

```sh
Usage: mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl        Start a REPL session for interactive query execution
  completion  Generate a shell completion script and print it to stdout
  help        Show documentation for a builtin function, selector, standard module, standard-module function, or the `examples` topic

Arguments:
  [QUERY OR FILE]  
  [FILES]...       

Options:
  -A, --aggregate
          Aggregate all input files/content into a single array
  -f, --from-file
          load filter from the file
  -I, --input-format <INPUT_FORMAT>
          Set input format [possible values: markdown, mdx, html, text, null, raw, bytes, cbor, csv, gron, json, psv, toml, toon, tsv, xml, yaml]
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
      --watch
          Watch the input file(s) for changes and automatically re-run the query whenever they change. Requires at least one input file (stdin cannot be watched). With --from-file, the query file is watched too. Runs until interrupted (Ctrl-C); a query error is printed to stderr and watching continues rather than exiting
      --eval-all
          Evaluate the query once against all input files combined (like yq's `eval-all`), instead of once per file. Enables cross-file aggregation in a single query
      --allow-http-import
          Allow `import`/`include` to fetch modules over HTTP(S). Disabled by default
      --allowed-domain <ALLOWED_DOMAINS>
          Allow HTTP imports from additional domain(s) beyond the default. Has no effect unless `--allow-http-import` (or `--allow-all`) is also passed. Use `github.com/{user}/{repo}` to allow a specific repository (expanded automatically), or a plain domain like `example.com` to allow any path under that host. Repeat to allow multiple extra domains
      --refresh-modules
          Force re-fetch of mutable-ref (HEAD/branch) HTTP-imported modules, ignoring the local cache. Versioned (tagged) modules are never re-fetched regardless of this flag
      --clear-cache
          Remove all HTTP module cache including versioned (tagged) modules and lock files. Use this to fully reset the cache when something goes wrong
      --no-lockfile
          Disable the mq.lock integrity check for HTTP imports. By default a fetched URL's content is checked against mq.lock, and a mismatch is rejected unless --refresh-modules is also passed
      --frozen
          Fail instead of recording a new mq.lock entry. `--frozen`; use in CI so a new module's content is only ever trusted during a reviewable local run whose mq.lock diff gets committed, not silently during CI
      --lockfile <PATH>
          Path to the mq.lock file used for HTTP import integrity checks. Defaults to ./mq.lock (relative to the current directory)
  -N, --allow-net[=<DOMAIN>...]
          Allow the `http` function to make outbound HTTPS requests. Disabled by default; requests are HTTPS-only and blocked from reaching loopback/private/link-local addresses regardless of this flag. Pass with no value to allow any domain, or `--allow-net=DOMAIN` (repeat the flag, or comma-separate, to add more) to restrict requests to just those domains (and any path under them). The `=` is required so a bare domain after the flag isn't swallowed as a query/file positional instead
  -R, --allow-read[=<PATH>...]
          Allow the `read_file`/`read_file_bytes`/`collection`/`file_exists`/`embed_images` functions to read from the filesystem. Disabled by default. Pass with no value to allow reading anywhere, or `--allow-read=PATH` (files or directories; repeat the flag, or comma-separate, to add more) to restrict reads to just those paths and their descendants. The `=` is required so a bare path after the flag isn't swallowed as a query/file positional instead
  -W, --allow-write[=<PATH>...]
          Allow the `write_file`/`extract_images` functions to write to the filesystem. Disabled by default. Pass with no value to allow writing anywhere, or `--allow-write=PATH` (files or directories; repeat the flag, or comma-separate, to add more) to restrict writes to just those paths and their descendants. The `=` is required so a bare path after the flag isn't swallowed as a query/file positional instead
      --allow-run[=<COMMAND>...]
          Allow the `system` function to execute external commands. Disabled by default. Commands run directly (never through a shell), so shell metacharacters in arguments are never interpreted. Pass with no value to allow any command, or `--allow-run=COMMAND` (repeat the flag, or comma-separate, to add more) to restrict execution to just those commands. The `=` is required so a bare command after the flag isn't swallowed as a query/file positional instead
  -E, --allow-env[=<NAME>...]
          Allow `$VAR`/`${$VAR}` interpolation and debugger logpoints to read environment variables. Disabled by default. Pass with no value to allow reading any variable, or `--allow-env=NAME` (repeat the flag, or comma-separate, to add more) to restrict access to just those names. The `=` is required so a bare name after the flag isn't swallowed as a query/file positional instead
  -a, --allow-all
          Grant every sandboxed permission at once (read/write/net/run/env), and also enable HTTP module imports as if --allow-http-import were passed. Disabled by default. Cannot be combined with the individual --allow-* flags above
  -F, --output-format <OUTPUT_FORMAT>
          Set output format [default: markdown] [possible values: markdown, html, text, json, table, grep, gron, raw, csv, toml, toon, xml, yaml, shell, none]
  -U, --update
          Update matching Markdown nodes and write the result to stdout
      --diff
          With --update, print a unified diff instead of the transformed content; nothing is written. Multiple files are diffed one at a time with their path in the headers; stdin is labeled `<stdin>`. Exits 1 if anything would change
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
      --no-position
          Omit Markdown node position information from structured output (json, table, gron, csv, toml, toon, xml, yaml). Reduces output size when source line/column spans aren't needed
      --list
          List all available subcommands (built-in and external)
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

Run `mq help examples` for more usage examples, or `mq help <name>` for
function/selector/module docs.
```
