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

#[derive(Clone, PartialEq)]
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
                RuntimeValue::Function(params, program, Rc::new(RefCell::new(Env::new(None))))
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
                Some(Selector::Index(i)) => node.find_children(*i).is_some(),
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
            RuntimeValue::Markdown(n, Some(Selector::Index(i))) => n.find_children(*i),
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
    fn test_runtime_value_name() {
        assert_eq!(RuntimeValue::Bool(true).name(), "bool");
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).name(), "number");
        assert_eq!(RuntimeValue::String(String::from("test")).name(), "string");
        assert_eq!(RuntimeValue::None.name(), "None");
    }

    #[test]
    fn test_runtime_value_text() {
        assert_eq!(RuntimeValue::Bool(true).text(), "true");
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).text(), "42");
        assert_eq!(RuntimeValue::String(String::from("test")).text(), "test");
        assert_eq!(RuntimeValue::None.text(), "None");
    }

    #[test]
    fn test_runtime_value_is_true() {
        assert!(RuntimeValue::Bool(true).is_true());
        assert!(!RuntimeValue::Bool(false).is_true());
        assert!(RuntimeValue::Number(Number::from(42.0)).is_true());
        assert!(!RuntimeValue::Number(Number::from(0.0)).is_true());
        assert!(RuntimeValue::String(String::from("test")).is_true());
        assert!(!RuntimeValue::String(String::from("")).is_true());
        assert!(!RuntimeValue::None.is_true());
    }

    #[test]
    fn test_runtime_value_partial_ord() {
        assert!(RuntimeValue::Number(Number::from(1.0)) < RuntimeValue::Number(Number::from(2.0)));
        assert!(RuntimeValue::String(String::from("a")) < RuntimeValue::String(String::from("b")));
        assert!(RuntimeValue::Bool(false) < RuntimeValue::Bool(true));
    }

    #[test]
    fn test_runtime_value_len() {
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).len(), 42);
        assert_eq!(RuntimeValue::String(String::from("test")).len(), 4);
        assert_eq!(RuntimeValue::Bool(true).len(), 1);
        assert_eq!(RuntimeValue::Array(vec![RuntimeValue::None]).len(), 1);
    }
}
