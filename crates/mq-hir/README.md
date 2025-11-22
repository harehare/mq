<h1 align="center">mq-hir</h1>

High-level Intermediate Representation (HIR) for mq query language.

## Usage

### Basic Symbol Management

```rust
use std::str::FromStr;
use mq_hir::{Hir, Symbol, SymbolId};
use url::Url;

// Create a new HIR instance
let mut hir = Hir::default();

// Add code to the HIR
let code = r#"
  def main():
    let x = 42;
    | x;
"#;
hir.add_code(Some(Url::from_str("file:///main.mq").unwrap()), code);

// Retrieve symbols from the HIR
let symbols: Vec<(SymbolId, &Symbol)> = hir.symbols().collect();

// Print the symbols
for (symbol_id, symbol) in symbols {
    println!("{:?}: {:?} (kind: {:?})", symbol_id, symbol.value, symbol.kind);
}
```

### Running Tests

```sh
cargo test -p mq-hir
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
