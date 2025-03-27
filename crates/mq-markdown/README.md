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

