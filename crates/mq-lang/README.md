<h1 align="center">mq-lang</h1>

Core language implementation for the mq query language - a Markdown processing language with jq-like syntax.

## Installation

Add `mq-lang` to your `Cargo.toml`:

```toml
[dependencies]
mq-lang = "0.5"
```

For specific features:

```toml
[dependencies]
mq-lang = { version = "0.5", features = ["cst", "debugger", "file-io"] }
```

## Usage

### Basic Query Evaluation

```rust
use mq_lang::{DefaultEngine, RuntimeValue};
use mq_markdown::Markdown;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an engine
    let mut engine = DefaultEngine::default();

    // Parse markdown
    let markdown: Markdown = "# Hello\n\nWorld!".parse()?;

    // Execute a query
    let input = vec![RuntimeValue::Markdown(markdown)].into_iter();
    let result = engine.eval(".h | to_text()", input)?;

    println!("{:?}", result); // Output: ["Hello"]

    Ok(())
}
```

### Using Helper Functions

```rust
use mq_lang::{DefaultEngine, parse_markdown_input};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = DefaultEngine::default();

    let markdown = "# Title\n\n- Item 1\n- Item 2";
    let input = parse_markdown_input(markdown)?;

    // Extract list items
    let result = engine.eval(".[] | to_text()", input)?;
    println!("{:?}", result); // Output: ["Item 1", "Item 2"]

    Ok(())
}
```

### Processing Different Input Formats

```rust
use mq_lang::{DefaultEngine, parse_html_input, parse_mdx_input};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = DefaultEngine::default();

    // Process HTML
    let html = "<h1>Hello</h1><p>World</p>";
    let input = parse_html_input(html)?;
    let result = engine.eval(".h", input)?;

    // Process MDX
    let mdx = "# Title\n\n<CustomComponent />";
    let input = parse_mdx_input(mdx)?;
    let result = engine.eval(".h", input)?;

    Ok(())
}
```

## Development

### Building from Source

```bash
git clone https://github.com/harehare/mq
cd mq/crates/mq-lang
cargo build --release
```

### Running Tests

```bash
cargo test -p mq-lang
```

### Running Benchmarks

```bash
cargo bench -p mq-lang
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
