use std::{
    cell::RefCell,
    cmp::Ordering,
    fmt::{self, Debug, Display, Formatter},
    rc::Rc,
};

use crate::{AstIdent, AstParams, Program, Value, number::Number};

use mq_md::Node;

use super::env::Env;

#[derive(Clone, PartialEq)]
pub enum RuntimeValue {
    Number(Number),
    Bool(bool),
    String(String),
    Array(Vec<RuntimeValue>),
    Markdown(Node),
    Function(AstParams, Program, Rc<RefCell<Env>>),
    NativeFunction(AstIdent),
    None,
}

impl From<Node> for RuntimeValue {
    fn from(node: Node) -> Self {
        RuntimeValue::Markdown(node)
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
            Value::Markdown(m) => RuntimeValue::Markdown(m),
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
            (RuntimeValue::Markdown(a), RuntimeValue::Markdown(b)) => {
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
            RuntimeValue::Markdown(m) => m.to_string(),
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

    pub fn name(&self) -> &str {
        match self {
            RuntimeValue::Number(_) => "number",
            RuntimeValue::Bool(_) => "bool",
            RuntimeValue::String(_) => "string",
            RuntimeValue::Markdown(_) => "markdown",
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
            RuntimeValue::Markdown(m) => m.value(),
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
            RuntimeValue::Markdown(_) => true,
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
            RuntimeValue::Markdown(m) => m.value().len(),
            _ => panic!("not supported"),
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
            RuntimeValue::Markdown(m) => m.to_string(),
            RuntimeValue::None => "None".to_string(),
            RuntimeValue::Function(_, _, _) => "function".to_string(),
            RuntimeValue::NativeFunction(_) => "native_function".to_string(),
        }
    }
}
