use std::{cell::RefCell, cmp::Ordering, collections::BTreeMap, rc::Rc};

use crate::{AstParams, Ident, Program, Value, impl_value_formatting, number::Number};
use mq_markdown::Node;

use super::env::Env;

#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Index(usize),
}

#[derive(Clone, PartialEq, Default)]
pub enum RuntimeValue {
    Number(Number),
    Bool(bool),
    String(String),
    Array(Vec<RuntimeValue>),
    Markdown(Node, Option<Selector>),
    Function(AstParams, Program, Rc<RefCell<Env>>),
    NativeFunction(Ident),
    Dict(BTreeMap<String, RuntimeValue>),
    #[default]
    None,
}

impl From<Node> for RuntimeValue {
    fn from(node: Node) -> Self {
        RuntimeValue::Markdown(node, None)
    }
}

impl From<bool> for RuntimeValue {
    fn from(b: bool) -> Self {
        RuntimeValue::Bool(b)
    }
}

impl From<String> for RuntimeValue {
    fn from(s: String) -> Self {
        RuntimeValue::String(s)
    }
}

impl From<&str> for RuntimeValue {
    fn from(s: &str) -> Self {
        RuntimeValue::String(s.to_string())
    }
}

impl From<&mut str> for RuntimeValue {
    fn from(s: &mut str) -> Self {
        RuntimeValue::String(s.to_string())
    }
}

impl From<Number> for RuntimeValue {
    fn from(n: Number) -> Self {
        RuntimeValue::Number(n)
    }
}

impl From<Vec<RuntimeValue>> for RuntimeValue {
    fn from(arr: Vec<RuntimeValue>) -> Self {
        RuntimeValue::Array(arr)
    }
}

impl From<BTreeMap<String, RuntimeValue>> for RuntimeValue {
    fn from(map: BTreeMap<String, RuntimeValue>) -> Self {
        RuntimeValue::Dict(map)
    }
}

impl From<Value> for RuntimeValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Number(n) => RuntimeValue::Number(n),
            Value::Bool(b) => RuntimeValue::Bool(b),
            Value::String(s) => RuntimeValue::String(s),
            Value::Array(a) => RuntimeValue::Array(a.into_iter().map(Into::into).collect()),
            Value::Markdown(m) => RuntimeValue::Markdown(m, None),
            Value::Function(params, program) => {
                RuntimeValue::Function(params, program, Rc::new(RefCell::new(Env::default())))
            }
            Value::NativeFunction(ident) => RuntimeValue::NativeFunction(ident),
            Value::Dict(value_map) => RuntimeValue::Dict(
                value_map
                    .into_iter()
                    .map(|(k, v)| (k, RuntimeValue::from(v)))
                    .collect(),
            ),
            Value::None => RuntimeValue::None,
        }
    }
}

impl PartialOrd for RuntimeValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a.partial_cmp(b),
            (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a.partial_cmp(b),
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a.partial_cmp(b),
            (RuntimeValue::Array(a), RuntimeValue::Array(b)) => a.partial_cmp(b),
            (RuntimeValue::Markdown(a, _), RuntimeValue::Markdown(b, _)) => {
                let a = a.to_string();
                let b = b.to_string();
                a.to_string().partial_cmp(&b)
            }
            (RuntimeValue::Function(a1, b1, _), RuntimeValue::Function(a2, b2, _)) => {
                match a1.partial_cmp(a2) {
                    Some(Ordering::Equal) => b1.partial_cmp(b2),
                    Some(Ordering::Greater) => Some(Ordering::Greater),
                    Some(Ordering::Less) => Some(Ordering::Less),
                    _ => None,
                }
            }
            (RuntimeValue::Dict(_), _) => None,
            (_, RuntimeValue::Dict(_)) => None,
            _ => None,
        }
    }
}

// Use macro to implement Display and Debug traits
impl_value_formatting!(RuntimeValue);

impl RuntimeValue {
    pub const NONE: RuntimeValue = Self::None;
    pub const TRUE: RuntimeValue = Self::Bool(true);
    pub const FALSE: RuntimeValue = Self::Bool(false);
    pub const EMPTY_ARRAY: RuntimeValue = Self::Array(Vec::new());

    #[inline(always)]
    pub fn new_dict() -> RuntimeValue {
        RuntimeValue::Dict(BTreeMap::new())
    }

