<h1 align="center">mq-lang</h1>

The mq query language: lexer, parser, compiler, bytecode VM and the standard library. This is the crate to embed when you want to run mq queries from Rust. The `mq` command is a thin CLI over it.

- Parses a query once (`compile`) and runs it against any number of inputs
- Input can be Markdown, MDX, HTML, plain text or any `RuntimeValue`
- Register your own Rust functions and inject values from the host
- Sandboxed I/O, execution timeout and call-depth limit for running untrusted queries
- Optional lossless CST (formatter, linter, language server), AST JSON and compiled `.mqc` programs

## The language in 30 seconds

mq works like jq, but the values are Markdown nodes. A query is a pipeline, and it runs once for each input node.

```sh
# headings, code blocks, links
mq '.h2 | to_text()' README.md
mq '.code("rust")' README.md
mq '.link.url' README.md

# filter and transform
mq 'select(.h) | to_text() | upcase()' README.md
mq '.code | select(.lang == "sh") | .value' README.md
```

It is a small expression language with `let`/`var`, `def`/`fn`, `if`/`elif`/`else`, `match`, `foreach`/`while`/`loop`, `try`/`catch`, string interpolation and modules. Pass `-I null` to run a query with no input document:

```sh
mq -I null 'def double(x): x * 2; | double(21)'          # 42
mq -I null '[1, 2, 3] | map(fn(x): x * 2;) | join(",")'   # 2,4,6
mq -I null 's"1+1=${1 + 1}"'                               # 1+1=2
mq -I null 'match (5): | 5: "five" | _: "other" end'      # five
mq -I null 'try: error("boom") catch: "caught"'           # caught
```

See the [language reference](https://mqlang.org/book/reference/) for the full syntax and the [builtin list](https://mqlang.org/book/builtins.html).

## Install

```toml
[dependencies]
mq-lang = "0.9"
```

## Embedding

### Evaluate a query

```rust
use mq_lang::DefaultEngine;

let mut engine = DefaultEngine::default();
engine.load_builtin_module(); // `builtin.mq`: map, filter, to_text, ...

let input = mq_lang::parse_markdown_input("# Title\n\nbody").unwrap();
let output = engine.eval(".h1 | to_text()", input.into_iter()).unwrap();
assert_eq!(output.into_markdown_nodes()[0].value(), "Title");
```

`parse_markdown_input`, `parse_mdx_input`, `parse_html_input` and `parse_text_input` turn a string into the `Vec<RuntimeValue>` that `eval` takes. Results come back as `RuntimeValues`. Use `into_markdown_nodes()` to get Markdown (non-Markdown values are rendered as text), or read plain values with `as_str`, `as_f64`, `as_bool`, `as_array`, `as_dict` and `get`. `mq_lang::from_value` deserializes a value into any `serde::Deserialize` type.

### Compile once, run many times

`eval` parses the query on every call. For a query you run against many inputs, compile it first:

```rust
use mq_lang::{DefaultEngine, RuntimeValue, parse_text_input};

let mut engine = DefaultEngine::default();
engine.load_builtin_module();

let query = engine.compile("1 + 2").unwrap();
for _ in 0..3 {
    let input = parse_text_input("ignored").unwrap();
    let output = engine.eval_compiled(&query, input.into_iter()).unwrap();
    assert_eq!(output.values(), &[RuntimeValue::Number(3.into())]);
}
```

### Call Rust from mq

`register_fn` makes a Rust closure callable as `name(...)`. Arguments and results are converted for you, and a wrong argument count or type becomes a normal mq runtime error. A function registered this way never overrides a `def` or a builtin of the same name.

```rust
use mq_lang::{DefaultEngine, RuntimeValue, parse_text_input};

let mut engine = DefaultEngine::default();
engine.load_builtin_module();
engine.register_fn("double", |n: i64| Ok(n * 2));

let input = parse_text_input("x").unwrap();
let output = engine.eval("double(21)", input.into_iter()).unwrap();
assert_eq!(output.values(), &[RuntimeValue::Number(42.into())]);
```

Values can also be injected as variables with `define_string_value` and `define_value`.

### Run untrusted queries

File, network and process access go through the `Io` trait. `SandboxedIo` denies everything until you allow it, and the engine can bound time and recursion:

```rust
use mq_lang::{Engine, NativeIo, SandboxedIo, Shared};
use std::{path::PathBuf, time::Duration};

let io = Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(vec![PathBuf::from("./queries")]));
let mut engine = Engine::with_default_io(io);
engine.set_search_paths(vec![PathBuf::from("./queries")]); // where `import` looks for modules
engine.set_timeout(Duration::from_secs(5));
engine.set_max_call_stack_depth(200);
```

`print`, `stderr` and `input` go through the same `Io`, so a TUI or test can capture them by implementing `write_stdout_line`, `write_stderr_line` and `read_stdin_line`. `halt(code)` does not exit the process: it returns an error that `try` cannot catch, and `Error::exit_code()` gives the requested code.

### Parse without running

```rust
use mq_lang::{Arena, Shared, SharedCell};

let token_arena = Shared::new(SharedCell::new(Arena::new(16)));
let program = mq_lang::parse("1 + 2", token_arena).unwrap();
assert_eq!(program.len(), 1);
```

With the `cst` feature, `parse_recovery` and `CstParser` return a lossless tree that keeps comments and whitespace and recovers from syntax errors.

## Documentation

- [Embedding mq in Rust](https://mqlang.org/book/start/embedding-rust.html)
- [Language reference](https://mqlang.org/book/reference/)
- [API docs](https://docs.rs/mq-lang)

## License

MIT
