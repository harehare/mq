<h1 align="center">mq-help</h1>

Documentation catalog for the mq language: builds the single, unified catalog of every native builtin, selector, `builtin.mq` function, and standard-module function — the shared source for the `mq help` CLI command and `mq-web-api`'s documentation endpoints.

## Usage

```rust
use mq_help::{all_entries, lookup, lookup_module, suggest, render_human, render_module_human};

// Every documented function/selector.
let entries = all_entries();

// Look up one name (with or without a leading `.` for selectors), or `module::name` to
// disambiguate a function whose name collides with its own module (e.g. `section::section`).
let matches = lookup("map");

// A standard module's header doc plus its function list.
let section = lookup_module("section").unwrap();
println!("{}", render_module_human(&section));

// "Did you mean" suggestion for a typo (also matches module names).
let suggestion = suggest("mpa"); // Some("map")

for entry in &matches {
    println!("{}", render_human(entry));
    // Or `render_markdown`/`render_module_markdown` — Markdown output that mq can query
    // right back, e.g. `mq help section --markdown | mq 'select(.code.lang == "mq")'`.
}
```

### Extracting docs from arbitrary mq source

The lower-level [`reference`] module parses the CST doc-comment convention (`#` comments above
`def`/`macro`, with `Example:`/`` ``` ``/`#=>`/`Returns:` sections) out of any mq source, independent
of the unified catalog:

```rust
use mq_help::extract_functions_from_cst;

let src = "# Adds one.\n# Example:\n# ```\n# add1(1)\n# #=> 2\n# ```\ndef add1(x): x + 1;";
let docs = extract_functions_from_cst(src, false);
assert_eq!(docs[0].name, "add1");
```

## Development

### Running Tests

```bash
just test-all
```

### Building

```bash
cargo build -p mq-help
```

## License

MIT
