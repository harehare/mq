use pyo3::pyclass;
use std::{collections::HashMap, fmt};

#[pyclass(from_py_object)]
#[derive(Debug, Clone)]
pub enum MQValue {
    Array { value: Vec<MQValue> },
    Dict { value: HashMap<String, MQValue> },
    Markdown { value: String, markdown_type: MarkdownType },
}

impl fmt::Display for MQValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MQValue::Array { value } => write!(
                f,
                "{}",
                value.iter().map(|val| val.text()).collect::<Vec<String>>().join("\n")
            ),
            MQValue::Dict { value } => write!(
                f,
                "{}",
                value
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.text()))
                    .collect::<Vec<String>>()
                    .join("\n")
            ),
            MQValue::Markdown { value, .. } => write!(f, "{}", value),
        }
    }
}

impl PartialEq for MQValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MQValue::Array { value: a }, MQValue::Array { value: b }) => a == b,
            (
                MQValue::Markdown {
                    value: a,
                    markdown_type: at,
                },
                MQValue::Markdown {
                    value: b,
                    markdown_type: bt,
                },
            ) => a == b && at == bt,
            _ => false,
        }
    }
}

#[pyclass(eq, eq_int, from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkdownType {
    Blockquote,
    Break,
    Definition,
    Delete,
    Heading,
    Emphasis,
    Footnote,
    FootnoteRef,
    Html,
    Yaml,
    Toml,
    Image,
    ImageRef,
    CodeInline,
    MathInline,
    Link,
    LinkRef,
    Math,
    List,
    TableHeader,
    TableRow,
    TableCell,
    Code,
    Strong,
    HorizontalRule,
    MdxFlowExpression,
    MdxJsxFlowElement,
    MdxJsxTextElement,
    MdxTextExpression,
    MdxJsEsm,
    Text,
    Empty,
}

impl From<mq_lang::RuntimeValue> for MQValue {
    fn from(value: mq_lang::RuntimeValue) -> Self {
        match value {
            mq_lang::RuntimeValue::Array(arr) => MQValue::Array {
                value: arr.into_iter().map(|v| v.into()).collect(),
            },
            mq_lang::RuntimeValue::Dict(map) => MQValue::Dict {
                value: map.into_iter().map(|(k, v)| (k.as_str(), v.into())).collect(),
            },
            mq_lang::RuntimeValue::Markdown(node, _) => MQValue::Markdown {
                value: node.to_string(),
                markdown_type: node.into(),
            },
            mq_lang::RuntimeValue::String(s) => MQValue::Markdown {
                value: s,
                markdown_type: MarkdownType::Text,
            },
            mq_lang::RuntimeValue::Symbol(i) => MQValue::Markdown {
                value: i.as_str(),
                markdown_type: MarkdownType::Text,
            },
            mq_lang::RuntimeValue::Number(n) => MQValue::Markdown {
                value: n.to_string(),
                markdown_type: MarkdownType::Text,
            },
            mq_lang::RuntimeValue::Boolean(b) => MQValue::Markdown {
                value: b.to_string(),
                markdown_type: MarkdownType::Text,
            },
            mq_lang::RuntimeValue::Function(..)
            | mq_lang::RuntimeValue::NativeFunction(..)
            | mq_lang::RuntimeValue::Module(..)
            | mq_lang::RuntimeValue::Ast(..) => MQValue::Markdown {
                value: "".to_string(),
                markdown_type: MarkdownType::Empty,
            },
            mq_lang::RuntimeValue::None => MQValue::Markdown {
                value: "".to_string(),
                markdown_type: MarkdownType::Empty,
            },
        }
    }
}

