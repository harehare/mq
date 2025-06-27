//! Converts HTML content to Markdown.
//!
//! This module provides the `convert_html_to_markdown` function, which takes an HTML string
//! as input and attempts to convert it into a Markdown string. The conversion process
//! involves parsing the HTML into an internal representation and then rendering that
//! representation as Markdown.
//!
//! This functionality is available when the `html-to-markdown` feature of the
//! `mq-markdown` crate is enabled.
//!
//! ## Current Status
//!
//! The HTML parser and Markdown converter are currently under development.
//! Support for various HTML tags and attributes will be added incrementally.
//! At present, only very basic HTML structures might be handled correctly.
//!
//! ## Error Handling
//!
//! The conversion can fail due to parsing errors (e.g., malformed HTML) or if
//! unsupported HTML constructs are encountered. Errors are reported using the
//! `HtmlToMarkdownError` type, which provides details about the failure.
//!
//! ## Example
//!
//! ```rust
//! # #[cfg(feature = "html-to-markdown")] // For doctest
//! # fn main() -> Result<(), mq_markdown::HtmlToMarkdownError> {
//! use mq_markdown::convert_html_to_markdown;
//!
//! let html = "<p>Hello, <strong>world</strong>!</p>";
//! // The actual output will depend on the implemented parser and converter logic.
//! // This is an illustrative example.
//! let expected_markdown = "Hello, **world**!"; // Simplified expected output
//!
//! // Placeholder: current parser is very basic, so this will likely error or give unexpected output.
//! // let markdown = convert_html_to_markdown(html)?;
//! // assert_eq!(markdown, expected_markdown);
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "html-to-markdown"))]
//! # fn main() {}
//! ```

#[cfg(feature = "html-to-markdown")]
pub mod converter;
#[cfg(feature = "html-to-markdown")]
pub mod error;
#[cfg(feature = "html-to-markdown")]
pub mod node;
#[cfg(feature = "html-to-markdown")]
pub mod parser;

#[cfg(feature = "html-to-markdown")]
pub use error::HtmlToMarkdownError;

/// Converts an HTML string into a Markdown string.
///
/// This function parses the input HTML and then converts the parsed structure
/// into Markdown format.
///
/// # Arguments
///
/// * `html_input`: A string slice representing the HTML content to convert.
///
/// # Returns
///
/// * `Ok(String)`: A `String` containing the converted Markdown if successful.
/// * `Err(HtmlToMarkdownError)`: An error if parsing or conversion fails.
///
/// # Features
///
/// This function is only available if the `html-to-markdown` feature is enabled.
#[cfg(feature = "html-to-markdown")]
pub fn convert_html_to_markdown(html_input: &str) -> Result<String, HtmlToMarkdownError> {
    // Actual parsing and conversion will be implemented progressively.
    // The current implementation is a placeholder.
    let nodes = parser::parse(html_input)?;
    converter::convert_nodes_to_markdown(&nodes, html_input)
}
