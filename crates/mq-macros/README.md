<h1 align="center">mq-macros</h1>

Procedural macros that register the native builtins of [mq](https://github.com/harehare/mq). It is an implementation detail of `mq-lang` and is published only because `mq-lang` depends on it. The API is not meant for other crates and may change without notice.

## Macros

### `#[mq_fn(name = "...", params = ...)]`

Turns a Rust function into a builtin. It generates a `static` `LazyLock<BuiltinFunction>` named after the builtin, in upper case (`"sort_desc"` becomes `SORT_DESC`).

| Argument | Meaning                                                                        |
| -------- | ------------------------------------------------------------------------------ |
| `name`   | Name of the function in mq, also used for the static                           |
| `params` | A `ParamNum` variant without the prefix: `None`, `Fixed(1)` or `Range(0, 255)` |

The function has a fixed signature: the identifier of the call (for error messages), the piped value, the evaluated arguments and the environment.

```rust,ignore
#[mq_macros::mq_fn(name = "halt", params = Fixed(1))]
fn halt_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(exit_code)] => Err(Error::Halt(exit_code.value() as i32)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}
```

### `builtin_dispatch! { ... }`

Takes the list of those statics and generates the lookup `mq-lang` uses to find a builtin by name:

- A hash constant per builtin (FNV-1a of the lower-case name) and `get_builtin_functions_by_str`
- `BUILTIN_FUNCTION_NAMES`, the public builtins, used for "did you mean" suggestions and checked against the docs and type signatures in tests
- `INTERNAL_FUNCTION_NAMES`, the builtins whose static starts with `_` or that are marked `#[internal]`

Entries can carry `#[cfg(...)]`, so a builtin behind a Cargo feature (`file-io`, `http`, ...) exists only when that feature is on.

```rust,ignore
mq_macros::builtin_dispatch! {
    ABS,
    ADD,
    #[cfg(feature = "file-io")]
    READ_FILE,
}
```

## Adding a builtin

Writing a new native builtin touches more than this crate. In the mq repository:

1. Write the function with `#[mq_fn]` in `crates/mq-lang/src/runtime/builtin.rs` (or a submodule) and add its static to `builtin_dispatch!`
2. Register its type signature in `crates/mq-check/src/builtin.rs`, or the type checker will not know it
3. Document it in the `mq-help` catalog (`crates/mq-help/tests/registry_docs.rs` fails when a public builtin has no entry)
4. Add a test for the logic you wrote

If it can be written in mq, put it in `builtin.mq` instead.

## License

MIT
