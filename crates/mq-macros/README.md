# mq-macro

This crate provides a proc-macro for compile-time validation of mq queries.

## Usage

Add to your `Cargo.toml`:

```toml
mq-macro = { path = "../mq-macro" }
```

In your code:

```rust
use mq_macro::mq_eval;

mq_eval!{".h | upcase()", "# test"};
```

## License

MIT
