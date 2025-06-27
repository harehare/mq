# mq-markdown

This crate provides markdown parsing and HTML conversion functionality used in [mq](https://github.com/harehare/mq).
It offers a simple API to manipulate markdown content and generate different output formats.

### Example

```rust
use mq_markdown::to_html;

let markdown = "# Hello, world!";
let html = to_html(markdown);
assert_eq!(html, "<h1>Hello, world!</h1>");
```

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
