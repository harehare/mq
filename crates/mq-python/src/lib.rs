//! Python bindings for the mq markdown processing library.
//!
//! This crate provides Python bindings for mq, allowing Python applications to
//! process markdown, MDX, and HTML using the mq query language.
//!
//! # Features
//!
//! - Process markdown, MDX, HTML, and plain text from Python
//! - Full mq query language support
//! - Multiple input and output format options
//! - Configurable rendering options
//! - Type-safe Python API using PyO3
//!
//! # Installation
//!
//! ```bash
//! pip install mq
//! ```
//!
//! # Python Usage
//!
//! Basic usage example:
//!
//! ```python
//! import mq
//!
//! # Process markdown with mq
//! result = mq.run('.h', '# Hello\n## World', mq.Options(input_format=mq.InputFormat.MARKDOWN))
//! for value in result.values:
//!     print(value)
//! ```
//!
//! Filter and transform markdown:
//!
//! ```python
//! import mq
//!
//! markdown = """
//! # Introduction
//! Some text here.
//!
//! ## Section 1
//! More content.
//!
//! ## Section 2
//! Even more content.
//! """
//!
//! # Get only level 2 headings
//! result = mq.run('.h | select(level == 2)', markdown)
//! for heading in result.values:
//!     print(heading)
//! ```
//!
//! # Input Formats
//!
//! Supported input formats:
//! - `InputFormat.MARKDOWN` - Standard markdown
//! - `InputFormat.MDX` - Markdown with JSX
//! - `InputFormat.HTML` - HTML content
//! - `InputFormat.TEXT` - Plain text
//! - `InputFormat.RAW` - Raw string input
//! - `InputFormat.NULL` - Null input
//!
//! # Configuration
//!
//! Customize rendering with options:
//!
//! ```python
//! import mq
//!
//! options = mq.Options()
//! options.input_format = mq.InputFormat.MARKDOWN
//! options.list_style = mq.ListStyle.PLUS
//! options.link_title_style = mq.TitleSurroundStyle.SINGLE
//!
//! result = mq.run('.', markdown, options)
//! ```
pub mod result;
pub mod value;

use pyo3::prelude::*;
use result::MQResult;
use value::MQValue;

#[pyclass(eq, eq_int, from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum InputFormat {
    #[pyo3(name = "MARKDOWN")]
    #[default]
    Markdown,
    #[pyo3(name = "MDX")]
    Mdx,
    #[pyo3(name = "TEXT")]
    Text,
    #[pyo3(name = "HTML")]
    Html,
    #[pyo3(name = "RAW")]
    Raw,
    #[pyo3(name = "NULL")]
    Null,
}

#[pyclass(eq, eq_int, from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ListStyle {
    #[pyo3(name = "DASH")]
    #[default]
    Dash,
    #[pyo3(name = "PLUS")]
    Plus,
    #[pyo3(name = "STAR")]
    Star,
}

#[pyclass(eq, eq_int, from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TitleSurroundStyle {
    #[pyo3(name = "DOUBLE")]
    #[default]
    Double,
    #[pyo3(name = "SINGLE")]
    Single,
    #[pyo3(name = "PAREN")]
    PAREN,
}

#[pyclass(eq, eq_int, from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UrlSurroundStyle {
    #[pyo3(name = "ANGLE")]
    Angle,
    #[pyo3(name = "NONE")]
    #[default]
    None,
}

#[pyclass(eq, from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Options {
    #[pyo3(get, set)]
    input_format: Option<InputFormat>,
    #[pyo3(get, set)]
    list_style: Option<ListStyle>,
    #[pyo3(get, set)]
    link_title_style: Option<TitleSurroundStyle>,
    #[pyo3(get, set)]
    link_url_style: Option<UrlSurroundStyle>,
}

#[pymethods]
impl Options {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }
}

#[pyclass(eq, from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct ConversionOptions {
    #[pyo3(get, set)]
    extract_scripts_as_code_blocks: bool,
    #[pyo3(get, set)]
    generate_front_matter: bool,
    #[pyo3(get, set)]
    use_title_as_h1: bool,
}

#[pymethods]
impl ConversionOptions {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }
}

#[pyfunction]
#[pyo3(signature = (code, content, options=None))]
fn run(code: &str, content: &str, options: Option<Options>) -> PyResult<MQResult> {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    let options = options.unwrap_or_default();
    let input = match options.input_format.unwrap_or(InputFormat::Markdown) {
        InputFormat::Markdown => mq_lang::parse_markdown_input(content),
        InputFormat::Mdx => mq_lang::parse_mdx_input(content),
        InputFormat::Text => mq_lang::parse_text_input(content),
        InputFormat::Html => mq_lang::parse_html_input(content),
        InputFormat::Raw => Ok(mq_lang::raw_input(content)),
        InputFormat::Null => Ok(mq_lang::null_input()),
    }
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Error evaluating query: {}", e)))?;

    engine
        .eval(code, input.into_iter())
        .map(|values| MQResult {
            values: values.into_iter().map(Into::into).collect::<Vec<_>>(),
        })
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Error evaluating query: {}", e)))
}

#[pyfunction]
#[pyo3(signature = (content, options=None))]
fn html_to_markdown(content: &str, options: Option<ConversionOptions>) -> PyResult<String> {
    mq_markdown::convert_html_to_markdown(
        content,
        match options {
            Some(opts) => mq_markdown::ConversionOptions {
                extract_scripts_as_code_blocks: opts.extract_scripts_as_code_blocks,
                generate_front_matter: opts.generate_front_matter,
                use_title_as_h1: opts.use_title_as_h1,
            },
            None => mq_markdown::ConversionOptions::default(),
        },
    )
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Error converting HTML to Markdown: {}", e)))
}

#[pymodule]
fn mq(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<InputFormat>()?;
    m.add_class::<ListStyle>()?;
    m.add_class::<UrlSurroundStyle>()?;
    m.add_class::<TitleSurroundStyle>()?;
    m.add_class::<Options>()?;
    m.add_class::<MQResult>()?;
    m.add_class::<MQValue>()?;
    m.add_class::<ConversionOptions>()?;
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(html_to_markdown, m)?)?;
    Ok(())
}
