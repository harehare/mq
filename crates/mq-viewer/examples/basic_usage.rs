/// Example demonstrating basic usage of the mq-viewer library
use mq_markdown::Markdown;
use mq_viewer::{render_markdown, render_markdown_to_string};
use std::io::BufWriter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let markdown_content = r#"
# Hello, World!

This is a **bold** text and this is *italic*.

## Code Example

```rust
fn main() {
    println!("Hello from Rust!");
}
```

## List Example

- Item 1
- Item 2
  - Nested item
- Item 3

## Callout Example

> [!NOTE]
> This is a note callout with important information.

---

Visit [Rust's website](https://www.rust-lang.org) for more info.
"#;

    // Parse markdown
    let markdown: Markdown = markdown_content.parse()?;

    println!("=== Example 1: Render to stdout ===\n");
    let mut stdout = BufWriter::new(std::io::stdout());
    render_markdown(&markdown, &mut stdout)?;

    println!("\n\n=== Example 2: Render to string ===\n");
    let rendered_string = render_markdown_to_string(&markdown)?;
    print!("{}", rendered_string);

    Ok(())
}
