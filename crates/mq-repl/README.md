<h1 align="center">mq-repl</h1>

Interactive REPL for [mq](https://github.com/harehare/mq). Load a Markdown document, then try queries against it and see the result immediately. It is also the quickest way to learn the language.

## Start it

The REPL is part of the `mq` command:

```sh
mq repl                 # no input document
mq repl README.md       # query a document
```

```text
  |\  /|   mq v0.9.3
  | \/ |   Query. Filter. Transform Markdown.
  |    |   Type /help to see available commands.
```

Type a query and press Enter. The query runs against the loaded document, just like `mq '<query>' README.md`:

```text
.h2 | to_text()
Install
Usage
```

## Commands

Commands start with `/`. `/help` prints this list.

| Command    | Description                                                         |
| ---------- | ------------------------------------------------------------------- |
| `/load`    | Load one or more Markdown files (supports globs, e.g. `/load *.md`) |
| `/save`    | Save the current result to a file (`/save out.md`)                  |
| `/copy`    | Copy the result to the clipboard (needs the `clipboard` feature)    |
| `/vars`    | List bound variables                                                |
| `/reset`   | Reset the REPL state (clear variables and input)                    |
| `/env`     | Set an environment variable (`/env KEY value`)                      |
| `/edit`    | Edit the current buffer in your external editor                     |
| `/history` | Show command history                                                |
| `/more`    | Show the next page of truncated output                              |
| `/all`     | Show all remaining truncated output                                 |
| `/clear`   | Clear the terminal screen                                           |
| `/version` | Print the mq version                                                |
| `/quit`    | Exit                                                                |

Long output is truncated into pages, so a large document does not flood the terminal. `/more` shows the next page and `/all` the rest.

## Editing

- Emacs key bindings, with history that persists between sessions
- Tab completion for builtin functions, selectors, `/commands`, and the keys of the document's frontmatter
- `Ctrl+Left` / `Ctrl+Right` move by word
- `Alt+O` opens the buffer in your editor (same as `/edit`), `Alt+C` clears the input

History is stored in `history.txt` in the mq config directory: `$MQ_CONFIG_DIR` if set, otherwise the platform config directory joined with `mq` (`~/.config/mq` on Linux, `~/Library/Application Support/mq` on macOS).

## Embedding

`Repl` is what `mq repl` runs. Use it to give your own tool a query prompt. `Repl::new` creates an engine with the builtin module loaded, and `with_engine` takes one you configured (with a sandboxed `Io`, extra functions or a timeout). `with_name` sets the name shown in the banner.

```rust,no_run
use mq_repl::Repl;

let input = mq_lang::parse_markdown_input("# Hello\n\nworld").unwrap();
Repl::new(input).with_name("my-tool").run().unwrap();
```

```rust,no_run
use mq_repl::Repl;

let mut engine = mq_lang::DefaultEngine::default();
engine.load_builtin_module();
engine.register_fn("double", |n: i64| Ok(n * 2));

Repl::with_engine(engine, Vec::new()).run().unwrap();
```

## Features

| Feature     | Description                               |
| ----------- | ----------------------------------------- |
| `clipboard` | `/copy` through `arboard` (on by default) |

## Documentation

- [mq book](https://mqlang.org/book/)
- [API docs](https://docs.rs/mq-repl)

## License

MIT
