# Embed mq in Rust

Add `mq-lang` to your `Cargo.toml`:

```toml
[dependencies]
mq-lang = "0.9"
```

Create an engine once, compile a query, and run it with each input:

```rust
use mq_lang::{DefaultEngine, RuntimeValue, parse_text_input};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = DefaultEngine::default();
    engine.load_builtin_module();
    engine.register_fn("double", |n: i64| Ok(n * 2));

    let query = engine.compile("double(21)")?;
    let input = parse_text_input("ignored")?;
    let output = engine.eval_compiled(&query, input.into_iter())?;
    assert_eq!(output.values(), &[RuntimeValue::Number(42.into())]);
    Ok(())
}
```

`compile` parses the query once. The VM compiles and caches its bytecode on the first evaluation, then reuses it when possible. Each input can be parsed with `parse_markdown_input`, `parse_text_input`, or another input helper.

For filesystem modules, pass one `Io` to both query evaluation and module loading. The sandbox denies access by default; grant only the paths your application needs:

```rust
use mq_lang::{Engine, NativeIo, SandboxedIo, Shared};
use std::path::PathBuf;

let io = Shared::new(
    SandboxedIo::new(NativeIo::default())
        .allow_read(vec![PathBuf::from("./queries")]),
);
let mut engine = Engine::with_default_io(io);
engine.set_search_paths(vec![PathBuf::from("./queries")]);
engine.load_builtin_module();
```

Use `Engine::with_io(resolver, io)` if your application supplies its own module resolver. `Engine::set_timeout` and `Engine::set_max_call_stack_depth` can limit query execution.
