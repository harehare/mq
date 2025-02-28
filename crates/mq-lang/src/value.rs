use std::fmt::{self, Debug, Display, Formatter};

use crate::{AstIdent, AstParams, Program, eval::runtime_value::RuntimeValue, number::Number};

use itertools::Itertools;
use mq_md::Node;

#[derive(Clone, PartialEq)]
pub enum Value {
    Number(Number),
    Bool(bool),
    String(String),
    Array(Vec<Value>),
    Markdown(Node),
    Function(AstParams, Program),
    NativeFunction(AstIdent),
    None,
}

impl From<Node> for Value {
    fn from(node: Node) -> Self {
        Value::Markdown(node)
    }
}

impl From<RuntimeValue> for Value {
    fn from(value: RuntimeValue) -> Self {
        match value {
            RuntimeValue::Number(n) => Value::Number(n),
            RuntimeValue::Bool(b) => Value::Bool(b),
            RuntimeValue::String(s) => Value::String(s),
            RuntimeValue::Array(a) => {
                Value::Array(a.iter().map(|v| v.clone().into()).collect_vec())
            }
            RuntimeValue::Markdown(m) => Value::Markdown(m),
            RuntimeValue::Function(params, program, _) => Value::Function(params, program),
            RuntimeValue::NativeFunction(ident) => Value::NativeFunction(ident),
            RuntimeValue::None => Value::None,
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        let value = match self {
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::String(s) => s.to_string(),
            Value::Array(a) => format!(
                "[{}]",
                a.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join("\n")
            ),
            Value::Markdown(m) => m.to_string(),
            Value::None => "".to_string(),
            Value::Function(_, _) => "function".to_string(),
            Value::NativeFunction(_) => "native_function".to_string(),
        };

        write!(f, "{}", value)
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        let v = match self {
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::String(s) => format!("\"{}\"", s),
            Value::Array(a) => a
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<String>>()
                .join("\n"),
            Value::Markdown(m) => m.to_string(),
            Value::None => "None".to_string(),
            Value::Function(params, _) => format!("function{}", params.len()),
            Value::NativeFunction(ident) => format!("native_function: {}", ident),
        };
        write!(f, "{}", v)
    }
}

impl Value {
    pub const NONE: Value = Self::None;
    pub const TRUE: Value = Self::Bool(true);
    pub const FALSE: Value = Self::Bool(false);

    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    pub fn is_function(&self) -> bool {
        matches!(self, Value::Function(_, _))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    pub fn len(&self) -> usize {
        match self {
            Value::Number(n) => n.value() as usize,
            Value::Bool(_) => 1,
            Value::String(s) => s.len(),
            Value::Array(a) => a.len(),
            Value::Markdown(m) => m.value().len(),
            _ => panic!("not supported"),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Values(Vec<Value>);

impl From<Vec<Value>> for Values {
    fn from(values: Vec<Value>) -> Self {
        Self(values)
    }
}

impl IntoIterator for Values {
    type Item = Value;
    type IntoIter = std::vec::IntoIter<Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Values {
    pub fn compact(&self) -> Vec<Value> {
        self.0
            .iter()
            .filter(|v| !v.is_none())
            .cloned()
            .collect_vec()
    }

    pub fn values(&self) -> &Vec<Value> {
        &self.0
    }
}
