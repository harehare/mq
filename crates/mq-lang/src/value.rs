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

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<Number> for Value {
    fn from(n: Number) -> Self {
        Value::Number(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Number(n.into())
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
            RuntimeValue::Markdown(m, _) => Value::Markdown(m),
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
            Value::Array(a) => a
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<String>>()
                .join("\n"),
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

    pub fn position(&self) -> Option<mq_markdown::Position> {
        match self {
            Value::Markdown(node) => node.position(),
            _ => None,
        }
    }

    pub fn set_position(&mut self, position: Option<mq_markdown::Position>) {
        if let Value::Markdown(node) = self {
            node.set_position(position);
        }
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
            .filter(|v| !v.is_none() && !v.is_empty())
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

    pub fn update_with(&self, other: Values) -> Self {
        self.0
            .clone()
            .into_iter()
            .zip(other)
            .map(|(current_value, mut updated_value)| {
                updated_value.set_position(current_value.position());

                if let Value::Markdown(node) = &current_value {
                    match &updated_value {
                        Value::None | Value::Function(_, _) | Value::NativeFunction(_) => {
                            current_value.clone()
                        }
                        Value::Markdown(node) if node.is_empty() => current_value.clone(),
                        Value::Markdown(node) => {
                            if node.is_fragment() {
                                if let Value::Markdown(mut current_node) = current_value {
                                    current_node.apply_fragment(node.clone());
                                    Value::Markdown(current_node)
                                } else {
                                    updated_value
                                }
                            } else {
                                updated_value
                            }
                        }
                        Value::String(s) => Value::Markdown(node.clone().with_value(s)),
                        Value::Bool(b) => {
                            Value::Markdown(node.clone().with_value(b.to_string().as_str()))
                        }
                        Value::Number(n) => {
                            Value::Markdown(node.clone().with_value(n.to_string().as_str()))
                        }
                        Value::Array(array) => Value::Array(
                            array
                                .iter()
                                .filter_map(|o| {
                                    if !matches!(o, Value::None) {
                                        Some(Value::Markdown(
                                            node.clone().with_value(o.to_string().as_str()),
                                        ))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>(),
                        ),
                    }
                } else {
                    unreachable!()
                }
            })
            .collect::<Vec<_>>()
            .into()
    }
}

#[cfg(test)]
mod tests {
    use mq_markdown::Text;
    use rstest::rstest;
    use smallvec::SmallVec;

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

    #[rstest]
    #[case(true, Value::TRUE)]
    #[case(true, Value::TRUE)]
    fn test_value_from_bool(#[case] input: bool, #[case] expected: Value) {
        let value = Value::from(input);
        assert_eq!(value, expected);
    }

    #[test]
    fn test_value_from_string() {
        let value = Value::from("hello".to_string());
        assert_eq!(value, Value::String("hello".to_string()));

        let value = Value::from("world");
        assert_eq!(value, Value::String("world".to_string()));

        let value = Value::from("");
        assert_eq!(value, Value::String("".to_string()));

        let value = Value::from("!@#$%^&*()");
        assert_eq!(value, Value::String("!@#$%^&*()".to_string()));
    }

    #[test]
    fn test_value_from_number() {
        let value = Value::from(Number::new(42.0));
        assert_eq!(value, Value::Number(Number::new(42.0)));

        let value = Value::from(42);
        assert_eq!(value, Value::Number(Number::new(42.0)));
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
        assert_eq!(
            format!(
                "{:?}",
                Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "test".to_string(),
                    position: None
                }))
            ),
            "test"
        );
        assert_eq!(format!("{:?}", Value::NONE), "None");
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
    fn test_value_len() {
        assert_eq!(Value::Number(Number::from(5.0)).len(), 5);
        assert_eq!(Value::Bool(true).len(), 1);
        assert_eq!(Value::Bool(false).len(), 1);
        assert_eq!(Value::String("hello".to_string()).len(), 5);
        assert_eq!(
            Value::Array(vec![
                Value::Number(Number::from(1.0)),
                Value::Number(Number::from(2.0))
            ])
            .len(),
            2
        );

        let markdown_node = Node::Text(Text {
            value: "test text".to_string(),
            position: None,
        });
        assert_eq!(Value::Markdown(markdown_node).len(), 9);
    }

    #[test]
    fn test_values_len_and_empty() {
        let empty_values = Values(Vec::new());
        assert_eq!(empty_values.len(), 0);
        assert!(empty_values.is_empty());

        let values = Values(vec![Value::Number(Number::from(1.0)), Value::None]);
        assert_eq!(values.len(), 2);
        assert!(!values.is_empty());
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
    #[rstest]
    #[case(Value::Markdown(Node::Text(Text {
        value: "test".to_string(),
        position: None,
    })), Value::String("updated".to_string()),
       Value::Markdown(Node::Text(Text {
           value: "updated".to_string(),
           position: None,
       })))]
    #[case(Value::Markdown(Node::Text(Text {
        value: "test".to_string(),
        position: None,
    })), Value::Bool(true),
       Value::Markdown(Node::Text(Text {
           value: "true".to_string(),
           position: None,
       })))]
    #[case(Value::Markdown(Node::Text(Text {
        value: "test".to_string(),
        position: None,
    })), Value::Number(Number::from(42.0)),
       Value::Markdown(Node::Text(Text {
           value: "42".to_string(),
           position: None,
       })))]
    #[case(Value::Markdown(Node::Text(Text {
        value: "test".to_string(),
        position: None,
    })), Value::None,
       Value::Markdown(Node::Text(Text {
           value: "test".to_string(),
           position: None,
       })))]
    #[case(Value::Markdown(Node::Text(Text {
                value: "test".to_string(),
                position: None,
           })),
           Value::Array(vec![
                Value::String("item1".to_string()),
                Value::String("item2".to_string()),
           ]),
           Value::Array(vec![
               Value::Markdown(Node::Text(Text {
                    value: "item1".to_string(),
                    position: None,
               })),
               Value::Markdown(Node::Text(Text {
                    value: "item2".to_string(),
                    position: None,
               })),
        ]))]
    #[case(Value::Markdown(Node::Text(Text {
               value: "updated".to_string(),
               position: None,
           })), Value::Function(SmallVec::new(), Vec::new()),
           Value::Markdown(Node::Text(Text {
               value: "updated".to_string(),
               position: None,
           }))
       )]
    fn test_values_update_with(
        #[case] original: Value,
        #[case] update: Value,
        #[case] expected: Value,
    ) {
        let values = Values(vec![original]);
        let update_values = Values(vec![update]);
        let result = values.update_with(update_values);
        assert_eq!(result.0[0], expected);
    }
}