    #[inline(always)]
    pub fn name(&self) -> &str {
        match self {
            RuntimeValue::Number(_) => "number",
            RuntimeValue::Bool(_) => "bool",
            RuntimeValue::String(_) => "string",
            RuntimeValue::Markdown(_, _) => "markdown",
            RuntimeValue::Array(_) => "array",
            RuntimeValue::None => "None",
            RuntimeValue::Function(_, _, _) => "function",
            RuntimeValue::NativeFunction(_) => "native_function",
            RuntimeValue::Dict(_) => "dict",
        }
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        matches!(self, RuntimeValue::None)
    }

    #[inline(always)]
    pub fn is_function(&self) -> bool {
        matches!(self, RuntimeValue::Function(_, _, _))
    }

    #[inline(always)]
    pub fn is_array(&self) -> bool {
        matches!(self, RuntimeValue::Array(_))
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        match self {
            RuntimeValue::Array(a) => a.is_empty(),
            RuntimeValue::String(s) => s.is_empty(),
            RuntimeValue::Markdown(m, _) => m.value().is_empty(),
            RuntimeValue::Dict(m) => m.is_empty(),
            RuntimeValue::None => true,
            _ => false,
        }
    }

    #[inline(always)]
    pub fn is_truthy(&self) -> bool {
        match self {
            RuntimeValue::Bool(b) => *b,
            RuntimeValue::Number(n) => n.value() != 0.0,
            RuntimeValue::String(s) => !s.is_empty(),
            RuntimeValue::Array(a) => !a.is_empty(),
            RuntimeValue::Markdown(node, selector) => match selector {
                Some(Selector::Index(i)) => node.find_at_index(*i).is_some(),
                None => true,
            },
            RuntimeValue::Function(_, _, _) => true,
            RuntimeValue::NativeFunction(_) => true,
            RuntimeValue::Dict(_) => true,
            RuntimeValue::None => false,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        match self {
            RuntimeValue::Number(n) => n.value() as usize,
            RuntimeValue::Bool(_) => 1,
            RuntimeValue::String(s) => s.len(),
            RuntimeValue::Array(a) => a.len(),
            RuntimeValue::Markdown(m, _) => m.value().len(),
            RuntimeValue::Dict(m) => m.len(),
            RuntimeValue::None => 0,
            RuntimeValue::Function(..) => 0,
            RuntimeValue::NativeFunction(..) => 0,
        }
    }

    #[inline(always)]
    pub fn markdown_node(&self) -> Option<Node> {
        match self {
            RuntimeValue::Markdown(n, Some(Selector::Index(i))) => n.find_at_index(*i),
            RuntimeValue::Markdown(n, _) => Some(n.clone()),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn update_markdown_value(&self, value: &str) -> RuntimeValue {
        match self {
            RuntimeValue::Markdown(n, Some(Selector::Index(i))) => {
                RuntimeValue::Markdown(n.with_children_value(value, *i), Some(Selector::Index(*i)))
            }
            RuntimeValue::Markdown(n, selector) => {
                RuntimeValue::Markdown(n.with_value(value), selector.clone())
            }
            _ => RuntimeValue::NONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{AstExpr, AstNode, arena::ArenaId, ast::node::IdentWithToken};
    use rstest::rstest;
    use smallvec::{SmallVec, smallvec};

    use super::*;

    #[test]
    fn test_runtime_value_from() {
        assert_eq!(RuntimeValue::from(true), RuntimeValue::Bool(true));
        assert_eq!(RuntimeValue::from(false), RuntimeValue::Bool(false));
        assert_eq!(
            RuntimeValue::from(String::from("test")),
            RuntimeValue::String(String::from("test"))
        );
        assert_eq!(
            RuntimeValue::from(Number::from(42.0)),
            RuntimeValue::Number(Number::from(42.0))
        );
    }

    #[rstest]
    #[case(RuntimeValue::Number(Number::from(42.0)), "42")]
    #[case(RuntimeValue::Bool(true), "true")]
    #[case(RuntimeValue::Bool(false), "false")]
    #[case(RuntimeValue::String("hello".to_string()), r#""hello""#)]
    #[case(RuntimeValue::None, "")]
    #[case(RuntimeValue::Array(vec![
            RuntimeValue::Number(Number::from(1.0)),
            RuntimeValue::String("test".to_string())
        ]), r#"[1, "test"]"#)]
    #[case(RuntimeValue::Dict({
            let mut map = BTreeMap::new();
            map.insert("key1".to_string(), RuntimeValue::String("value1".to_string()));
            map.insert("key2".to_string(), RuntimeValue::Number(Number::from(42.0)));
            map
        }), r#"{"key1": "value1", "key2": 42}"#)]
    fn test_string_method(#[case] value: RuntimeValue, #[case] expected: &str) {
        assert_eq!(value.string(), expected);
    }

    #[test]
    fn test_runtime_value_display() {
        assert_eq!(format!("{}", RuntimeValue::Bool(true)), "true");
        assert_eq!(
            format!("{}", RuntimeValue::Number(Number::from(42.0))),
            "42"
        );
        assert_eq!(
            format!("{}", RuntimeValue::String(String::from("test"))),
            "test"
        );
        assert_eq!(format!("{}", RuntimeValue::None), "");
        let map_val = RuntimeValue::Dict(BTreeMap::default());
        assert_eq!(format!("{}", map_val), "{}");
    }

    #[test]
    fn test_runtime_value_debug() {
        assert_eq!(format!("{:?}", RuntimeValue::Bool(true)), "true");
        assert_eq!(
            format!("{:?}", RuntimeValue::Number(Number::from(42.0))),
            "42"
        );
        assert_eq!(
            format!("{:?}", RuntimeValue::String(String::from("test"))),
            "\"test\""
        );
        assert_eq!(format!("{:?}", RuntimeValue::None), "None");

        let mut map = BTreeMap::default();
        map.insert("name".to_string(), RuntimeValue::String("MQ".to_string()));
        map.insert(
            "version".to_string(),
            RuntimeValue::Number(Number::from(1.0)),
        );
        let map_val = RuntimeValue::Dict(map);
        let debug_str = format!("{:?}", map_val);
        assert!(
            debug_str == r#"{"name": "MQ", "version": 1}"#
                || debug_str == r#"{"version": 1, "name": "MQ"}"#
        );
    }

    #[test]
    fn test_runtime_value_name() {
        assert_eq!(RuntimeValue::Bool(true).name(), "bool");
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).name(), "number");
        assert_eq!(RuntimeValue::String(String::from("test")).name(), "string");
        assert_eq!(RuntimeValue::None.name(), "None");
        assert_eq!(
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            )
            .name(),
            "function"
        );
        assert_eq!(
            RuntimeValue::NativeFunction(Ident::new("name")).name(),
            "native_function"
        );
        assert_eq!(
            RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "".to_string(),
                    position: None
                }),
                None
            )
            .name(),
            "markdown"
        );
        assert_eq!(RuntimeValue::Dict(BTreeMap::default()).name(), "dict");
    }

    #[test]
    fn test_runtime_value_is_true() {
        assert!(RuntimeValue::Bool(true).is_truthy());
        assert!(!RuntimeValue::Bool(false).is_truthy());
        assert!(RuntimeValue::Number(Number::from(42.0)).is_truthy());
        assert!(!RuntimeValue::Number(Number::from(0.0)).is_truthy());
        assert!(RuntimeValue::String(String::from("test")).is_truthy());
        assert!(!RuntimeValue::String(String::from("")).is_truthy());
        assert!(RuntimeValue::Array(vec!["".to_string().into()]).is_truthy());
        assert!(!RuntimeValue::Array(Vec::new()).is_truthy());
        assert!(
            RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "".to_string(),
                    position: None
                }),
                None
            )
            .is_truthy()
        );
        assert!(
            !RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "".to_string(),
                    position: None
                }),
                Some(Selector::Index(1))
            )
            .is_truthy()
        );
        assert!(!RuntimeValue::Array(Vec::new()).is_truthy());
        assert!(!RuntimeValue::None.is_truthy());
        assert!(RuntimeValue::NativeFunction(Ident::new("name")).is_truthy());
        assert!(
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            )
            .is_truthy()
        );
        assert!(RuntimeValue::Dict(BTreeMap::default()).is_truthy());
    }

    #[test]
    fn test_runtime_value_partial_ord() {
        assert!(RuntimeValue::Number(Number::from(1.0)) < RuntimeValue::Number(Number::from(2.0)));
        assert!(RuntimeValue::String(String::from("a")) < RuntimeValue::String(String::from("b")));
        assert!(
            RuntimeValue::Array(Vec::new()) < RuntimeValue::Array(vec!["a".to_string().into()])
        );
        assert!(
            RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "test".to_string(),
                    position: None
                }),
                None
            ) < RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "test2".to_string(),
                    position: None
                }),
                None
            )
        );
        assert!(RuntimeValue::Bool(false) < RuntimeValue::Bool(true));
        assert!(
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            ) < RuntimeValue::Function(
                smallvec![Rc::new(AstNode {
                    expr: Rc::new(AstExpr::Ident(IdentWithToken::new("test"))),
                    token_id: ArenaId::new(0),
                })],
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            )
        );
    }

    #[test]
    fn test_runtime_value_len() {
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).len(), 42);
        assert_eq!(RuntimeValue::String(String::from("test")).len(), 4);
        assert_eq!(RuntimeValue::Bool(true).len(), 1);
        assert_eq!(RuntimeValue::Array(vec![RuntimeValue::None]).len(), 1);
        assert_eq!(
            RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "a".to_string(),
                    position: None
                }),
                None
            )
            .len(),
            1
        );
        let mut map = BTreeMap::default();
        map.insert("a".to_string(), RuntimeValue::String("alpha".to_string()));
        map.insert("b".to_string(), RuntimeValue::String("beta".to_string()));
        assert_eq!(RuntimeValue::Dict(map).len(), 2);
    }

    #[test]
    fn test_runtime_value_debug_output() {
        let array = RuntimeValue::Array(vec![
            RuntimeValue::Number(Number::from(1.0)),
            RuntimeValue::String("hello".to_string()),
        ]);
        assert_eq!(format!("{:?}", array), r#"[1, "hello"]"#);

        let node = mq_markdown::Node::Text(mq_markdown::Text {
            value: "test markdown".to_string(),
            position: None,
        });
        let markdown = RuntimeValue::Markdown(node, None);
        assert_eq!(format!("{:?}", markdown), "test markdown");

        let function = RuntimeValue::Function(
            SmallVec::new(),
            Vec::new(),
            Rc::new(RefCell::new(Env::default())),
        );
        assert_eq!(format!("{:?}", function), "function");

        let native_fn = RuntimeValue::NativeFunction(Ident::new("debug"));
        assert_eq!(format!("{:?}", native_fn), "native_function");

        let mut map = BTreeMap::default();
        map.insert("a".to_string(), RuntimeValue::String("alpha".to_string()));
        let map_val = RuntimeValue::Dict(map);
        assert_eq!(format!("{:?}", map_val), r#"{"a": "alpha"}"#);
    }

    #[test]
    fn test_runtime_value_from_value() {
        let num_value = Value::Number(Number::from(42.0));
        assert_eq!(
            RuntimeValue::from(num_value),
            RuntimeValue::Number(Number::from(42.0))
        );

        let bool_value = Value::Bool(true);
        assert_eq!(RuntimeValue::from(bool_value), RuntimeValue::Bool(true));

        let string_value = Value::String("test".to_string());
        assert_eq!(
            RuntimeValue::from(string_value),
            RuntimeValue::String("test".to_string())
        );

        let array_value = Value::Array(vec![Value::Number(Number::from(1.0)), Value::Bool(false)]);
        let expected_array = RuntimeValue::Array(vec![
            RuntimeValue::Number(Number::from(1.0)),
            RuntimeValue::Bool(false),
        ]);
        assert_eq!(RuntimeValue::from(array_value), expected_array);

        let none_value = Value::None;
        assert_eq!(RuntimeValue::from(none_value), RuntimeValue::None);

        let fn_value = Value::Function(SmallVec::new(), Vec::new());
        assert_eq!(
            RuntimeValue::from(fn_value),
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            )
        );

        let ident = Ident::new("test_fn");
        let native_fn_value = Value::NativeFunction(ident);
        assert_eq!(
            RuntimeValue::from(native_fn_value),
            RuntimeValue::NativeFunction(ident)
        );

        let mut value_map = BTreeMap::new();
        value_map.insert("key".to_string(), Value::String("val".to_string()));
        let map_value = Value::Dict(value_map);
        let mut expected_rt_map = BTreeMap::default();
        expected_rt_map.insert("key".to_string(), RuntimeValue::String("val".to_string()));
        assert_eq!(
            RuntimeValue::from(map_value),
            RuntimeValue::Dict(expected_rt_map)
        );
    }

    #[test]
    fn test_runtime_value_markdown() {
        let markdown = RuntimeValue::Markdown("test markdown".to_string().into(), None);
        assert_eq!(markdown.markdown_node().unwrap().value(), "test markdown");

        let updated = markdown.update_markdown_value("updated markdown");
        match &updated {
            RuntimeValue::Markdown(node, selector) => {
                assert_eq!(node.value(), "updated markdown");
                assert_eq!(*selector, None);
            }
            _ => panic!("Expected Markdown variant"),
        }
    }

    #[test]
    fn test_runtime_value_markdown_with_selector() {
        let child1 = mq_markdown::Node::Text(mq_markdown::Text {
            value: "child1".to_string(),
            position: None,
        });
        let child2 = mq_markdown::Node::Text(mq_markdown::Text {
            value: "child2".to_string(),
            position: None,
        });

        let parent = mq_markdown::Node::Strong(mq_markdown::Strong {
            values: vec![child1, child2],
            position: None,
        });

        let markdown_with_selector =
            RuntimeValue::Markdown(parent.clone(), Some(Selector::Index(1)));

        let selected = markdown_with_selector.markdown_node();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().value(), "child2");

        let updated = markdown_with_selector.update_markdown_value("updated child");
        match &updated {
            RuntimeValue::Markdown(node, selector) => {
                assert_eq!(selector, &Some(Selector::Index(1)));
                assert_eq!(node.find_at_index(1).unwrap().value(), "updated child");
            }
            _ => panic!("Expected Markdown variant"),
        }
    }

    #[test]
    fn test_update_markdown_value_non_markdown() {
        assert_eq!(
            RuntimeValue::Number(Number::from(42.0)).update_markdown_value("test"),
            RuntimeValue::NONE
        );
        assert_eq!(
            RuntimeValue::String("hello".to_string()).update_markdown_value("test"),
            RuntimeValue::NONE
        );
        assert_eq!(
            RuntimeValue::Bool(true).update_markdown_value("test"),
            RuntimeValue::NONE
        );
        assert_eq!(
            RuntimeValue::None.update_markdown_value("test"),
            RuntimeValue::NONE
        );
    }

    #[test]
    fn test_runtime_value_map_creation_and_equality() {
        let mut map1_data = BTreeMap::default();
        map1_data.insert("a".to_string(), RuntimeValue::Number(Number::from(1.0)));
        map1_data.insert("b".to_string(), RuntimeValue::String("hello".to_string()));
        let map1 = RuntimeValue::Dict(map1_data);

        let mut map2_data = BTreeMap::default();
        map2_data.insert("a".to_string(), RuntimeValue::Number(Number::from(1.0)));
        map2_data.insert("b".to_string(), RuntimeValue::String("hello".to_string()));
        let map2 = RuntimeValue::Dict(map2_data);

        let mut map3_data = BTreeMap::default();
        map3_data.insert("a".to_string(), RuntimeValue::Number(Number::from(1.0)));
        map3_data.insert("c".to_string(), RuntimeValue::String("world".to_string()));
        let map3 = RuntimeValue::Dict(map3_data);

        assert_eq!(map1, map2);
        assert_ne!(map1, map3);
    }

    #[test]
    fn test_runtime_value_map_is_empty() {
        let empty_map = RuntimeValue::Dict(BTreeMap::default());
        assert!(empty_map.is_empty());

        let mut map_data = BTreeMap::default();
        map_data.insert("a".to_string(), RuntimeValue::Number(Number::from(1.0)));
        let non_empty_map = RuntimeValue::Dict(map_data);
        assert!(!non_empty_map.is_empty());
    }

    #[test]
    fn test_runtime_value_map_partial_ord() {
        let mut map1_data = BTreeMap::default();
        map1_data.insert("a".to_string(), RuntimeValue::Number(Number::from(1.0)));
        let map1 = RuntimeValue::Dict(map1_data);

        let mut map2_data = BTreeMap::default();
        map2_data.insert("b".to_string(), RuntimeValue::Number(Number::from(2.0)));
        let map2 = RuntimeValue::Dict(map2_data);

        assert_eq!(map1.partial_cmp(&map2), None);
        assert_eq!(map2.partial_cmp(&map1), None);
        assert_eq!(map1.partial_cmp(&map1), None);

        let num_val = RuntimeValue::Number(Number::from(5.0));
        assert_eq!(map1.partial_cmp(&num_val), None);
        assert_eq!(num_val.partial_cmp(&map1), None);
    }
}
