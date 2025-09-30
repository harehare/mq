//! A library for rendering Markdown documents with syntax highlighting.
//!
//! This crate provides functionality to render Markdown content with rich text formatting
//! and syntax highlighting for code blocks using tree-sitter.
//!
//! # Examples
//!
//! ```rust
//! use mq_viewer::render_markdown_to_string;
//! use mq_markdown::Markdown;
//!
//! let markdown: Markdown = "# Hello\n\n```rust\nfn main() {}\n```".parse().unwrap();
//! let rendered = render_markdown_to_string(&markdown).unwrap();
//! println!("{}", rendered);
//! ```

mod highlighter;
mod renderer;

pub use highlighter::SyntaxHighlighter;
pub use renderer::{render_markdown, render_markdown_to_string};
