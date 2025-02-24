# mq-md

This module provides functionality for parsing and converting Markdown content.

It includes the following submodules:

- `markdown`: Contains the core Markdown parsing logic.
- `node`: Defines various node types used in the Markdown AST (Abstract Syntax Tree).

## Example

```rust
use mq_md::to_html;

let markdown = "# Hello, world!";
let html = to_html(markdown);
assert_eq!(html, "<h1>Hello, world!</h1>\n");
```
