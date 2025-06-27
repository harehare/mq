# mq-markdown

## mq-markdown: Markdown parsing and manipulation for mq

This crate provides comprehensive markdown parsing, manipulation, and conversion
functionality used in [mq](https://github.com/harehare/mq). It offers a robust
API to work with markdown content and generate different output formats.

### Features

- **Parse Markdown**: Convert markdown strings to structured AST
- **HTML Conversion**: Convert between markdown and HTML formats
- **MDX Support**: Parse and manipulate MDX (Markdown + JSX) content
- **JSON Export**: Serialize markdown AST to JSON (with `json` feature)
- **Configurable Rendering**: Customize output formatting and styles

### Quick Start

#### Basic HTML Conversion

```rust
use mq_markdown::to_html;

let markdown = "# Hello, world!";
let html = to_html(markdown);
assert_eq!(html, "<h1>Hello, world!</h1>");
```

#### Working with Markdown AST

```rust
use mq_markdown::Markdown;

let markdown = "# Heading\n\nParagraph with *emphasis*";
let doc = markdown.parse::<Markdown>().unwrap();

println!("Found {} nodes", doc.nodes.len());
println!("HTML: {}", doc.to_html());
println!("Text: {}", doc.to_text());
```

#### Custom Rendering Options

```rust
use mq_markdown::{Markdown, RenderOptions, ListStyle};

let mut doc = "- Item 1\n- Item 2".parse::<Markdown>().unwrap();
doc.set_options(RenderOptions {
    list_style: ListStyle::Plus,
    ..Default::default()
});

// Now renders with "+" instead of "-"
println!("{}", doc);
```

### Performance Considerations

- Use `&str` methods when possible to avoid unnecessary allocations
- The AST uses structural equality checking for efficient comparisons
- Consider using `CompactString` for memory-efficient string storage
- Position information can be omitted to reduce memory usage

## HTML to Markdown Conversion (Optional Feature)

This crate also provides functionality to convert HTML content to Markdown. This feature is optional and can be enabled by specifying the `html-to-markdown` feature in your `Cargo.toml`.

### Enabling the Feature

Add `mq-markdown` to your `Cargo.toml` with the `html-to-markdown` feature:

```toml
[dependencies]
mq-markdown = { version = "0.2.6", features = ["html-to-markdown"] }
```
*(Note: Replace `0.2.6` with the desired version of mq-markdown.)*

### Example

```rust
# #[cfg(feature = "html-to-markdown")] // Only for testing purposes
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use mq_markdown::convert_html_to_markdown;

let html_input = "<p>This is a <strong>paragraph</strong> with some <em>emphasis</em>.</p>";
match convert_html_to_markdown(html_input) {
    Ok(markdown) => {
        // Expected (once implemented): "This is a **paragraph** with some *emphasis*.\n\n"
        println!("{}", markdown);
    }
    Err(e) => {
        eprintln!("Error converting HTML to Markdown: {:?}", e);
    }
}
# Ok(())
# }
# #[cfg(not(feature = "html-to-markdown"))]
# fn main() {}
```

Currently, the HTML parser is under development, and support for various HTML tags is being progressively added. Please refer to the crate's documentation for the most up-to-date list of supported tags and known limitations.
