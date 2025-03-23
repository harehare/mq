use std::{
    cell::RefCell,
    cmp::Ordering,
    fmt::{self, Debug, Display, Formatter},
    rc::Rc,
    vec,
};

use crate::{AstIdent, AstParams, Program, Value, number::Number};

use mq_markdown::Node;

use super::env::Env;

#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Index(usize),
}

#[derive(Clone, PartialEq)]
pub enum RuntimeValue {
    Number(Number),
    Bool(bool),
    String(String),
    Array(Vec<RuntimeValue>),
    Markdown(Node, Option<Selector>),
    Function(AstParams, Program, Rc<RefCell<Env>>),
    NativeFunction(AstIdent),
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

impl From<Number> for RuntimeValue {
    fn from(n: Number) -> Self {
        RuntimeValue::Number(n)
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
            _ => None,
        }
    }
}

impl Display for RuntimeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.string())
    }
}

impl Debug for RuntimeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        let v = match self {
            RuntimeValue::Number(n) => n.to_string(),
            RuntimeValue::Bool(b) => b.to_string(),
            RuntimeValue::String(s) => format!("\"{}\"", s),
            RuntimeValue::Array(a) => a
                .iter()
                .map(|o| o.string())
                .collect::<Vec<String>>()
                .join("\n"),
            RuntimeValue::Markdown(m, _) => m.to_string(),
            RuntimeValue::None => "None".to_string(),
            RuntimeValue::Function(params, _, _) => format!("function{}", params.len()),
            RuntimeValue::NativeFunction(ident) => format!("native_function: {}", ident),
        };
        write!(f, "{}", v)
    }
}

