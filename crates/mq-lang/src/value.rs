use std::collections::BTreeMap;

use crate::{
    AstIdent, AstParams, Program, eval::runtime_value::RuntimeValue, impl_value_formatting,
    number::Number,
};

use mq_markdown::Node;

/// Represents a value in the mq language.
///
/// Values are the fundamental data types that can be manipulated and processed
/// within mq expressions. They include primitive types like numbers and strings,
/// as well as complex types like arrays, dictionaries, and Markdown nodes.
#[derive(Clone, PartialEq)]
pub enum Value {
    /// A numeric value (integer or floating-point)
    Number(Number),
    /// A boolean value (true or false)
    Bool(bool),
    /// A string value
    String(String),
    /// An array of values
    Array(Vec<Value>),
    /// A Markdown node
    Markdown(Node),
    /// A user-defined function with parameters and body
    Function(AstParams, Program),
    /// A built-in native function
    NativeFunction(AstIdent),
    /// A dictionary/map of string keys to values
    Dict(BTreeMap<String, Value>),
    /// Represents no value or null
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

impl From<BTreeMap<String, Value>> for Value {
    fn from(dict: BTreeMap<String, Value>) -> Self {
        Value::Dict(dict)
    }
}

impl From<RuntimeValue> for Value {
    fn from(value: RuntimeValue) -> Self {
        match value {
            RuntimeValue::Number(n) => Value::Number(n),
            RuntimeValue::Bool(b) => Value::Bool(b),
            RuntimeValue::String(s) => Value::String(s),
            RuntimeValue::Array(a) => Value::Array(a.into_iter().map(Into::into).collect()),
            RuntimeValue::Markdown(m, _) => Value::Markdown(m),
            RuntimeValue::Function(params, program, _) => Value::Function(params, program),
            RuntimeValue::NativeFunction(ident) => Value::NativeFunction(ident),
            RuntimeValue::Dict(rt_map) => Value::Dict(
                rt_map
                    .into_iter()
                    .map(|(k, v)| (k, Value::from(v)))
                    .collect(),
            ),
            RuntimeValue::None => Value::None,
        }
    }
}

// Use macro to implement Display and Debug traits
impl_value_formatting!(Value);

impl Value {
    pub const NONE: Value = Self::None;
    pub const TRUE: Value = Self::Bool(true);
    pub const FALSE: Value = Self::Bool(false);

    /// Creates a new empty dictionary value.
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::Value;
    ///
    /// let dict = Value::new_dict();
    /// assert!(matches!(dict, Value::Dict(_)));
    /// assert_eq!(dict.len(), 0);
    /// ```
    pub fn new_dict() -> Self {
        Value::Dict(BTreeMap::new())
    }

    /// Returns true if the value is a number.
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::Value;
    ///
    /// let num = Value::from(42);
    /// assert!(num.is_number());
    ///
    /// let text = Value::from("hello");
    /// assert!(!text.is_number());
    /// ```
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }

