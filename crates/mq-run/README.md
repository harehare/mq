<h1 align="center">mq-run</h1>

The `mq` command: a jq-like tool for Markdown. This crate holds the whole CLI (argument parsing, input and output formats, sandbox flags, file watching, the `repl`, `compile` and `help` subcommands), so you can also embed it in your own binary. It provides the `mq` binary, and `mq-dbg` with the `debugger` feature.

## Install

```sh
cargo install mq-run
```

Other ways to install (shell script, prebuilt binaries, Homebrew) are in the [install guide](https://mqlang.org/book/start/install.html).

## Usage

A query is a pipeline over Markdown nodes. It runs once for each node of the input, which is a file or standard input.

```sh
mq '.h2 | to_text()' README.md                  # text of every level-2 heading
mq '.code("rust")' README.md                    # Rust code blocks
mq '.code | select(.lang == "sh") | .value' README.md
mq '.link.url' README.md                        # URL of every link
cat README.md | mq '.h1 | to_text()'            # from stdin
mq --count '.h' README.md                       # how many headings
mq --limit 1 '.h | to_text()' README.md         # first match only
```

`mq QUERY FILE...` runs the query on each file. `--eval-all` merges the files into one input, and `-A` / `--aggregate` collects the nodes of one input into an array first, so the query sees the whole document at once:

```sh
mq -A 'len()' README.md                         # number of nodes
```

A directory argument reads every `.md`, `.markdown` and `.mdx` file under it, recursively and in path order. Hidden and `.gitignore`d entries are skipped, and a directory with no Markdown file is an error.

```sh
mq '.h1' docs/                                  # level-1 headings of every Markdown file under docs/
```

Text input is UTF-8. A UTF-8 BOM is removed and UTF-16 with a BOM is decoded. For any other encoding, convert first with `iconv -f <encoding> -t UTF-8`.

### Update a document in place

`-U` / `--update` runs the query on every node and writes the whole document back, with the nodes the query changed. Unmatched nodes pass through as they are. Use `self` for "leave this node alone". `--diff` prints a unified diff instead and exits 1 when something would change, which suits CI.

```sh
# Demote every level-2 heading to level 3
mq -U 'if (is_h2()): to_h(3) else: self' README.md

mq -U --diff 'if (is_h2()): to_h(3) else: self' README.md
# -## Install
# +### Install
```

Redirect to a file or pass `-o FILE` to save the result. `-o` writes atomically (temp file and rename) by default, and `--no-clobber` and `--append` change what happens to an existing file.

### Input and output formats

Markdown is the default for both. `-I` sets the input and `-F` the output, and `-T` sets both at once.

| Input (`-I`)                                   | Notes                                                           |
| ---------------------------------------------- | --------------------------------------------------------------- |
| `markdown`, `mdx`, `html`, `text`              | Documents. `.md` and `.mdx` files are detected                  |
| `json`, `jsonl`, `yaml`, `toml`, `toon`, `xml` | Structured data, parsed into mq values                          |
| `csv`, `tsv`, `psv`                            | A row becomes a dict keyed by header (`--no-header` for arrays) |
| `cbor`, `gron`, `raw`, `bytes`                 | Binary and line-oriented inputs                                 |
| `null`                                         | No input: run the query once, like `jq -n`                      |

| Output (`-F`)                                                 | Notes                                                          |
| ------------------------------------------------------------- | -------------------------------------------------------------- |
| `markdown`, `html`, `text`                                    | Rendered documents (`-C` colors Markdown)                      |
| `json`, `jsonl`, `yaml`, `toml`, `toon`, `xml`, `csv`, `gron` | The node tree or values, with positions unless `--no-position` |
| `table`, `grep`, `shell`, `raw`, `none`                       | `grep` prints `file:line:node`                                 |

```sh
mq -I json '.["a"] | map(fn(x): x + 1;)' data.json         # [2, 3]
mq -I csv '.[] | .name' data.csv
mq -F json -c '.h1' README.md                              # compact JSON
mq -F grep 'select(.h2)' README.md
# README.md:5:## Install
# README.md:11:## Usage
```

`-I null` is how you run a query that needs no input document:

```sh
mq -I null 'def double(x): x * 2; | double(21)'      # 42
mq --arg who world -I null 's"hi ${who}"'            # hi world
```

`--arg`, `--argjson`, `--rawfile`, `--slurpfile` and `--args` pass values into the query, as in jq. List style, link style and other Markdown output options (`--list-style`, `--link-url-style`, `--link-title-style`) apply when the output is Markdown.

### Exit status

`mq` exits non-zero on a syntax or runtime error. With `-e` / `--exit-status` the exit code follows the result, as in jq: 0 for a truthy value, 1 for `false` or `none`, 4 when there is no result.

## Sandbox

A query cannot touch the filesystem, network, processes or environment unless you allow it. The default is deny.

```sh
$ mq -I null 'read_file("README.md")'
Error: ... Failed to read file README.md: filesystem reads are disabled

$ mq -R -I null 'read_file("README.md") | len()'
<number of characters>
```

| Flag                                             | Allows                                                           |
| ------------------------------------------------ | ---------------------------------------------------------------- |
| `-R`, `--allow-read[=PATH]`                      | `read_file`, `open_file`, `walk_files`, `collection` and friends |
| `-W`, `--allow-write[=PATH]`                     | `write_file`, `mkdir`, `extract_images`; under `--sandbox`, also `-o`     |
| `-N`, `--allow-net[=DOMAIN]`                     | `http` (HTTPS only, never loopback or private addresses)         |
| `--allow-run[=COMMAND]`                          | `system`, run directly without a shell                           |
| `-E`, `--allow-env[=NAME]`                       | `$VAR` interpolation                                             |
| `-a`, `--allow-all`                              | Everything above, and HTTP module imports                        |
| `--sandbox strict\|read-only\|networked\|unsafe` | A named preset                                                   |

Pass a value (`--allow-read=./docs`) to restrict a permission to those paths, domains or commands. The `=` is required.

## Modules

`-L DIR` adds a module search directory, `-M FILE` loads a module file, and `-m NAME` imports a module so its functions are called as `name::fn()`. Modules can be imported over HTTP with `--allow-http-import`, which is off by default. Fetched modules are pinned in `mq.lock` and checked on each run (`--frozen` fails instead of recording a new entry, for CI). See the [module guide](https://mqlang.org/book/start/modules.html).

## Subcommands

| Command               | Description                                                             |
| --------------------- | ----------------------------------------------------------------------- |
| `mq repl`             | Interactive session, see [`mq-repl`](https://crates.io/crates/mq-repl)  |
| `mq help`             | Documentation of a builtin, selector or standard module (`mq help map`) |
| `mq compile`          | Compile a query to experimental `.mqc` bytecode                         |
| `mq completion SHELL` | Print a shell completion script                                         |
| `mq commands`         | List built-in and external subcommands                                  |

Any executable named `mq-NAME` in `~/.local/bin` or on `PATH` is available as `mq NAME`. This is how `mq fmt`, `mq lint`, `mq check`, `mq lsp`, `mq test`, `mq dbg` and the rest of the toolchain plug in. `mq commands` shows what is found on your machine.

## Features

| Feature                     | Description                                                      |
| --------------------------- | ---------------------------------------------------------------- |
| `http-import`               | Import modules over HTTP (default)                               |
| `css-selector`              | CSS selectors on HTML input (default)                            |
| `watch`                     | `--watch` re-runs the query when an input file changes (default) |
| `use_mimalloc`              | Use the mimalloc allocator (default)                             |
| `debugger`                  | Builds `mq-dbg`, the step debugger, with DAP support             |
| `tiktoken`                  | Token-counting builtins                                          |
| `debug-trace`, `vm-profile` | VM diagnostics for development                                   |

## Embedding

`Cli` is the same parser and runner the `mq` binary uses.

```rust,no_run
use clap::Parser;
use mq_run::Cli;

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // Dispatch `mq-NAME` external subcommands; skip this if you do not want them.
    if let Some(code) = Cli::run_external_subcommand(&args) {
        return code;
    }
    Cli::parse_from(args).run().expect("CLI execution failed");
    std::process::ExitCode::SUCCESS
}
```

To run queries from Rust without a command line, use [`mq-lang`](https://crates.io/crates/mq-lang) directly.

## Documentation

- [mq book](https://mqlang.org/book/)
- [CLI reference](https://mqlang.org/book/reference/cli.html)
- [API docs](https://docs.rs/mq-run)

## License

MIT
