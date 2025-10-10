use super::env::Env;
use crate::{AstParams, Ident, Program, Shared, SharedCell, number::Number};
use mq_markdown::Node;
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::BTreeMap,
    ops::{Index, IndexMut},
};

#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Index(usize),
}

#[derive(Clone, Default)]
pub enum RuntimeValue {
    Number(Number),
    Bool(bool),
    String(String),
    Symbol(Ident),
    Array(Vec<RuntimeValue>),
    Markdown(Node, Option<Selector>),
    Function(AstParams, Program, Shared<SharedCell<Env>>),
    NativeFunction(Ident),
    Dict(BTreeMap<Ident, RuntimeValue>),
    #[default]
    None,
}

// Custom PartialEq implementation to avoid comparing Env pointers
impl PartialEq for RuntimeValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a == b,
            (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a == b,
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
            (RuntimeValue::Symbol(a), RuntimeValue::Symbol(b)) => a == b,
            (RuntimeValue::Array(a), RuntimeValue::Array(b)) => a == b,
            (RuntimeValue::Markdown(a, sa), RuntimeValue::Markdown(b, sb)) => a == b && sa == sb,
            (RuntimeValue::Function(a1, b1, _), RuntimeValue::Function(a2, b2, _)) => {
                a1 == a2 && b1 == b2
            }
            (RuntimeValue::NativeFunction(a), RuntimeValue::NativeFunction(b)) => a == b,
            (RuntimeValue::Dict(a), RuntimeValue::Dict(b)) => a == b,
            (RuntimeValue::None, RuntimeValue::None) => true,
            _ => false,
        }
    }
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

impl From<Ident> for RuntimeValue {
    fn from(i: Ident) -> Self {
        RuntimeValue::Symbol(i)
    }
}

impl From<usize> for RuntimeValue {
    fn from(n: usize) -> Self {
        RuntimeValue::Number(Number::from(n))
    }
}

impl From<Vec<RuntimeValue>> for RuntimeValue {
    fn from(arr: Vec<RuntimeValue>) -> Self {
        RuntimeValue::Array(arr)
    }
}

impl From<BTreeMap<Ident, RuntimeValue>> for RuntimeValue {
    fn from(map: BTreeMap<Ident, RuntimeValue>) -> Self {
        RuntimeValue::Dict(map)
    }
}

impl PartialOrd for RuntimeValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a.partial_cmp(b),
            (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a.partial_cmp(b),
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a.partial_cmp(b),
            (RuntimeValue::Symbol(a), RuntimeValue::Symbol(b)) => a.partial_cmp(b),
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

impl std::fmt::Display for RuntimeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let value: Cow<'_, str> = match self {
            Self::Number(n) => Cow::Owned(n.to_string()),
            Self::Bool(b) => Cow::Owned(b.to_string()),
            Self::String(s) => Cow::Borrowed(s),
            Self::Symbol(i) => Cow::Owned(format!(":{}", i)),
            Self::Array(_) => self.string(),
            Self::Markdown(m, ..) => Cow::Owned(m.to_string()),
            Self::None => Cow::Borrowed(""),
            Self::Function(params, ..) => Cow::Owned(format!("function/{}", params.len())),
            Self::NativeFunction(_) => Cow::Borrowed("native_function"),
            Self::Dict(_) => self.string(),
        };
        write!(f, "{}", value)
    }
}

