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

Run the Markdown parser targets:

```sh
just test-fuzz markdown
just test-fuzz markdown_scaling
```

`markdown` reads generated Markdown and MDX and renders it as Markdown, HTML, text and JSON, so a panic, an abort or a stack overflow is reported. A document is put together from markup tokens, repeats of one token and free text, which reaches nesting and repeated constructs that random bytes seldom do.

`markdown_scaling` repeats the document to 16 KiB and to four times that, and fails when the second takes more than 12 times as long as the first. Linear work takes four times as long and quadratic work sixteen times. A reported input is slow in itself, so check it with `cargo fuzz run markdown_scaling <artifact>`.

CI runs `interpreter`, `mqc` and `markdown` for 15 minutes each every week. `markdown_scaling` compares run times, so it is left to local runs. Pass cargo-fuzz arguments after `--`, for example `just test-fuzz mqc -- -max_total_time=60`.
