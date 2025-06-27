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
=======
//! ## HTML to Markdown (optional feature)
//!
//! When the `html-to-markdown` feature is enabled, this crate can also convert HTML to Markdown.
//!
//! ```rust,ignore
//! // This example requires the `html-to-markdown` feature.
//! // Add `mq-markdown = { version = "...", features = ["html-to-markdown"] }` to your Cargo.toml.
//! # #[cfg(feature = "html-to-markdown")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use mq_markdown::convert_html_to_markdown;
//!
//! let html = "<p>Hello <strong>world</strong>!</p>";
//! let markdown = convert_html_to_markdown(html)?;
//! assert_eq!(markdown, "Hello **world**!");
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "html-to-markdown"))]
//! # fn main() {}
//! ```
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

// Conditionally compile and expose the html_to_markdown module
#[cfg(feature = "html-to-markdown")]
pub mod html_to_markdown;

#[cfg(feature = "html-to-markdown")]
pub use html_to_markdown::convert_html_to_markdown;
#[cfg(feature = "html-to-markdown")]
pub use html_to_markdown::HtmlToMarkdownError;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_html() {
        let markdown = "# Hello, world!";
        let html = to_html(markdown);
        assert_eq!(html, "<h1>Hello, world!</h1>");
    }

    #[cfg(feature = "html-to-markdown")]
    #[test]
    fn test_html_to_markdown_simple_paragraph() {
        let html = "<p>Hello world</p>";
        match convert_html_to_markdown(html) {
            Ok(markdown) => assert_eq!(markdown.trim(), "Hello world"),
            Err(e) => panic!("HTML to Markdown conversion failed: {:?}", e),
        }
    }

    #[cfg(feature = "html-to-markdown")]
    #[test]
    fn test_html_to_markdown_empty_input() {
        let html = "";
        match convert_html_to_markdown(html) {
            Ok(markdown) => assert_eq!(markdown, ""),
            Err(e) => panic!("HTML to Markdown conversion failed for empty input: {:?}", e),
        }
    }

    #[cfg(feature = "html-to-markdown")]
    #[test]
    fn test_html_to_markdown_unimplemented() {
        // This test expects a ParseError because the parser is not fully implemented
        let html = "<div>Unsupported</div>";
        match convert_html_to_markdown(html) {
            Ok(_) => panic!("Conversion should have failed for unimplemented HTML."),
            Err(HtmlToMarkdownError::ParseError { .. }) => {
                // Expected error
            }
            Err(e) => panic!("Unexpected error type: {:?}", e),
        }
    }
}
