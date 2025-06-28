# mq-markdown

High-performance markdown parsing and manipulation library for [mq](https://github.com/harehare/mq).

## Features

- **Fast & Memory Efficient**: Optimized for performance with minimal allocations
- **Comprehensive Parsing**: Full CommonMark + GFM support with MDX extensions
- **Multiple Output Formats**: HTML, JSON, and plain text conversion
- **Configurable Rendering**: Customize list styles, link formatting, and more
- **Type-Safe AST**: Strongly-typed Abstract Syntax Tree for reliable manipulation

## Quick Start

### Basic HTML Conversion

```rust
use mq_markdown::to_html;

let markdown = "# Hello, world!";
let html = to_html(markdown);
assert_eq!(html, "<h1>Hello, world!</h1>");
```

### Working with AST

```rust
use mq_markdown::Markdown;

let markdown = "# Heading\n\nParagraph with *emphasis*";
let doc = markdown.parse::<Markdown>()?;

println!("Found {} nodes", doc.nodes.len());
println!("HTML: {}", doc.to_html());
println!("Text: {}", doc.to_text());
```

### Custom Rendering

```rust
use mq_markdown::{Markdown, RenderOptions, ListStyle};

let mut doc = "- Item 1\n- Item 2".parse::<Markdown>()?;
doc.set_options(RenderOptions {
    list_style: ListStyle::Plus,
    ..Default::default()
});

// Now renders with "+" instead of "-"
println!("{}", doc);
```

## Performance

- Uses structural comparison for efficient node equality
- Pre-allocates buffers to minimize string allocations  
- Leverages `CompactString` for memory-efficient string storage
- Zero-copy operations where possible

## License

MIT

