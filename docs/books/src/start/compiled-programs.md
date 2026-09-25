# Compiled Programs

`mq compile` saves a query as Tarn VM bytecode in a `.mqc` file. Pass the `.mqc` file in place of the query to run it. Modules, including HTTP imports, are resolved at compile time and stored in the file, so running it needs no module files or network access.

```sh
mq compile query.mq          # writes query.mqc
mq query.mqc README.md
```

Use `-o` to choose another output path. The program runs only when its path ends in `.mqc`, and `-f` does not accept it.

The usual input and output options apply:

```sh
mq query.mqc -F json docs/*.md
cat notes.md | mq query.mqc -U
```

## What is fixed at compile time

These options shape the compiled program, so pass them after `mq compile` (for example `mq compile -I csv query.mq`). `mq compile` rejects other options:

| Option | At run time |
| --- | --- |
| `-L`, `-M`, `-m` | Rejected. The modules are already compiled in. |
| `-A` | Must match the compile-time setting. |
| `-I`, `--csv-delimiter`, `--no-header` | Must produce the same input handling. For example, a program compiled with `-I csv` runs on CSV input only. |
| `--allow-http-import`, `--allowed-domain`, `--frozen`, `--lockfile` | Used only by `mq compile`. |
| `--watch` | Rejected. The `.mqc` file is loaded once and never reloaded. |

Module-level `let` values are computed once, when the program is compiled.

## What is read at run time

- Values from `--args`, `--argjson`, `--rawfile`, `--slurpfile`, and `__FILE__`.
- Permissions such as `--allow-read` and `--allow-net`. A compiled program gets no permission that the running `mq` does not grant. Permissions passed to `mq compile` apply only while module-level `let` values are computed, and are not saved.
- Environment variables read with interpolation, such as `s"${$HOME}"` (with `--allow-env`). `mq compile` rejects a bare `$VAR`, and any environment read in a module-level `let`, because the value would be saved in the file.

## Compatibility

A `.mqc` file runs only on the same mq version that compiled it. Recompile the source after upgrading mq. The file keeps the original query and the sources of your own modules, so runtime errors point at the original code. Standard modules such as `csv` are not stored; their sources come from mq itself.

mq checks the file's structure and checksum and verifies its bytecode before running it. The checksum detects corruption. It does not show who created the file.

## Embedding

Rust applications can use the same format through `mq-lang` with the `mqc` feature:

```rust
let mut engine = mq_lang::DefaultEngine::default();
engine.load_builtin_module();
let bytes = engine.compile_to_mqc("upcase()", &[])?;

let program = engine.load_mqc(&bytes)?;
let input = mq_lang::parse_text_input("hello")?;
let output = engine.eval_compiled(program.program(), input.into_iter())?;
```
