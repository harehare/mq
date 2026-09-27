# Fuzzing mq

Fuzzing needs a nightly Rust toolchain and `cargo-fuzz`.

Run the bytecode VM fuzz target, which evaluates generated queries:

```sh
just test-fuzz
```

Run the `.mqc` fuzz target, which corrupts compiled `.mqc` files and loads them:

```sh
just test-fuzz mqc
```

The `mqc` target compiles a known program, overwrites bytes of the file, and recomputes its checksum so the input reaches the decoder and the bytecode verifier. A file that loads must then run without panicking, since the VM trusts verified bytecode.
