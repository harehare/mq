# mq-macros

This crate provides a proc-macro for compile-time validation of mq queries.

## Usage

Add to your `Cargo.toml`:

```toml
mq-macros = { path = "../mq-macros" }
```

In your code:

```rust
use mq_macro::mq_eval;

mq_eval!{".h | upcase()", "# test"};
```

## License

MIT
