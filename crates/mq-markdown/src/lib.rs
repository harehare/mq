//! This crate provides markdown parsing and HTML conversion functionality used in [mq](https://github.com/harehare/mq).
//! It offers a simple API to manipulate markdown content and generate different output formats.
//!
//! ## Example
//!
//! ```rust
//! use mq_markdown::to_html;
//!
//! let markdown = "# Hello, world!";
//! let html = to_html(markdown);
//! assert_eq!(html, "<h1>Hello, world!</h1>");
//! ```
//!
mod markdown;
mod node;
pub use markdown::{Markdown, to_html};
pub use node::{
    Blockquote, Code, CodeInline, Definition, Delete, Emphasis, Footnote, FootnoteRef, Fragment,
    Heading, Html, Image, ImageRef, Link, LinkRef, List, ListStyle, Math, MathInline,
    MdxFlowExpression, MdxJsEsm, MdxJsxFlowElement, MdxTextExpression, Node, Point, Position,
    RenderOptions, Strong, TableCell, TableRow, Text, Title, TitleSurroundStyle, Toml, Url,
    UrlSurroundStyle, Yaml,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_html() {
        let markdown = "# Hello, world!";
        let html = to_html(markdown);
        assert_eq!(html, "<h1>Hello, world!</h1>");
    }
}
