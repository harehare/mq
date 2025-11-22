<h1 align="center">mq-markdown</h1>

Markdown parsing and manipulation library for mq.

## Usage

### Basic HTML Conversion

```rust
use mq_markdown::to_html;

let markdown = "# Hello, world!";
let html = to_html(markdown);
assert_eq!(html, "<h1>Hello, world!</h1>");
```

### Working with Markdown AST

```rust
use mq_markdown::Markdown;

let markdown = "# Heading\n\nParagraph with *emphasis*";
let doc = markdown.parse::<Markdown>().unwrap();

println!("Found {} nodes", doc.nodes.len());
println!("HTML: {}", doc.to_html());
println!("Text: {}", doc.to_text());
```

## Development

### Building from Source

```sh
git clone https://github.com/harehare/mq
cd mq
cargo build --release -p mq-markdown
```

### Running Tests

```sh
cargo test -p mq-markdown
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
