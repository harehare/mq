use pyo3::pyclass;
use std::fmt;

#[pyclass]
#[derive(Debug, Clone)]
pub enum MQValue {
    String {
        value: String,
    },
    Number {
        value: f64,
    },
    Array {
        value: Vec<MQValue>,
    },
    Bool {
        value: bool,
    },
    Markdown {
        value: String,
        markdown_type: MarkdownType,
    },
    NoneValue {},
    Function {},
    NativeFunction {},
}

impl fmt::Display for MQValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MQValue::String { value } => write!(f, "{}", value),
            MQValue::Number { value } => write!(f, "{}", value),
            MQValue::Bool { value } => write!(f, "{}", value),
            MQValue::Array { value } => write!(
                f,
                "{}",
                value
                    .iter()
                    .map(|val| val.text())
                    .collect::<Vec<String>>()
                    .join("\n")
            ),
            MQValue::Markdown { value, .. } => write!(f, "{}", value),
            MQValue::NoneValue {} => write!(f, ""),
            MQValue::Function {} => write!(f, "<function>"),
            MQValue::NativeFunction {} => write!(f, "<native_function>"),
        }
    }
}

impl PartialEq for MQValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MQValue::String { value: a }, MQValue::String { value: b }) => a == b,
            (MQValue::Number { value: a }, MQValue::Number { value: b }) => a == b,
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
            (MQValue::NoneValue {}, MQValue::NoneValue {}) => true,
            (MQValue::Function {}, MQValue::Function {}) => false,
            (MQValue::NativeFunction {}, MQValue::NativeFunction {}) => false,
            (MQValue::Bool { value: a }, MQValue::Bool { value: b }) => a == b,
            _ => false,
        }
    }
}

#[pyclass(eq, eq_int)]
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

impl From<mq_lang::Value> for MQValue {
    fn from(value: mq_lang::Value) -> Self {
        match value {
            mq_lang::Value::String(value) => MQValue::String { value },
            mq_lang::Value::Number(value) => MQValue::Number {
                value: value.value(),
            },
            mq_lang::Value::Array(arr) => MQValue::Array {
                value: arr.into_iter().map(|v| v.into()).collect(),
            },
            mq_lang::Value::Markdown(node) => MQValue::Markdown {
                value: node.to_string(),
                markdown_type: node.into(),
            },
            mq_lang::Value::Bool(value) => MQValue::Bool { value },
            mq_lang::Value::None => MQValue::NoneValue {},
            mq_lang::Value::Function(..) => MQValue::Function {},
            mq_lang::Value::NativeFunction(..) => MQValue::NativeFunction {},
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
            mq_markdown::Node::TableHeader(_) => MarkdownType::TableHeader,
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
    pub fn text(&self) -> String {
        self.to_string()
    }

    pub fn array(&self) -> Vec<Self> {
        match self {
            MQValue::Array { value } => value.clone(),
            a => vec![a.clone()],
        }
    }

    pub fn markdown_type(&self) -> Option<MarkdownType> {
        match self {
            MQValue::Markdown { markdown_type, .. } => Some(*markdown_type),
            _ => None,
        }
    }

    pub fn is_string(&self) -> bool {
        matches!(self, MQValue::String { .. })
    }

    pub fn is_number(&self) -> bool {
        matches!(self, MQValue::Number { .. })
    }

    pub fn is_array(&self) -> bool {
        matches!(self, MQValue::Array { .. })
    }

    pub fn is_markdown(&self) -> bool {
        matches!(self, MQValue::Markdown { .. })
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, MQValue::Bool { .. })
    }

    pub fn is_none(&self) -> bool {
        matches!(self, MQValue::NoneValue {})
    }

    pub fn is_function(&self) -> bool {
        matches!(self, MQValue::Function {})
    }

    pub fn is_native_function(&self) -> bool {
        matches!(self, MQValue::NativeFunction {})
    }

    pub fn __str__(&self) -> String {
        self.text()
    }

    pub fn __repr__(&self) -> String {
        match self {
            MQValue::String { value } => format!("MQValue::STRING(\"{}\")", value),
            MQValue::Number { value } => format!("MQValue::NUMBER({})", value),
            MQValue::Array { value: arr } => format!(
                "MQValue::ARRAY([{}])",
                arr.iter()
                    .map(|v| v.__repr__())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            MQValue::Markdown {
                value,
                markdown_type,
            } => {
                format!("MQValue::Markdown(\"{}\", {:?})", value, markdown_type)
            }
            MQValue::Bool { value } => format!("MQValue::BOOL({})", value),
            MQValue::NoneValue {} => "MQValue::NONE".to_string(),
            MQValue::Function {} => "MQValue::FUNCTION".to_string(),
            MQValue::NativeFunction {} => "MQValue::NATIVE_FUNCTION".to_string(),
        }
    }

    pub fn __bool__(&self) -> bool {
        match self {
            MQValue::String { value } => !value.is_empty(),
            MQValue::Number { value } => *value != 0.0,
            MQValue::Array { value } => !value.is_empty(),
            MQValue::Markdown { value, .. } => !value.is_empty(),
            MQValue::NoneValue {} => false,
            MQValue::Bool { value } => *value,
            MQValue::Function {} | MQValue::NativeFunction {} => true,
        }
    }

    pub fn __len__(&self) -> usize {
        match self {
            MQValue::String { value } => value.len(),
            MQValue::Array { value } => value.len(),
            MQValue::Markdown { value, .. } => value.len(),
            _ => 0,
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
            (MQValue::String { value: a }, MQValue::String { value: b }) => a < b,
            (MQValue::Number { value: a }, MQValue::Number { value: b }) => a < b,
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
            (MQValue::String { value: a }, MQValue::String { value: b }) => a > b,
            (MQValue::Number { value: a }, MQValue::Number { value: b }) => a > b,
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
