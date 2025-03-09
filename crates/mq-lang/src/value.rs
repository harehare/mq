use std::fmt::{self, Debug, Display, Formatter};

use crate::{AstIdent, AstParams, Program, eval::runtime_value::RuntimeValue, number::Number};

use mq_markdown::Node;

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

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<Number> for Value {
    fn from(n: Number) -> Self {
        Value::Number(n)
    }
}

impl From<RuntimeValue> for Value {
    fn from(value: RuntimeValue) -> Self {
        match value {
            RuntimeValue::Number(n) => Value::Number(n),
            RuntimeValue::Bool(b) => Value::Bool(b),
            RuntimeValue::String(s) => Value::String(s),
            RuntimeValue::Array(a) => {
                Value::Array(a.iter().map(|v| v.clone().into()).collect::<Vec<_>>())
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
                    .join(", ")
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
            .collect::<Vec<_>>()
    }

    pub fn values(&self) -> &Vec<Value> {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
}
#[cfg(test)]
mod tests {
    use mq_markdown::Text;

    use super::*;

    #[test]
    fn test_value_from_node() {
        let node = Node::Text(Text {
            value: "test".to_string(),
            position: None,
        });
        let value = Value::from(node.clone());
        assert_eq!(value, Value::Markdown(node));
    }

    #[test]
    fn test_value_from_runtime_value() {
        let rt_value = RuntimeValue::Number(Number::from(42.0));
        let value = Value::from(rt_value);
        assert_eq!(value, Value::Number(Number::from(42.0)));
    }

    #[test]
    fn test_value_display() {
        assert_eq!(Value::Number(Number::from(42.0)).to_string(), "42");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::String("hello".to_string()).to_string(), "hello");
        assert_eq!(
            Value::Array(vec!["a".to_string().into(), "b".to_string().into()]).to_string(),
            "[a, b]"
        );
        assert_eq!(Value::None.to_string(), "");
    }

    #[test]
    fn test_value_debug() {
        assert_eq!(format!("{:?}", Value::Number(Number::from(42.0))), "42");
        assert_eq!(format!("{:?}", Value::Bool(true)), "true");
        assert_eq!(
            format!(
                "{:?}",
                Value::Array(vec!["a".to_string().into(), "b".to_string().into()])
            ),
            "a\nb"
        );
        assert_eq!(
            format!("{:?}", Value::String("test".to_string())),
            "\"test\""
        );
    }

    #[test]
    fn test_value_array_operations() {
        let array = Value::Array(vec![
            Value::Number(Number::from(1.0)),
            Value::Number(Number::from(2.0)),
            Value::Number(Number::from(3.0)),
        ]);
        assert_eq!(array.len(), 3);
        assert!(array.is_array());
        assert!(!array.is_empty());
    }

    #[test]
    fn test_values_compact() {
        let values = Values(vec![
            Value::Number(Number::from(1.0)),
            Value::None,
            Value::Number(Number::from(2.0)),
        ]);
        assert_eq!(
            values.compact(),
            vec![
                Value::Number(Number::from(1.0)),
                Value::Number(Number::from(2.0)),
            ]
        );
    }
}
