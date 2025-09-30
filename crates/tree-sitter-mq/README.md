# tree-sitter-mq

Tree-sitter grammar for the mq language.

## Overview

This package provides a tree-sitter parser for mq, a jq-like command-line tool for Markdown processing.

## Usage

### Rust

Add this to your `Cargo.toml`:

```toml
[dependencies]
tree-sitter-mq = "0.1"
tree-sitter = "0.24"
```

Example usage:

```rust
use tree_sitter_mq::LANGUAGE;

fn main() {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&LANGUAGE.into()).expect("Error loading mq grammar");

    let source_code = r#"
    def greet(name):
      let message = "Hello, " + name
      | message
    end
    "#;

    let tree = parser.parse(source_code, None).unwrap();
    println!("{}", tree.root_node().to_sexp());
}
```

## Development

### Generate Parser

```bash
npm install
npm run generate
```

### Build

```bash
cargo build
```

### Test

```bash
npm test
cargo test
```

## License

MIT
