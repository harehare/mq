<h1 align="center">mq-hir</h1>

High-level Intermediate Representation (HIR) for [mq](https://github.com/harehare/mq): the symbols, scopes and name resolution of an mq program. It is the layer that tools share, so that each does not re-implement name lookup.

| Tool       | Uses the HIR for                                       |
| ---------- | ------------------------------------------------------ |
| `mq-lsp`   | symbol lookup and diagnostics                          |
| `mq-lint`  | unused variables and functions, shadowing, style rules |
| `mq-check` | type inference over the symbol tree                    |

## What it holds

- **Symbols** (`Symbol`): every function, variable, parameter, call, reference, literal, selector and control-flow construct, with a `SymbolKind`, its text, its source range, its doc comments and its parent
- **Scopes** (`Scope`): function, block and module scopes, so a lookup finds what is visible at a position
- **Sources** (`SourceId`): one per file or `include`/`import`ed module, plus the builtins from `builtin.mq` and the standard modules
- **References**: for each call or reference symbol, the symbol that declares it

## Install

```toml
[dependencies]
mq-hir = "0.9"
```

## Usage

### Build a HIR and list the symbols

```rust
use mq_hir::{Hir, SymbolKind};

let mut hir = Hir::default();
let (source_id, _scope_id) = hir.add_code(None, "def double(x): x * 2; | double(21)");

for (_, symbol) in hir.symbols_for_source(source_id) {
    if matches!(symbol.kind, SymbolKind::Function(_)) {
        println!("function {:?}", symbol.value);
    }
}
```

`add_code` takes an optional `Url` that names the source (the LSP passes the file URL). Without one, the source is anonymous. Call `add_code` again for each file of a workspace, and `add_nodes` if you already have the CST.

### Find errors

`errors()` reports unresolved names (with a spelling suggestion when a similar name exists), `include`/`import` of a missing or circular module, and `yield` outside a function. Running the snippet below prints ``Unresolved symbol: totl. A name with a similar spelling exists: `total`.``

```rust
use mq_hir::Hir;

let mut hir = Hir::default();
hir.add_code(None, "let total = 1 | totl");

for error in hir.errors() {
    println!("{error}");
}
```

### Go to definition and find references

```rust
use mq_hir::Hir;
use mq_lang::Position;

let mut hir = Hir::default();
let (source_id, _) = hir.add_code(None, "def double(x): x * 2;\n| double(21)");

// A reference or call resolves to its declaration.
let at_call = Position { line: 2, column: 3 };
let (decl_id, decl) = hir.find_symbol_in_position(source_id, at_call).unwrap();
assert_eq!(decl.value.as_deref(), Some("double"));

// And back: every use of a declaration.
let uses = hir.references(decl_id);
assert!(!uses.is_empty());
```

Positions are one-based, like the rest of mq.

### Scope-aware lookup

`find_scope_in_position` gives the innermost scope at a position, and `find_visible_symbols_in_scope` lists what can be named there. This is how completion offers local variables and parameters but not those of other functions.

### Test files

A runner can define names the program does not declare, such as `mq-test`'s `TEST_FILE`, with `Hir::declare_global` so they are not reported as unresolved.

## Features

| Feature        | Description                                                          |
| -------------- | -------------------------------------------------------------------- |
| `http`         | Declare the `http` builtin without building its runtime dependencies |
| `css-selector` | Declare the CSS selector builtins the same way                       |

## Documentation

[API docs](https://docs.rs/mq-hir)

## License

MIT