    /// Returns true if the value is None.
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::Value;
    ///
    /// let none = Value::NONE;
    /// assert!(none.is_none());
    ///
    /// let text = Value::from("hello");
    /// assert!(!text.is_none());
    /// ```
    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }

    /// Returns true if the value is a function.
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::Value;
    ///
    /// let text = Value::from("hello");
    /// assert!(!text.is_function());
    /// ```
    pub fn is_function(&self) -> bool {
        matches!(self, Value::Function(_, _))
    }

    /// Returns true if the value is an array.
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::Value;
    ///
    /// let arr = Value::Array(vec![Value::from(1), Value::from(2)]);
    /// assert!(arr.is_array());
    ///
    /// let text = Value::from("hello");
    /// assert!(!text.is_array());
    /// ```
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    /// Returns the length of the value.
    ///
    /// For different value types:
    /// - Number: the numeric value as usize
    /// - Bool: always 1
    /// - String: character count
    /// - Array: number of elements
    /// - Markdown: length of the text content
    /// - Dict: number of key-value pairs
    ///
    /// # Panics
    ///
    /// Panics if called on Function or NativeFunction values.
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::Value;
    ///
    /// let text = Value::from("hello");
    /// assert_eq!(text.len(), 5);
    ///
    /// let arr = Value::Array(vec![Value::from(1), Value::from(2)]);
    /// assert_eq!(arr.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        match self {
            Value::Number(n) => n.value() as usize,
            Value::Bool(_) => 1,
            Value::String(s) => s.len(),
            Value::Array(a) => a.len(),
            Value::Markdown(m) => m.value().len(),
            Value::Dict(m) => m.len(),
            _ => panic!("len() not supported for this value type"),
        }
    }

    /// Returns true if the value is empty.
    ///
    /// Uses the same logic as `len()` to determine emptiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mq_lang::Value;
    ///
    /// let empty_string = Value::from("");
    /// assert!(empty_string.is_empty());
    ///
    /// let text = Value::from("hello");
    /// assert!(!text.is_empty());
    /// ```
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
                        Value::Dict(map) => {
                            let mut new_dict = BTreeMap::new();
                            for (k, v) in map {
                                if !v.is_none() && !v.is_empty() {
                                    new_dict.insert(
                                        k.clone(),
                                        Value::Markdown(
                                            node.clone().with_value(v.to_string().as_str()),
                                        ),
                                    );
                                }
                            }
                            Value::Dict(new_dict)
                        }
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

        let mut rt_map = BTreeMap::default();
        rt_map.insert(
            "key1".to_string(),
            RuntimeValue::String("value1".to_string()),
        );
        rt_map.insert(
            "key2".to_string(),
            RuntimeValue::Number(Number::from(123.0)),
        );
        let rt_value_map = RuntimeValue::Dict(rt_map);
        let value_map = Value::from(rt_value_map);
        let mut expected_map = BTreeMap::new();
        expected_map.insert("key1".to_string(), Value::String("value1".to_string()));
        expected_map.insert("key2".to_string(), Value::Number(Number::from(123.0)));
        assert_eq!(value_map, Value::Dict(expected_map));
    }

    #[test]
    fn test_value_display() {
        assert_eq!(Value::Number(Number::from(42.0)).to_string(), "42");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::String("hello".to_string()).to_string(), "hello");
        assert_eq!(
            Value::Array(vec!["a".to_string().into(), "b".to_string().into()]).to_string(),
            r#"["a", "b"]"#
        );
        assert_eq!(Value::None.to_string(), "");
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), Value::String("test".to_string()));
        map.insert("count".to_string(), Value::Number(Number::from(42.0)));
        let map_val = Value::Dict(map);
        assert_eq!(map_val.to_string(), r#"{"count": 42, "name": "test"}"#);
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
            r#"["a", "b"]"#
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
        let mut map = BTreeMap::new();
        map.insert("key1".to_string(), Value::String("value1".to_string()));
        map.insert("key2".to_string(), Value::Number(Number::from(123.0)));
        assert_eq!(Value::Dict(map.clone()).len(), 2);

        let markdown_node = Node::Text(Text {
            value: "test text".to_string(),
            position: None,
        });
        assert_eq!(Value::Markdown(markdown_node).len(), 9);
    }

    #[test]
    fn test_value_map_is_empty() {
        let mut map = BTreeMap::new();
        assert!(Value::Dict(map.clone()).is_empty());
        map.insert("key1".to_string(), Value::String("value1".to_string()));
        assert!(!Value::Dict(map).is_empty());
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

    #[test]
    fn test_value_map_creation_and_equality() {
        let mut map1 = BTreeMap::new();
        map1.insert("a".to_string(), Value::Number(1.into()));
        map1.insert("b".to_string(), Value::String("hello".into()));
        let value_map1 = Value::Dict(map1.clone());

        let mut map2 = BTreeMap::new();
        map2.insert("a".to_string(), Value::Number(1.into()));
        map2.insert("b".to_string(), Value::String("hello".into()));
        let value_map2 = Value::Dict(map2.clone());

        let mut map3 = BTreeMap::new();
        map3.insert("a".to_string(), Value::Number(1.into()));
        map3.insert("c".to_string(), Value::String("world".into()));
        let value_map3 = Value::Dict(map3.clone());

        assert_eq!(value_map1, value_map2);
        assert_ne!(value_map1, value_map3);
    }

    #[test]
    fn test_value_map_debug_formatting() {
        let mut map = BTreeMap::new();
        map.insert("name".to_string(), Value::String("MQ".to_string()));
        map.insert("version".to_string(), Value::Number(1.into()));
        let value_map = Value::Dict(map);

        // The order of items in a HashMap is not guaranteed, so we need to check for both possible orderings
        let option1 = r#"{"name": "MQ", "version": 1}"#;
        let option2 = r#"{"version": 1, "name": "MQ"}"#;

        let debug_str = format!("{:?}", value_map);
        assert!(debug_str == option1 || debug_str == option2);

        let mut nested_map = BTreeMap::new();
        nested_map.insert("key".to_string(), Value::String("value".to_string()));
        let mut map_with_nested = BTreeMap::new();
        map_with_nested.insert("outer_key".to_string(), Value::Dict(nested_map));
        let value_map_nested = Value::Dict(map_with_nested);
        assert_eq!(
            format!("{:?}", value_map_nested),
            r#"{"outer_key": {"key": "value"}}"#
        );
    }
}
