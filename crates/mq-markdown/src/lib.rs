//! # mq-markdown: Markdown parsing and manipulation for mq
//!
//! This crate provides comprehensive markdown parsing, manipulation, and conversion
//! functionality used in [mq](https://github.com/harehare/mq). It offers a robust
//! API to work with markdown content and generate different output formats.
//!
//! ## Features
//!
//! - **Parse Markdown**: Convert markdown strings to structured AST
//! - **HTML Conversion**: Convert between markdown and HTML formats
//! - **MDX Support**: Parse and manipulate MDX (Markdown + JSX) content
//! - **JSON Export**: Serialize markdown AST to JSON (with `json` feature)
//! - **Configurable Rendering**: Customize output formatting and styles
//!
//! ## Quick Start
//!
//! ### Basic HTML Conversion
//!
//! ```rust
//! use mq_markdown::to_html;
//!
//! let markdown = "# Hello, world!";
//! let html = to_html(markdown);
//! assert_eq!(html, "<h1>Hello, world!</h1>");
//! ```
//!
//! ### Working with Markdown AST
//!
//! ```rust
//! use mq_markdown::Markdown;
//!
//! let markdown = "# Heading\n\nParagraph with *emphasis*";
//! let doc = markdown.parse::<Markdown>().unwrap();
//!
//! println!("Found {} nodes", doc.nodes.len());
//! println!("HTML: {}", doc.to_html());
//! println!("Text: {}", doc.to_text());
//! ```
//!
//! ### Custom Rendering Options
//!
//! ```rust
//! use mq_markdown::{Markdown, RenderOptions, ListStyle};
//!
//! let mut doc = "- Item 1\n- Item 2".parse::<Markdown>().unwrap();
//! doc.set_options(RenderOptions {
//!     list_style: ListStyle::Plus,
//!     ..Default::default()
//! });
//!
//! // Now renders with "+" instead of "-"
//! println!("{}", doc);
//! ```
//!
//! ## Performance Considerations
//!
//! - Use `&str` methods when possible to avoid unnecessary allocations
//! - The AST uses structural equality checking for efficient comparisons
//! - Consider using `CompactString` for memory-efficient string storage
//! - Position information can be omitted to reduce memory usage
//!
mod markdown;
mod node;
pub use markdown::{Markdown, to_html};
pub use node::{
    Blockquote, Break, Code, CodeInline, Definition, Delete, Emphasis, Footnote, FootnoteRef,
    Fragment, Heading, HorizontalRule, Html, Image, ImageRef, Link, LinkRef, List, ListStyle, Math,
    MathInline, MdxFlowExpression, MdxJsEsm, MdxJsxFlowElement, MdxJsxTextElement,
    MdxTextExpression, Node, Point, Position, RenderOptions, Strong, TableCell, TableRow, Text,
    Title, TitleSurroundStyle, Toml, Url, UrlSurroundStyle, Yaml,
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
