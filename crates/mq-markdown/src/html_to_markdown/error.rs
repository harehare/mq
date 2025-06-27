#[cfg(feature = "html-to-markdown")]
use miette::{Diagnostic, SourceSpan};
#[cfg(feature = "html-to-markdown")]
use thiserror::Error;

#[cfg(feature = "html-to-markdown")]
#[derive(Debug, Error, Diagnostic)]
pub enum HtmlToMarkdownError {
    #[error("HTML parsing error: {message}")]
    #[diagnostic(
        code(mq_markdown::html::parsing),
        help("The input HTML could not be parsed correctly at the indicated span.")
    )]
    ParseError {
        message: String,
        #[source_code]
        src: String, // The original HTML input string
        #[label("here")]
        span: SourceSpan, // The location of the error
    },

    #[error("Unsupported HTML tag: <{tag_name}>")]
    #[diagnostic(
        code(mq_markdown::html::unsupported_tag),
        help("The HTML tag '<{tag_name}>' is not currently supported for conversion or is used incorrectly.")
    )]
    UnsupportedTag {
        tag_name: String,
        #[source_code]
        src: String,
        #[label("this tag")]
        span: SourceSpan, // Location of the unsupported tag
    },

    #[error("Invalid HTML structure: {message}")]
    #[diagnostic(
        code(mq_markdown::html::invalid_structure),
        help("The HTML structure is invalid or unexpected.")
    )]
    InvalidStructure {
        message: String,
        #[source_code]
        src: String,
        #[label("here")]
        span: SourceSpan,
    },
    // Add other specific error types as needed during development
}
