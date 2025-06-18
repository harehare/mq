# mq-macro

This crate provides a proc-macro for compile-time validation of mq queries.

- Detects errors in mq queries at compile time.
- Shares options/types with `mq-wasm` for consistent validation.

## Usage

Add to your `Cargo.toml`:

```toml
mq-macro = { path = "../mq-macro" }
```

In your code:

```rust
use mq_macro::validate_mq;

validate_mq!(".heading");
```

## License
MIT
