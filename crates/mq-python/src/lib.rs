pub mod result;
pub mod value;

use pyo3::prelude::*;
use result::MQResult;
use value::MQValue;

#[pyclass(eq, eq_int)]
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

#[pyclass(eq, eq_int)]
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

#[pyclass(eq, eq_int)]
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

#[pyclass(eq, eq_int)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UrlSurroundStyle {
    #[pyo3(name = "ANGLE")]
    Angle,
    #[pyo3(name = "NONE")]
    #[default]
    None,
}

#[pyclass(eq)]
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

#[pyfunction]
#[pyo3(signature = (code, content, options=None))]
fn run(code: &str, content: &str, options: Option<Options>) -> PyResult<MQResult> {
    let mut engine = mq_lang::Engine::default();
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
    .map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Error evaluating query: {}", e))
    })?;

    engine
        .eval(code, input.into_iter())
        .map(|values| MQResult {
            values: values.into_iter().map(Into::into).collect::<Vec<_>>(),
        })
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Error evaluating query: {}",
                e
            ))
        })
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
    m.add_function(wrap_pyfunction!(run, m)?)?;
    Ok(())
}