impl From<mq_markdown::Node> for MarkdownType {
    fn from(node: mq_markdown::Node) -> Self {
        match node {
            mq_markdown::Node::Blockquote(_) => MarkdownType::Blockquote,
            mq_markdown::Node::Break(_) => MarkdownType::Break,
            mq_markdown::Node::Definition(_) => MarkdownType::Definition,
            mq_markdown::Node::Delete(_) => MarkdownType::Delete,
            mq_markdown::Node::Heading(_) => MarkdownType::Heading,
            mq_markdown::Node::Emphasis(_) => MarkdownType::Emphasis,
            mq_markdown::Node::Footnote(_) => MarkdownType::Footnote,
            mq_markdown::Node::FootnoteRef(_) => MarkdownType::FootnoteRef,
            mq_markdown::Node::Html(_) => MarkdownType::Html,
            mq_markdown::Node::Yaml(_) => MarkdownType::Yaml,
            mq_markdown::Node::Toml(_) => MarkdownType::Toml,
            mq_markdown::Node::Image(_) => MarkdownType::Image,
            mq_markdown::Node::ImageRef(_) => MarkdownType::ImageRef,
            mq_markdown::Node::CodeInline(_) => MarkdownType::CodeInline,
            mq_markdown::Node::MathInline(_) => MarkdownType::MathInline,
            mq_markdown::Node::Link(_) => MarkdownType::Link,
            mq_markdown::Node::LinkRef(_) => MarkdownType::LinkRef,
            mq_markdown::Node::Math(_) => MarkdownType::Math,
            mq_markdown::Node::List(_) => MarkdownType::List,
            mq_markdown::Node::TableAlign(_) => MarkdownType::TableHeader,
            mq_markdown::Node::TableRow(_) => MarkdownType::TableRow,
            mq_markdown::Node::TableCell(_) => MarkdownType::TableCell,
            mq_markdown::Node::Code(_) => MarkdownType::Code,
            mq_markdown::Node::Strong(_) => MarkdownType::Strong,
            mq_markdown::Node::HorizontalRule(_) => MarkdownType::HorizontalRule,
            mq_markdown::Node::MdxFlowExpression(_) => MarkdownType::MdxFlowExpression,
            mq_markdown::Node::MdxJsxFlowElement(_) => MarkdownType::MdxJsxFlowElement,
            mq_markdown::Node::MdxJsxTextElement(_) => MarkdownType::MdxJsxTextElement,
            mq_markdown::Node::MdxTextExpression(_) => MarkdownType::MdxTextExpression,
            mq_markdown::Node::MdxJsEsm(..) => MarkdownType::MdxJsEsm,
            mq_markdown::Node::Text(_) => MarkdownType::Text,
            _ => MarkdownType::Empty,
        }
    }
}

use pyo3::prelude::*;

#[pymethods]
impl MQValue {
    #[getter]
    pub fn text(&self) -> String {
        self.to_string()
    }

    #[getter]
    pub fn values(&self) -> Vec<Self> {
        match self {
            MQValue::Array { value } => value.clone(),
            a => vec![a.clone()],
        }
    }

    #[getter]
    pub fn markdown_type(&self) -> Option<MarkdownType> {
        match self {
            MQValue::Markdown { markdown_type, .. } => Some(*markdown_type),
            _ => None,
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(self, MQValue::Array { .. })
    }

    pub fn is_markdown(&self) -> bool {
        matches!(self, MQValue::Markdown { .. })
    }

    pub fn __getitem__(&self, idx: usize) -> PyResult<MQValue> {
        let array = self.values();

        if idx < array.len() {
            Ok(array[idx].clone())
        } else {
            Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "Index {} out of range for MQResult with length {}",
                idx,
                array.len()
            )))
        }
    }

    pub fn __str__(&self) -> String {
        self.text()
    }

    pub fn __repr__(&self) -> String {
        match self {
            MQValue::Array { value: arr } => format!(
                "MQValue::ARRAY([{}])",
                arr.iter().map(|v| v.__repr__()).collect::<Vec<_>>().join(", ")
            ),
            MQValue::Dict { value: map } => {
                format!(
                    "MQValue::MAP({})",
                    map.iter()
                        .map(|(k, v)| format!("\"{}\": {}", k, v.__repr__()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            MQValue::Markdown { value, markdown_type } => {
                format!("MQValue::Markdown(\"{}\", {:?})", value, markdown_type)
            }
        }
    }

    pub fn __bool__(&self) -> bool {
        match self {
            MQValue::Array { value } => !value.is_empty(),
            MQValue::Dict { value } => !value.is_empty(),
            MQValue::Markdown { value, .. } => !value.is_empty(),
        }
    }

    pub fn __len__(&self) -> usize {
        match self {
            MQValue::Array { value } => value.len(),
            MQValue::Dict { value } => value.len(),
            MQValue::Markdown { value, .. } => value.len(),
        }
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self == other
    }

    pub fn __ne__(&self, other: &Self) -> bool {
        !self.__eq__(other)
    }

    pub fn __lt__(&self, other: &Self) -> bool {
        match (self, other) {
            (MQValue::Array { value: a }, MQValue::Array { value: b }) => {
                if a.len() != b.len() {
                    a.len() < b.len()
                } else {
                    for (a_item, b_item) in a.iter().zip(b.iter()) {
                        if a_item != b_item {
                            return a_item.__lt__(b_item);
                        }
                    }
                    false
                }
            }
            (MQValue::Markdown { value: a, .. }, MQValue::Markdown { value: b, .. }) => a < b,
            _ => false,
        }
    }

    pub fn __gt__(&self, other: &Self) -> bool {
        match (self, other) {
            (MQValue::Array { value: a }, MQValue::Array { value: b }) => {
                if a.len() != b.len() {
                    a.len() > b.len()
                } else {
                    for (a_item, b_item) in a.iter().zip(b.iter()) {
                        if a_item != b_item {
                            return a_item.__gt__(b_item);
                        }
                    }
                    false
                }
            }
            (MQValue::Markdown { value: a, .. }, MQValue::Markdown { value: b, .. }) => a > b,
            _ => false,
        }
    }
}
