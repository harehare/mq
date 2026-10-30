# CLI

The mq command-line interface provides tools for querying and manipulating markdown content.
Below is the complete reference for all available commands and options.

```sh
Usage: mq [OPTIONS] [QUERY OR FILE] [FILES]... [COMMAND]

Commands:
  repl        Start a REPL session for interactive query execution. Optional FILES are combined as the initial input (same file/format handling as `mq QUERY FILES...`)
  commands    List built-in and external subcommands
  completion  Generate a shell completion script and print it to stdout
  help        Show documentation for a builtin function, selector, standard module, standard-module function, or the `examples` topic
  compile     Compile a query to experimental `.mqc` bytecode

Arguments:
  [QUERY OR FILE]  
  [FILES]...       

Options:
  -q, --quiet
          Suppress normal query output while preserving diagnostics, exit status, and query side effects
  -A, --aggregate
          Collect each input's nodes into a single array before the query runs. Combine with `--eval-all` to get one array across all files
  -I, --input-format <INPUT_FORMAT>
          Set input format [possible values: markdown, mdx, html, text, null, raw, bytes, cbor, csv, gron, json, jsonl, psv, toml, toon, tsv, xml, yaml]
      --csv-delimiter <CHAR>
          Custom delimiter for `-I csv` input (a single ASCII character). Has no effect on `-I tsv`/`-I psv`, which use a fixed tab/pipe delimiter by design; pass `-I csv` with this flag instead if you need a different delimiter (e.g. `;`)
      --no-header
          Treat csv/tsv/psv input as headerless: each row becomes an array of values instead of a dict keyed by header names. Applies to `-I csv`, `-I tsv`, and `-I psv`
      --no-frontmatter
          Do not read frontmatter at the start of Markdown or MDX input: the lines between `---` or `+++` are then text, a heading or a rule. Applies to `-I markdown` and `-I mdx`, and to `.md` and `.mdx` files
  -L, --directory <MODULE_DIRECTORIES>
          Search modules from the directory
  -M, --module-names <MODULE_NAMES>
          Load additional modules by name
  -m, --import-module-names <IMPORT_MODULE_NAMES>
          Import modules by name, making them available as `name::fn()` in queries
      --allow-http-import[=<DOMAIN>...]
          Allow `import`/`include` to fetch modules over HTTP(S). Disabled by default. `--allow-http-import=DOMAIN` (repeat the flag, or comma-separate) also allows those domains beyond the default. Use `github.com/{user}/{repo}` for one repository (expanded automatically), or a plain domain like `example.com` for any path under that host. The `=` is required so a bare domain isn't taken as a query/file positional
      --refresh-modules
          Force re-fetch of mutable-ref (HEAD/branch) HTTP-imported modules, ignoring the local cache. Versioned (tagged) modules are never re-fetched regardless of this flag
      --clear-cache
          Remove all HTTP module cache including versioned (tagged) modules and lock files. Use this to fully reset the cache when something goes wrong
      --no-lockfile
          Disable the mq.lock integrity check for HTTP imports. By default a fetched URL's content is checked against mq.lock, and a mismatch is rejected unless --refresh-modules is also passed
      --frozen
          Fail instead of recording a new mq.lock entry. Use in CI so a new module's content is only trusted during a reviewable local run whose mq.lock diff gets committed
      --lockfile <PATH>
          Path to the mq.lock file used for HTTP import integrity checks. Defaults to ./mq.lock (relative to the current directory)
  -N, --allow-net[=<DOMAIN>...]
          Allow the `http` function to make outbound HTTPS requests. Disabled by default; requests are HTTPS-only and blocked from reaching loopback/private/link-local addresses regardless of this flag. Pass with no value to allow any domain, or `--allow-net=DOMAIN` (repeat the flag, or comma-separate, to add more) to restrict requests to just those domains (and any path under them). The `=` is required so a bare domain after the flag isn't swallowed as a query/file positional instead
  -R, --allow-read[=<PATH>...]
          Allow the `read_file`/`read_file_bytes`/`open_file`/`collection`/`walk_files`/`file_exists`/`file_info`/`embed_images` functions to read from the filesystem. Disabled by default. Pass with no value to allow reading anywhere, or `--allow-read=PATH` (files or directories; repeat the flag, or comma-separate, to add more) to restrict reads to just those paths and their descendants. The `=` is required so a bare path after the flag isn't swallowed as a query/file positional instead
  -W, --allow-write[=<PATH>...]
          Allow the `write_file`/`mkdir`/`extract_images` functions to write to the filesystem. Under `--sandbox`, this also gates `-o`/`--output` (and `--append`, `--no-clobber`, `--atomic-output`), which are rejected unless the target is allowed. Without `--sandbox`, `-o` is not restricted. Disabled by default. Pass with no value to allow writing anywhere, or `--allow-write=PATH` (files or directories; repeat the flag, or comma-separate, to add more) to restrict writes to just those paths and their descendants. The `=` is required so a bare path after the flag isn't swallowed as a query/file positional instead
      --allow-run[=<COMMAND>...]
          Allow the `system` function to execute external commands. Disabled by default. Commands run directly (never through a shell), so shell metacharacters in arguments are never interpreted. Pass with no value to allow any command, or `--allow-run=COMMAND` (repeat the flag, or comma-separate, to add more) to restrict execution to just those commands. The `=` is required so a bare command after the flag isn't swallowed as a query/file positional instead
  -E, --allow-env[=<NAME>...]
          Allow `$VAR`/`${$VAR}` interpolation and debugger logpoints to read environment variables. Disabled by default. Pass with no value to allow reading any variable, or `--allow-env=NAME` (repeat the flag, or comma-separate, to add more) to restrict access to just those names. The `=` is required so a bare name after the flag isn't swallowed as a query/file positional instead
  -a, --allow-all
          Grant every sandboxed permission at once (read/write/net/run/env), and also enable HTTP module imports as if --allow-http-import were passed. Disabled by default. Cannot be combined with the individual --allow-* flags above
      --sandbox <SANDBOX>
          Named preset of sandboxed capabilities. Cannot be combined with --allow-* flags, except --allow-write, which adds write access (including `-o`/`--output`) on top of the preset. Under a preset, `-o`/`--output` is denied unless write access is granted [possible values: strict, read-only, networked, unsafe]
  -f, --from-file
          load filter from the file
      --arg <NAME> <VALUE>
          Sets a named string argument. NAME is accessible directly in queries, and also via ARGS."named" when --arg or --args is given
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
          Merge all input files into one input and run the query once (like yq's `eval-all`), instead of once per file
  -F, --output-format <OUTPUT_FORMAT>
          Set output format. When omitted, inferred from the `-o`/`--output` file extension if given (e.g. `.json` -> json, `.csv` -> csv), else defaults to markdown [possible values: markdown, html, text, json, table, grep, gron, raw, csv, jsonl, toml, toon, xml, yaml, shell, none]
      --shell-prefix <PREFIX>
          Prefix for variable names in `-F shell` output (must be non-empty) [default: MQ_]
  -U, --update
          Update matching Markdown nodes and write the result to stdout
      --diff
          With --update, print a unified diff instead of the transformed content; nothing is written. Multiple files are diffed one at a time with their path in the headers; stdin is labeled `<stdin>`. Exits 1 if anything would change
      --unbuffered
          Unbuffered output
      --list-style <LIST_STYLE>
          Set the list style for markdown output (default: keep the original) [possible values: dash, plus, star]
      --link-title-style <LINK_TITLE_STYLE>
          Set the link title surround style for markdown output [default: double] [possible values: double, single, paren]
      --link-url-style <LINK_URL_STYLE>
          Set the link URL surround style for markdown links [default: none] [possible values: none, angle]
  -S, --separator <QUERY>
          Specify a query to insert between files as a separator
  -o, --output <FILE>
          Output to the specified file. With several inputs, results are written one after another
      --atomic-output <ATOMIC_OUTPUT>
          Write `-o`/`--output` atomically via a same-directory temp file + fsync + rename, so a crash or full disk mid-write can't truncate the target [default: auto] [possible values: auto, always, never]
      --no-clobber
          Fail instead of overwriting `-o`/`--output` if the target already exists
      --append
          Add to `-o`/`--output` instead of replacing it, creating it if missing. Use `--atomic-output never` for concurrent-safe appends across processes
  -C, --color[=<WHEN>]
          When to colorize output. `-C` alone means `always` [default: auto] [alias: --color-output] [possible values: auto, always, never]
  -B, --before-context <NUM>
          Show NUM nodes before each match. Only effective with -F grep
      --after-context <NUM>
          Show NUM nodes after each match. Only effective with -F grep
      --context <NUM>
          Show NUM nodes before and after each match. Only effective with -F grep
  -e, --exit-status
          Exit with 4 if every result is empty (None, "", [] or an unmatched select), otherwise 0
      --count
          Output only the count of matching (non-None) results. With multiple files, prints "filename: N" per file and "total: N" at the end
      --skip <N>
          Skip the first N matching results before outputting
      --limit <N>
          Limit output to at most N results
      --no-position
          Omit Markdown node position information from structured output (json, table, gron, csv, toml, toon, xml, yaml). Reduces output size when source line/column spans aren't needed
  -c, --compact
          Print JSON on a single line, without pretty-printing. Only valid with -F json
      --indent <N>
          Number of spaces per indent level in pretty-printed output (0-7), jq's `--indent`. Only valid with -F json or -F xml; default is 2
      --tab
          Indent pretty-printed output with tabs instead of spaces, jq's `--tab`. Only valid with -F json or -F xml
  -T, --format <FORMAT>
          Set both input and output format at once (shorthand for `-I FORMAT -F FORMAT`). An explicit `-I`/`-F` overrides this for that side. Only accepts formats valid on both sides; e.g. `-I mdx` or `-F table` still require the dedicated flag [possible values: markdown, html, text, json, jsonl, gron, raw, csv, toml, toon, xml, yaml]
  -P, --parallel-threshold <PARALLEL_THRESHOLD>
          Number of files to process before switching to parallel processing [default: 10]
      --args [<ARGS>...]
          Positional string arguments, available as ARGS."positional" in queries
      --timeout <SECONDS>
          Maximum time in seconds allowed for query evaluation before aborting (e.g. 0.5, 5). Must be a finite positive number. No timeout by default
      --stack-trace
          Include VM frames in uncaught runtime errors
      --error-format <ERROR_FORMAT>
          Format for the final uncaught error. Combines with `--stack-trace` [default: human] [possible values: human, json]
  -h, --help
          Print help (see more with '--help')
  -V, --version
          Print version

# Examples

mq 'query' file.md

mq compile -f query.mq
mq query.mqc file.md

Compiled .mqc programs are experimental. Recompile them after upgrading mq.

Run `mq help examples` for more usage examples, or `mq help <name>` for
function/selector/module docs.
```
