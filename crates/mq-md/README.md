# mq-md

This module provides functionality for parsing and converting Markdown content.

### Example

```rust
use mq_md::to_html;

let markdown = "# Hello, world!";
let html = to_html(markdown);
assert_eq!(html, "<h1>Hello, world!</h1>\n");
```