impl RuntimeValue {
    pub const NONE: RuntimeValue = Self::None;
    pub const TRUE: RuntimeValue = Self::Bool(true);
    pub const FALSE: RuntimeValue = Self::Bool(false);
    pub const EMPTY_ARRAY: RuntimeValue = Self::Array(vec![]);

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
        }
    }

    pub fn text(&self) -> String {
        match self {
            RuntimeValue::None => "None".to_string(),
            RuntimeValue::Number(n) => n.to_string(),
            RuntimeValue::Bool(b) => b.to_string(),
            RuntimeValue::String(s) => s.to_string(),
            RuntimeValue::Array(a) => a
                .iter()
                .map(|o| o.text())
                .collect::<Vec<String>>()
                .join("\n"),
            RuntimeValue::Markdown(m, _) => m.value(),
            RuntimeValue::Function(_, _, _) => "function".to_string(),
            RuntimeValue::NativeFunction(_) => "native_function".to_string(),
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, RuntimeValue::None)
    }

    pub fn is_function(&self) -> bool {
        matches!(self, RuntimeValue::Function(_, _, _))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, RuntimeValue::Array(_))
    }

    pub fn is_true(&self) -> bool {
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
            RuntimeValue::None => false,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            RuntimeValue::Number(n) => n.value() as usize,
            RuntimeValue::Bool(_) => 1,
            RuntimeValue::String(s) => s.len(),
            RuntimeValue::Array(a) => a.len(),
            RuntimeValue::Markdown(m, _) => m.value().len(),
            _ => panic!("not supported"),
        }
    }

    pub fn markdown_node(&self) -> Option<Node> {
        match self {
            RuntimeValue::Markdown(n, Some(Selector::Index(i))) => n.find_at_index(*i),
            RuntimeValue::Markdown(n, _) => Some(n.clone()),
            _ => None,
        }
    }

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

    fn string(&self) -> String {
        match self {
            RuntimeValue::Number(n) => n.to_string(),
            RuntimeValue::Bool(b) => b.to_string(),
            RuntimeValue::String(s) => s.to_string(),
            RuntimeValue::Array(a) => a
                .iter()
                .map(|o| o.string())
                .collect::<Vec<String>>()
                .join("\n"),
            RuntimeValue::Markdown(m, _) => m.to_string(),
            RuntimeValue::None => "None".to_string(),
            RuntimeValue::Function(_, _, _) => "function".to_string(),
            RuntimeValue::NativeFunction(_) => "native_function".to_string(),
        }
    }
}
#[cfg(test)]
mod tests {
    use crate::{AstExpr, AstNode, arena::ArenaId};
    use rstest::rstest;

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
    #[case(RuntimeValue::String("hello".to_string()), "hello")]
    #[case(RuntimeValue::None, "None")]
    #[case(RuntimeValue::Array(vec![
            RuntimeValue::Number(Number::from(1.0)),
            RuntimeValue::String("test".to_string())
        ]), "1\ntest")]
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
        assert_eq!(format!("{}", RuntimeValue::None), "None");
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
    }

    #[test]
    fn test_runtime_value_string() {
        #[rstest]
        #[case(RuntimeValue::Number(Number::from(42.0)), "42")]
        #[case(RuntimeValue::Bool(true), "true")]
        #[case(RuntimeValue::Bool(false), "false")]
        #[case(RuntimeValue::String("hello".to_string()), "hello")]
        #[case(RuntimeValue::None, "None")]
        #[case(RuntimeValue::Array(vec![
            RuntimeValue::Number(Number::from(1.0)),
            RuntimeValue::String("test".to_string())
        ]), "1\ntest")]
        fn test_string_method(#[case] value: RuntimeValue, #[case] expected: &str) {
            assert_eq!(value.string(), expected);
        }

        let markdown_node = mq_markdown::Node::Text(mq_markdown::Text {
            value: "test markdown".to_string(),
            position: None,
        });
        assert_eq!(
            RuntimeValue::Markdown(markdown_node, None).string(),
            "test markdown"
        );

        let function =
            RuntimeValue::Function(vec![], vec![], Rc::new(RefCell::new(Env::default())));
        assert_eq!(function.string(), "function");

        let native_fn = RuntimeValue::NativeFunction(AstIdent::new("print"));
        assert_eq!(native_fn.string(), "native_function");
    }

    #[test]
    fn test_runtime_value_name() {
        assert_eq!(RuntimeValue::Bool(true).name(), "bool");
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).name(), "number");
        assert_eq!(RuntimeValue::String(String::from("test")).name(), "string");
        assert_eq!(RuntimeValue::None.name(), "None");
        assert_eq!(
            RuntimeValue::Function(vec![], vec![], Rc::new(RefCell::new(Env::default()))).name(),
            "function"
        );
        assert_eq!(
            RuntimeValue::NativeFunction(AstIdent::new("name")).name(),
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
    }

    #[test]
    fn test_runtime_value_text() {
        assert_eq!(RuntimeValue::Bool(true).text(), "true");
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).text(), "42");
        assert_eq!(RuntimeValue::String(String::from("test")).text(), "test");
        assert_eq!(
            RuntimeValue::Array(vec!["test1".to_string().into(), "test2".to_string().into()])
                .text(),
            "test1\ntest2"
        );
        assert_eq!(RuntimeValue::None.text(), "None");
        assert_eq!(
            RuntimeValue::Function(vec![], vec![], Rc::new(RefCell::new(Env::default()))).text(),
            "function"
        );
        assert_eq!(
            RuntimeValue::NativeFunction(AstIdent::new("name")).text(),
            "native_function"
        );
        assert_eq!(
            RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "value".to_string(),
                    position: None
                }),
                None
            )
            .text(),
            "value"
        );
    }

    #[test]
    fn test_runtime_value_is_true() {
        assert!(RuntimeValue::Bool(true).is_true());
        assert!(!RuntimeValue::Bool(false).is_true());
        assert!(RuntimeValue::Number(Number::from(42.0)).is_true());
        assert!(!RuntimeValue::Number(Number::from(0.0)).is_true());
        assert!(RuntimeValue::String(String::from("test")).is_true());
        assert!(!RuntimeValue::String(String::from("")).is_true());
        assert!(RuntimeValue::Array(vec!["".to_string().into()]).is_true());
        assert!(!RuntimeValue::Array(vec![]).is_true());
        assert!(
            RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "".to_string(),
                    position: None
                }),
                None
            )
            .is_true()
        );
        assert!(
            !RuntimeValue::Markdown(
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: "".to_string(),
                    position: None
                }),
                Some(Selector::Index(1))
            )
            .is_true()
        );
        assert!(!RuntimeValue::Array(vec![]).is_true());
        assert!(!RuntimeValue::None.is_true());
    }

    #[test]
    fn test_runtime_value_partial_ord() {
        assert!(RuntimeValue::Number(Number::from(1.0)) < RuntimeValue::Number(Number::from(2.0)));
        assert!(RuntimeValue::String(String::from("a")) < RuntimeValue::String(String::from("b")));
        assert!(RuntimeValue::Array(vec![]) < RuntimeValue::Array(vec!["a".to_string().into()]));
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
            RuntimeValue::Function(vec![], vec![], Rc::new(RefCell::new(Env::default())))
                < RuntimeValue::Function(
                    vec![Rc::new(AstNode {
                        expr: Rc::new(AstExpr::Ident(AstIdent::new("test"))),
                        token_id: ArenaId::new(0),
                    })],
                    vec![],
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
    }

    #[test]
    fn test_runtime_value_debug_output() {
        let array = RuntimeValue::Array(vec![
            RuntimeValue::Number(Number::from(1.0)),
            RuntimeValue::String("hello".to_string()),
        ]);
        assert_eq!(format!("{:?}", array), "1\nhello");

        let node = mq_markdown::Node::Text(mq_markdown::Text {
            value: "test markdown".to_string(),
            position: None,
        });
        let markdown = RuntimeValue::Markdown(node, None);
        assert_eq!(format!("{:?}", markdown), "test markdown");

        let function =
            RuntimeValue::Function(vec![], vec![], Rc::new(RefCell::new(Env::default())));
        assert_eq!(format!("{:?}", function), "function0");

        let native_fn = RuntimeValue::NativeFunction(AstIdent::new("debug"));
        assert_eq!(format!("{:?}", native_fn), "native_function: debug");
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

        let fn_value = Value::Function(vec![], vec![]);
        assert_eq!(
            RuntimeValue::from(fn_value),
            RuntimeValue::Function(vec![], vec![], Rc::new(RefCell::new(Env::default())))
        );

        let ident = AstIdent::new("test_fn");
        let native_fn_value = Value::NativeFunction(ident.clone());
        assert_eq!(
            RuntimeValue::from(native_fn_value),
            RuntimeValue::NativeFunction(ident)
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

        let parent = mq_markdown::Node::Strong(mq_markdown::Value {
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
}
