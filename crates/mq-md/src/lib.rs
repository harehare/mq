//! This crate provides markdown parsing and HTML conversion functionality used in [mq](https://github.com/harehare/mq).
//! It offers a simple API to manipulate markdown content and generate different output formats.
//!
//! ## Example
//!
//! ```rust
//! use mq_md::to_html;
//!
//! let markdown = "# Hello, world!";
//! let html = to_html(markdown);
//! assert_eq!(html, "<h1>Hello, world!</h1>\n");
//! ```
//!
mod markdown;
mod node;
pub use markdown::{Markdown, RenderOptions};
pub use node::{
    Code, CodeInline, Footnote, FootnoteRef, Heading, Html, Image, ImageRef, Link, LinkRef, List,
    ListIndent, ListStyle, Math, MathInline, Node, TableCell, Text, Toml, Value, Yaml,
};

pub fn to_html(markdown: &str) -> String {
    let options = comrak::ComrakOptions::default();
    comrak::markdown_to_html(markdown, &options)
}