impl std::fmt::Debug for RuntimeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let v: Cow<'_, str> = match self {
            Self::None => Cow::Borrowed("None"),
            a => a.string(),
        };
        write!(f, "{}", v)
    }
}

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
            RuntimeValue::Symbol(_) => "symbol",
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
    pub fn is_native_function(&self) -> bool {
        matches!(self, RuntimeValue::NativeFunction(_))
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
            RuntimeValue::Symbol(_)
            | RuntimeValue::Function(_, _, _)
            | RuntimeValue::NativeFunction(_)
            | RuntimeValue::Dict(_) => true,
            RuntimeValue::None => false,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        match self {
            RuntimeValue::Number(n) => n.value() as usize,
            RuntimeValue::Bool(_) => 1,
            RuntimeValue::String(s) => s.len(),
            RuntimeValue::Symbol(i) => i.as_str().len(),
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

    #[inline(always)]
    pub fn position(&self) -> Option<mq_markdown::Position> {
        match self {
            RuntimeValue::Markdown(node, _) => node.position(),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn set_position(&mut self, position: Option<mq_markdown::Position>) {
        if let RuntimeValue::Markdown(node, _) = self {
            node.set_position(position);
        }
    }

    #[inline(always)]
    fn string(&self) -> Cow<'_, str> {
        match self {
            Self::Number(n) => Cow::Owned(n.to_string()),
            Self::Bool(b) => Cow::Owned(b.to_string()),
            Self::String(s) => Cow::Owned(format!(r#""{}""#, s)),
            Self::Symbol(i) => Cow::Owned(format!(":{}", i)),
            Self::Array(a) => Cow::Owned(format!(
                "[{}]",
                a.iter()
                    .map(|v| v.string())
                    .collect::<Vec<Cow<str>>>()
                    .join(", ")
            )),
            Self::Markdown(m, ..) => Cow::Owned(m.to_string()),
            Self::None => Cow::Borrowed(""),
            Self::Function(..) => Cow::Borrowed("function"),
            Self::NativeFunction(_) => Cow::Borrowed("native_function"),
            Self::Dict(map) => {
                let items = map
                    .iter()
                    .map(|(k, v)| format!("\"{}\": {}", k, v.string()))
                    .collect::<Vec<String>>()
                    .join(", ");
                Cow::Owned(format!("{{{}}}", items))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeValues(Vec<RuntimeValue>);

impl From<Vec<RuntimeValue>> for RuntimeValues {
    fn from(values: Vec<RuntimeValue>) -> Self {
        Self(values)
    }
}

impl Index<usize> for RuntimeValues {
    type Output = RuntimeValue;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for RuntimeValues {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl IntoIterator for RuntimeValues {
    type Item = RuntimeValue;
    type IntoIter = std::vec::IntoIter<RuntimeValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl RuntimeValues {
    pub fn compact(&self) -> Vec<RuntimeValue> {
        self.0
            .iter()
            .filter(|v| !v.is_none() && !v.is_empty())
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn values(&self) -> &Vec<RuntimeValue> {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    pub fn update_with(&self, other: Self) -> Self {
        self.0
            .clone()
            .into_iter()
            .zip(other)
            .map(|(current_value, mut updated_value)| {
                updated_value.set_position(current_value.position());

                if let RuntimeValue::Markdown(node, _) = &current_value {
                    match &updated_value {
                        RuntimeValue::None
                        | RuntimeValue::Function(_, _, _)
                        | RuntimeValue::NativeFunction(_) => current_value.clone(),
                        RuntimeValue::Markdown(node, _) if node.is_empty() => current_value.clone(),
                        RuntimeValue::Markdown(node, _) => {
                            if node.is_fragment() {
                                if let RuntimeValue::Markdown(mut current_node, selector) =
                                    current_value
                                {
                                    current_node.apply_fragment(node.clone());
                                    RuntimeValue::Markdown(current_node, selector)
                                } else {
                                    updated_value
                                }
                            } else {
                                updated_value
                            }
                        }
                        RuntimeValue::String(s) => {
                            RuntimeValue::Markdown(node.clone().with_value(s), None)
                        }
                        RuntimeValue::Symbol(i) => {
                            RuntimeValue::Markdown(node.clone().with_value(&i.as_str()), None)
                        }
                        RuntimeValue::Bool(b) => RuntimeValue::Markdown(
                            node.clone().with_value(b.to_string().as_str()),
                            None,
                        ),
                        RuntimeValue::Number(n) => RuntimeValue::Markdown(
                            node.clone().with_value(n.to_string().as_str()),
                            None,
                        ),
                        RuntimeValue::Array(array) => RuntimeValue::Array(
                            array
                                .iter()
                                .filter_map(|o| {
                                    if !matches!(o, RuntimeValue::None) {
                                        Some(RuntimeValue::Markdown(
                                            node.clone().with_value(o.to_string().as_str()),
                                            None,
                                        ))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>(),
                        ),
                        RuntimeValue::Dict(map) => {
                            let mut new_dict = BTreeMap::new();
                            for (k, v) in map {
                                if !v.is_none() && !v.is_empty() {
                                    new_dict.insert(
                                        *k,
                                        RuntimeValue::Markdown(
                                            node.clone().with_value(v.to_string().as_str()),
                                            None,
                                        ),
                                    );
                                }
                            }
                            RuntimeValue::Dict(new_dict)
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
            map.insert(Ident::new("key1"), RuntimeValue::String("value1".to_string()));
            map.insert(Ident::new("key2"), RuntimeValue::Number(Number::from(42.0)));
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
        map.insert(Ident::new("name"), RuntimeValue::String("MQ".to_string()));
        map.insert(
            Ident::new("version"),
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
                Shared::new(SharedCell::new(Env::default()))
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
                Shared::new(SharedCell::new(Env::default()))
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
                Shared::new(SharedCell::new(Env::default()))
            ) < RuntimeValue::Function(
                smallvec![Shared::new(AstNode {
                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("test"))),
                    token_id: ArenaId::new(0),
                })],
                Vec::new(),
                Shared::new(SharedCell::new(Env::default()))
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
        map.insert(Ident::new("a"), RuntimeValue::String("alpha".to_string()));
        map.insert(Ident::new("b"), RuntimeValue::String("beta".to_string()));
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
            Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(format!("{:?}", function), "function");

        let native_fn = RuntimeValue::NativeFunction(Ident::new("debug"));
        assert_eq!(format!("{:?}", native_fn), "native_function");

        let mut map = BTreeMap::default();
        map.insert(Ident::new("a"), RuntimeValue::String("alpha".to_string()));
        let map_val = RuntimeValue::Dict(map);
        assert_eq!(format!("{:?}", map_val), r#"{"a": "alpha"}"#);
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
        map1_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        map1_data.insert(Ident::new("b"), RuntimeValue::String("hello".to_string()));
        let map1 = RuntimeValue::Dict(map1_data);

        let mut map2_data = BTreeMap::default();
        map2_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        map2_data.insert(Ident::new("b"), RuntimeValue::String("hello".to_string()));
        let map2 = RuntimeValue::Dict(map2_data);

        let mut map3_data = BTreeMap::default();
        map3_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        map3_data.insert(Ident::new("c"), RuntimeValue::String("world".to_string()));
        let map3 = RuntimeValue::Dict(map3_data);

        assert_eq!(map1, map2);
        assert_ne!(map1, map3);
    }

    #[test]
    fn test_runtime_value_map_is_empty() {
        let empty_map = RuntimeValue::Dict(BTreeMap::default());
        assert!(empty_map.is_empty());

        let mut map_data = BTreeMap::default();
        map_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        let non_empty_map = RuntimeValue::Dict(map_data);
        assert!(!non_empty_map.is_empty());
    }

    #[test]
    fn test_runtime_value_map_partial_ord() {
        let mut map1_data = BTreeMap::default();
        map1_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        let map1 = RuntimeValue::Dict(map1_data);

        let mut map2_data = BTreeMap::default();
        map2_data.insert(Ident::new("b"), RuntimeValue::Number(Number::from(2.0)));
        let map2 = RuntimeValue::Dict(map2_data);

        assert_eq!(map1.partial_cmp(&map2), None);
        assert_eq!(map2.partial_cmp(&map1), None);
        assert_eq!(map1.partial_cmp(&map1), None);

        let num_val = RuntimeValue::Number(Number::from(5.0));
        assert_eq!(map1.partial_cmp(&num_val), None);
        assert_eq!(num_val.partial_cmp(&map1), None);
    }
}
