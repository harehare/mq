use super::env::Env;
use crate::{AstParams, Ident, Program, Shared, SharedCell, ast, number::Number};
use mq_markdown::Node;
use smol_str::SmolStr;
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::BTreeMap,
    ops::{Index, IndexMut},
};

/// Runtime selector for indexing into markdown nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    /// Selects a child node at the specified index.
    Index(usize),
}

/// Represents a module's runtime environment with its exports.
#[derive(Clone, Debug)]
pub struct ModuleEnv {
    name: SmolStr,
    exports: Shared<SharedCell<Env>>,
}

impl ModuleEnv {
    /// Creates a new module environment with the given name and exports.
    pub fn new(name: &str, exports: Shared<SharedCell<Env>>) -> Self {
        Self {
            name: SmolStr::new(name),
            exports,
        }
    }

    /// Returns the name of the module.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns a reference to the module's exports environment.
    pub fn exports(&self) -> &Shared<SharedCell<Env>> {
        &self.exports
    }

    /// Returns the number of exports in this module.
    pub fn len(&self) -> usize {
        #[cfg(not(feature = "sync"))]
        {
            self.exports.borrow().len()
        }

        #[cfg(feature = "sync")]
        {
            self.exports.read().unwrap().len()
        }
    }
}

impl PartialEq for ModuleEnv {
    fn eq(&self, other: &Self) -> bool {
        #[cfg(not(feature = "sync"))]
        let exports = self.exports().borrow();
        #[cfg(feature = "sync")]
        let exports = self.exports().read().unwrap();

        #[cfg(not(feature = "sync"))]
        let other_exports = other.exports().borrow();
        #[cfg(feature = "sync")]
        let other_exports = other.exports().read().unwrap();

        self.name == other.name && std::ptr::eq(&*exports, &*other_exports)
    }
}

/// A value in the mq runtime.
///
/// This enum represents all possible value types that can exist during
/// program execution, including numbers, strings, markdown nodes, functions,
/// and more complex data structures.
#[derive(Clone, Default)]
pub enum RuntimeValue {
    /// A numeric value.
    Number(Number),
    /// A boolean value (`true` or `false`).
    Boolean(bool),
    /// A string value.
    String(String),
    /// A symbol (interned identifier).
    Symbol(Ident),
    /// An array of runtime values.
    ///
    /// Behind [`Shared`] for clone-on-write: cloning is an O(1) refcount bump; mutating
    /// builtins must go through [`array_mut`] instead of mutating directly.
    Array(Shared<Vec<RuntimeValue>>),
    /// A markdown node with an optional selector for indexing.
    Markdown(Box<Node>, Option<Selector>),
    /// A user-defined function with parameters, body (program), and captured environment.
    Function(Box<AstParams>, Program, Shared<SharedCell<Env>>),
    /// A built-in native function identified by name.
    NativeFunction(Ident),
    /// A dictionary mapping identifiers to runtime values.
    ///
    /// Same clone-on-write scheme as [`RuntimeValue::Array`]; see [`dict_mut`].
    Dict(Shared<BTreeMap<Ident, RuntimeValue>>),
    /// A module with its exports.
    Module(ModuleEnv),
    /// An AST node (quoted expression).
    Ast(Shared<ast::node::Node>),
    /// Raw binary data (e.g. CBOR byte strings).
    Bytes(Vec<u8>),
    /// An empty or null value.
    #[default]
    None,
}

// Custom PartialEq implementation to avoid comparing Env pointers
impl PartialEq for RuntimeValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a == b,
            (RuntimeValue::Boolean(a), RuntimeValue::Boolean(b)) => a == b,
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
            (RuntimeValue::Symbol(a), RuntimeValue::Symbol(b)) => a == b,
            (RuntimeValue::Array(a), RuntimeValue::Array(b)) => a == b,
            (RuntimeValue::Markdown(a, sa), RuntimeValue::Markdown(b, sb)) => a == b && sa == sb,
            (RuntimeValue::Function(a1, b1, _), RuntimeValue::Function(a2, b2, _)) => a1 == a2 && b1 == b2,
            (RuntimeValue::NativeFunction(a), RuntimeValue::NativeFunction(b)) => a == b,
            (RuntimeValue::Dict(a), RuntimeValue::Dict(b)) => a == b,
            (RuntimeValue::Module(a), RuntimeValue::Module(b)) => a == b,
            (RuntimeValue::Ast(a), RuntimeValue::Ast(b)) => a == b,
            (RuntimeValue::Bytes(a), RuntimeValue::Bytes(b)) => a == b,
            (RuntimeValue::None, RuntimeValue::None) => true,
            _ => false,
        }
    }
}

impl From<Node> for RuntimeValue {
    fn from(node: Node) -> Self {
        RuntimeValue::new_markdown(node)
    }
}

impl From<bool> for RuntimeValue {
    fn from(b: bool) -> Self {
        RuntimeValue::Boolean(b)
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
        RuntimeValue::Array(Shared::new(arr))
    }
}

impl From<BTreeMap<Ident, RuntimeValue>> for RuntimeValue {
    fn from(map: BTreeMap<Ident, RuntimeValue>) -> Self {
        RuntimeValue::Dict(Shared::new(map))
    }
}

impl From<Vec<(String, Number)>> for RuntimeValue {
    fn from(v: Vec<(String, Number)>) -> Self {
        RuntimeValue::Dict(Shared::new(
            v.into_iter()
                .map(|(k, v)| (Ident::new(&k), RuntimeValue::Number(v)))
                .collect::<BTreeMap<Ident, RuntimeValue>>(),
        ))
    }
}

impl From<mq_markdown::AttrValue> for RuntimeValue {
    fn from(attr_value: mq_markdown::AttrValue) -> Self {
        match attr_value {
            mq_markdown::AttrValue::String(s) => RuntimeValue::String(s),
            mq_markdown::AttrValue::Number(n) => RuntimeValue::Number(n.into()),
            mq_markdown::AttrValue::Integer(n) => RuntimeValue::Number(n.into()),
            mq_markdown::AttrValue::Boolean(b) => RuntimeValue::Boolean(b),
            mq_markdown::AttrValue::Array(arr) => {
                RuntimeValue::Array(Shared::new(arr.into_iter().map(RuntimeValue::from).collect()))
            }
            mq_markdown::AttrValue::Null => RuntimeValue::NONE,
        }
    }
}

impl From<yaml_rust2::Yaml> for RuntimeValue {
    fn from(value: yaml_rust2::Yaml) -> Self {
        match value {
            yaml_rust2::Yaml::Null | yaml_rust2::Yaml::BadValue => RuntimeValue::NONE,
            yaml_rust2::Yaml::Boolean(b) => RuntimeValue::Boolean(b),
            yaml_rust2::Yaml::Integer(i) => RuntimeValue::Number((i as f64).into()),
            yaml_rust2::Yaml::Real(s) => s
                .parse::<f64>()
                .map(|f| RuntimeValue::Number(f.into()))
                .unwrap_or(RuntimeValue::NONE),
            yaml_rust2::Yaml::String(s) => RuntimeValue::String(s),
            yaml_rust2::Yaml::Array(arr) => {
                RuntimeValue::Array(Shared::new(arr.into_iter().map(RuntimeValue::from).collect()))
            }
            yaml_rust2::Yaml::Hash(map) => {
                let mut btree = BTreeMap::new();
                for (k, v) in map {
                    let key = match k {
                        yaml_rust2::Yaml::String(s) => s,
                        other => format!("{other:?}"),
                    };
                    btree.insert(Ident::new(&key), RuntimeValue::from(v));
                }
                RuntimeValue::Dict(Shared::new(btree))
            }
            yaml_rust2::Yaml::Alias(_) => RuntimeValue::NONE,
        }
    }
}

impl From<serde_json::Value> for RuntimeValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => RuntimeValue::NONE,
            serde_json::Value::Bool(b) => RuntimeValue::Boolean(b),
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    RuntimeValue::Number(f.into())
                } else {
                    RuntimeValue::Number(0.into())
                }
            }
            serde_json::Value::String(s) => RuntimeValue::String(s),
            serde_json::Value::Array(arr) => {
                RuntimeValue::Array(Shared::new(arr.into_iter().map(RuntimeValue::from).collect()))
            }
            serde_json::Value::Object(obj) => {
                let mut map = BTreeMap::new();
                for (k, v) in obj {
                    map.insert(Ident::new(&k), RuntimeValue::from(v));
                }
                RuntimeValue::Dict(Shared::new(map))
            }
        }
    }
}

impl From<ciborium::Value> for RuntimeValue {
    fn from(value: ciborium::Value) -> Self {
        match value {
            ciborium::Value::Null => RuntimeValue::NONE,
            ciborium::Value::Bool(b) => RuntimeValue::Boolean(b),
            ciborium::Value::Integer(i) => {
                let n: i128 = i.into();
                RuntimeValue::Number(crate::number::Number::from(n as f64))
            }
            ciborium::Value::Float(f) => RuntimeValue::Number(crate::number::Number::from(f)),
            ciborium::Value::Text(s) => RuntimeValue::String(s),
            ciborium::Value::Bytes(b) => RuntimeValue::Bytes(b),
            ciborium::Value::Array(arr) => {
                let items = arr.into_iter().map(Into::into).collect();
                RuntimeValue::Array(Shared::new(items))
            }
            ciborium::Value::Map(pairs) => {
                let mut map = BTreeMap::new();
                for (k, v) in pairs {
                    let key = match k {
                        ciborium::Value::Text(s) => Ident::new(&s),
                        other => Ident::new(&format!("{:?}", other)),
                    };
                    map.insert(key, v.into());
                }
                RuntimeValue::Dict(Shared::new(map))
            }
            ciborium::Value::Tag(_, inner) => (*inner).into(),
            _ => RuntimeValue::NONE,
        }
    }
}

impl PartialOrd for RuntimeValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a.partial_cmp(b),
            (RuntimeValue::Boolean(a), RuntimeValue::Boolean(b)) => a.partial_cmp(b),
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a.partial_cmp(b),
            (RuntimeValue::Symbol(a), RuntimeValue::Symbol(b)) => a.partial_cmp(b),
            (RuntimeValue::Array(a), RuntimeValue::Array(b)) => a.partial_cmp(b),
            (RuntimeValue::Markdown(a, _), RuntimeValue::Markdown(b, _)) => {
                let a = a.to_string();
                let b = b.to_string();
                a.to_string().partial_cmp(&b)
            }
            (RuntimeValue::Function(a1, b1, _), RuntimeValue::Function(a2, b2, _)) => match a1.partial_cmp(a2) {
                Some(Ordering::Equal) => b1.partial_cmp(b2),
                Some(Ordering::Greater) => Some(Ordering::Greater),
                Some(Ordering::Less) => Some(Ordering::Less),
                _ => None,
            },
            (RuntimeValue::Bytes(a), RuntimeValue::Bytes(b)) => a.partial_cmp(b),
            (RuntimeValue::Dict(_), _) => None,
            (_, RuntimeValue::Dict(_)) => None,
            (RuntimeValue::Module(a), RuntimeValue::Module(b)) => a.name.partial_cmp(&b.name),
            (RuntimeValue::Ast(_), _) => None,
            (_, RuntimeValue::Ast(_)) => None,
            _ => None,
        }
    }
}

impl std::fmt::Display for RuntimeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let value: Cow<'_, str> = match self {
            Self::Number(n) => Cow::Owned(n.to_string()),
            Self::Boolean(b) => Cow::Owned(b.to_string()),
            Self::String(s) => Cow::Borrowed(s),
            Self::Symbol(i) => Cow::Owned(format!(":{}", i)),
            Self::Array(_) => self.string(),
            Self::Markdown(m, ..) => Cow::Owned(m.to_string()),
            Self::None => Cow::Borrowed(""),
            Self::Function(params, ..) => Cow::Owned(format!("function/{}", params.len())),
            Self::NativeFunction(_) => Cow::Borrowed("native_function"),
            Self::Dict(_) => self.string(),
            Self::Module(module_name) => Cow::Owned(format!(r#"module "{}""#, module_name.name)),
            Self::Ast(node) => Cow::Owned(node.to_code()),
            Self::Bytes(b) => Cow::Owned(bytes_to_hex(b)),
        };
        write!(f, "{}", value)
    }
}

impl std::fmt::Debug for RuntimeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let v: Cow<'_, str> = match self {
            Self::None => Cow::Borrowed("None"),
            Self::String(s) => Cow::Owned(format!("{:?}", s)),
            Self::Array(arr) => Cow::Owned(format!("{:?}", arr)),
            Self::Bytes(b) => Cow::Owned(format!("bytes({})", bytes_to_hex(b))),
            a => a.string(),
        };
        write!(f, "{}", v)
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    })
}

/// Clone-on-write access to an array's elements.
///
/// Bare `Shared::make_mut` is ambiguous (it also matches the `Rc<[T]>`/`Arc<[T]>` slice
/// specialization), so this pins the type down.
#[inline(always)]
pub(crate) fn array_mut(array: &mut Shared<Vec<RuntimeValue>>) -> &mut Vec<RuntimeValue> {
    Shared::<Vec<RuntimeValue>>::make_mut(array)
}

/// Clone-on-write access to a dict's entries; see [`array_mut`].
#[inline(always)]
pub(crate) fn dict_mut(map: &mut Shared<BTreeMap<Ident, RuntimeValue>>) -> &mut BTreeMap<Ident, RuntimeValue> {
    Shared::<BTreeMap<Ident, RuntimeValue>>::make_mut(map)
}

impl RuntimeValue {
    /// The boolean `false` value.
    pub const FALSE: RuntimeValue = Self::Boolean(false);
    /// The `None` (null) value.
    pub const NONE: RuntimeValue = Self::None;
    /// The boolean `true` value.
    pub const TRUE: RuntimeValue = Self::Boolean(true);

    /// Returns a new empty array.
    ///
    /// Not a `const` because `Shared::new` (`Rc`/`Arc::new`) isn't const-evaluable.
    #[inline(always)]
    pub fn empty_array() -> RuntimeValue {
        RuntimeValue::Array(Shared::new(Vec::new()))
    }

    /// Creates a new empty dictionary.
    #[inline(always)]
    pub fn new_dict() -> RuntimeValue {
        RuntimeValue::Dict(Shared::new(BTreeMap::new()))
    }

    /// Creates a new markdown runtime value from the given node.
    pub fn new_markdown(node: Node) -> RuntimeValue {
        RuntimeValue::Markdown(Box::new(node), None)
    }

    /// Returns the type name of this runtime value as a string.
    #[inline(always)]
    pub fn name(&self) -> &str {
        match self {
            RuntimeValue::Number(_) => "number",
            RuntimeValue::Boolean(_) => "bool",
            RuntimeValue::String(_) => "string",
            RuntimeValue::Symbol(_) => "symbol",
            RuntimeValue::Markdown(_, _) => "markdown",
            RuntimeValue::Array(_) => "array",
            RuntimeValue::None => "None",
            RuntimeValue::Function(_, _, _) => "function",
            RuntimeValue::NativeFunction(_) => "native_function",
            RuntimeValue::Dict(_) => "dict",
            RuntimeValue::Module(_) => "module",
            RuntimeValue::Ast(_) => "ast",
            RuntimeValue::Bytes(_) => "bytes",
        }
    }

    /// Returns `true` if this value is `None`.
    #[inline(always)]
    pub fn is_none(&self) -> bool {
        matches!(self, RuntimeValue::None)
    }

    /// Returns `true` if this value is a user-defined function.
    #[inline(always)]
    pub fn is_function(&self) -> bool {
        matches!(self, RuntimeValue::Function(_, _, _))
    }

    /// Returns `true` if this value is a native (built-in) function.
    #[inline(always)]
    pub fn is_native_function(&self) -> bool {
        matches!(self, RuntimeValue::NativeFunction(_))
    }

    /// Returns `true` if this value is an array.
    #[inline(always)]
    pub fn is_array(&self) -> bool {
        matches!(self, RuntimeValue::Array(_))
    }

    /// Returns `true` if this value is a dict.
    #[inline(always)]
    pub fn is_dict(&self) -> bool {
        matches!(self, RuntimeValue::Dict(_))
    }

    /// Returns `true` if this value is considered empty.
    ///
    /// Empty values include empty arrays, empty strings, empty markdown nodes,
    /// empty dictionaries, and `None`.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        match self {
            RuntimeValue::Array(a) => a.is_empty(),
            RuntimeValue::String(s) => s.is_empty(),
            RuntimeValue::Markdown(m, _) => m.value().is_empty(),
            RuntimeValue::Dict(m) => m.is_empty(),
            RuntimeValue::Bytes(b) => b.is_empty(),
            RuntimeValue::None => true,
            _ => false,
        }
    }

    /// Returns `true` if this value is considered truthy in conditional contexts.
    ///
    /// Truthy values include non-zero numbers, non-empty strings and arrays,
    /// `true`, functions, symbols, and modules. Falsy values include `false`,
    /// zero, empty collections, and `None`.
    #[inline(always)]
    pub fn is_truthy(&self) -> bool {
        match self {
            RuntimeValue::Boolean(b) => *b,
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
            RuntimeValue::Module(_) => true,
            RuntimeValue::Ast(_) => true,
            RuntimeValue::Bytes(b) => !b.is_empty(),
            RuntimeValue::None => false,
        }
    }

    /// Returns the length of this value.
    ///
    /// For numbers, returns the value as `usize`. For strings and arrays, returns
    /// the number of elements. For dictionaries, returns the number of entries.
    #[inline(always)]
    pub fn len(&self) -> usize {
        match self {
            RuntimeValue::Number(n) => n.value() as usize,
            RuntimeValue::Boolean(_) => 1,
            RuntimeValue::String(s) => s.len(),
            RuntimeValue::Symbol(i) => i.as_str().len(),
            RuntimeValue::Array(a) => a.len(),
            RuntimeValue::Markdown(m, _) => m.value().len(),
            RuntimeValue::Dict(m) => m.len(),
            RuntimeValue::Bytes(b) => b.len(),
            RuntimeValue::None => 0,
            RuntimeValue::Function(..) => 0,
            RuntimeValue::Module(m) => m.len(),
            RuntimeValue::NativeFunction(..) => 0,
            RuntimeValue::Ast(_) => 0,
        }
    }

    /// Extracts the markdown node from this value, if it is a markdown value.
    ///
    /// If a selector is present, returns the selected child node.
    #[inline(always)]
    pub fn markdown_node(&self) -> Option<Node> {
        match self {
            RuntimeValue::Markdown(n, Some(Selector::Index(i))) => n.find_at_index(*i),
            RuntimeValue::Markdown(n, _) => Some((**n).clone()),
            _ => None,
        }
    }

    /// Updates the value of a markdown node, returning a new runtime value.
    ///
    /// If this is not a markdown value, returns `None`.
    #[inline(always)]
    pub fn update_markdown_value(&self, value: &str) -> RuntimeValue {
        match self {
            RuntimeValue::Markdown(n, Some(Selector::Index(i))) => {
                RuntimeValue::Markdown(Box::new(n.with_children_value(value, *i)), Some(Selector::Index(*i)))
            }
            RuntimeValue::Markdown(n, selector) => {
                RuntimeValue::Markdown(Box::new(n.with_value(value)), selector.clone())
            }
            _ => RuntimeValue::NONE,
        }
    }

    /// Returns the position information for a markdown node, if available.
    #[inline(always)]
    pub fn position(&self) -> Option<mq_markdown::Position> {
        match self {
            RuntimeValue::Markdown(node, _) => node.position(),
            _ => None,
        }
    }

    /// Sets the position information for a markdown node.
    ///
    /// Only affects markdown values; other value types are unaffected.
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
            Self::Boolean(b) => Cow::Owned(b.to_string()),
            Self::String(s) => Cow::Owned(format!(r#""{}""#, s)),
            Self::Symbol(i) => Cow::Owned(format!(":{}", i)),
            Self::Array(a) => Cow::Owned(format!(
                "[{}]",
                a.iter().map(|v| v.string()).collect::<Vec<Cow<str>>>().join(", ")
            )),
            Self::Markdown(m, ..) => Cow::Owned(m.to_string()),
            Self::None => Cow::Borrowed(""),
            Self::Function(f, _, _) => Cow::Owned(format!("function/{}", f.len())),
            Self::NativeFunction(_) => Cow::Borrowed("native_function"),
            Self::Module(m) => Cow::Owned(format!("module/{}", m.name())),
            Self::Ast(node) => Cow::Owned(node.to_code()),
            Self::Bytes(b) => Cow::Owned(bytes_to_hex(b)),
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

    /// Returns a new runtime value that is the logical negation of this value.
    pub fn negated(&self) -> Self {
        match self {
            RuntimeValue::Boolean(b) => RuntimeValue::Boolean(!b),
            RuntimeValue::Number(n) => RuntimeValue::Number((-n.value()).into()),
            RuntimeValue::String(s) => RuntimeValue::String(s.chars().rev().collect()),
            _ => self.clone(),
        }
    }

    pub fn to_json_value(self) -> serde_json::Value {
        use base64::Engine;
        match self {
            RuntimeValue::None => serde_json::Value::Null,
            RuntimeValue::Boolean(b) => serde_json::Value::Bool(b),
            RuntimeValue::Number(n) => serde_json::Number::from_f64(n.value())
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            RuntimeValue::String(s) => serde_json::Value::String(s),
            RuntimeValue::Symbol(i) => serde_json::Value::String(i.to_string()),
            RuntimeValue::Array(arr) => serde_json::Value::Array(
                Shared::unwrap_or_clone(arr)
                    .into_iter()
                    .map(Self::to_json_value)
                    .collect(),
            ),
            RuntimeValue::Dict(map) => {
                let obj: serde_json::Map<String, serde_json::Value> = Shared::unwrap_or_clone(map)
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_json_value()))
                    .collect();
                serde_json::Value::Object(obj)
            }
            RuntimeValue::Bytes(b) => serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(b)),
            RuntimeValue::Markdown(node, _) => serde_json::to_value(node.as_ref()).unwrap_or(serde_json::Value::Null),
            _ => serde_json::Value::Null,
        }
    }

    pub fn to_cbor_value(self) -> ciborium::Value {
        match self {
            RuntimeValue::None => ciborium::Value::Null,
            RuntimeValue::Boolean(b) => ciborium::Value::Bool(b),
            RuntimeValue::Number(n) => ciborium::Value::Float(n.value()),
            RuntimeValue::String(s) => ciborium::Value::Text(s),
            RuntimeValue::Symbol(i) => ciborium::Value::Text(i.to_string()),
            RuntimeValue::Bytes(b) => ciborium::Value::Bytes(b),
            RuntimeValue::Array(arr) => ciborium::Value::Array(
                Shared::unwrap_or_clone(arr)
                    .into_iter()
                    .map(Self::to_cbor_value)
                    .collect(),
            ),
            RuntimeValue::Dict(map) => ciborium::Value::Map(
                Shared::unwrap_or_clone(map)
                    .into_iter()
                    .map(|(k, v)| (ciborium::Value::Text(k.to_string()), v.to_cbor_value()))
                    .collect(),
            ),
            _ => ciborium::Value::Null,
        }
    }
}

/// A collection of runtime values.
///
/// Provides utilities for working with multiple values, such as filtering
/// and updating operations.
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
    type IntoIter = std::vec::IntoIter<RuntimeValue>;
    type Item = RuntimeValue;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl RuntimeValues {
    /// Returns a compacted version of this collection, removing `None` and empty values.
    pub fn compact(&self) -> Vec<RuntimeValue> {
        self.0
            .iter()
            .filter(|v| !v.is_none() && !v.is_empty())
            .cloned()
            .collect::<Vec<_>>()
    }

    /// Returns a reference to the underlying vector of values.
    pub fn values(&self) -> &Vec<RuntimeValue> {
        &self.0
    }

    /// Returns the number of values in this collection.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if this collection contains no values.
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    /// Updates this collection with values from another collection.
    ///
    /// Pairs corresponding elements from both collections and applies special
    /// update logic for markdown nodes.
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
                        | RuntimeValue::Module(_)
                        | RuntimeValue::Ast(_)
                        | RuntimeValue::NativeFunction(_) => current_value.clone(),
                        RuntimeValue::Markdown(node, _) if node.is_empty() => current_value.clone(),
                        RuntimeValue::Markdown(node, _) => {
                            if node.is_fragment() {
                                if let RuntimeValue::Markdown(mut current_node, selector) = current_value {
                                    current_node.apply_fragment((**node).clone());
                                    RuntimeValue::Markdown(current_node, selector)
                                } else {
                                    updated_value
                                }
                            } else {
                                updated_value
                            }
                        }
                        RuntimeValue::String(s) => RuntimeValue::new_markdown(node.with_value(s)),
                        RuntimeValue::Symbol(i) => RuntimeValue::new_markdown(node.with_value(&i.as_str())),
                        RuntimeValue::Boolean(b) => RuntimeValue::new_markdown(node.with_value(b.to_string().as_str())),
                        RuntimeValue::Number(n) => RuntimeValue::new_markdown(node.with_value(n.to_string().as_str())),
                        RuntimeValue::Array(array) => RuntimeValue::Array(Shared::new(
                            array
                                .iter()
                                .filter_map(|o| {
                                    if o.is_none() {
                                        None
                                    } else {
                                        Some(RuntimeValue::Markdown(
                                            Box::new(node.with_value(o.to_string().as_str())),
                                            None,
                                        ))
                                    }
                                })
                                .collect::<Vec<_>>(),
                        )),
                        RuntimeValue::Bytes(b) => RuntimeValue::new_markdown(node.with_value(bytes_to_hex(b).as_str())),
                        RuntimeValue::Dict(map) => {
                            let mut new_dict = BTreeMap::new();
                            for (k, v) in map.iter() {
                                if !v.is_none() && !v.is_empty() {
                                    new_dict.insert(
                                        *k,
                                        RuntimeValue::new_markdown(node.with_value(v.to_string().as_str())),
                                    );
                                }
                            }
                            RuntimeValue::Dict(Shared::new(new_dict))
                        }
                    }
                } else {
                    updated_value
                }
            })
            .collect::<Vec<_>>()
            .into()
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::node::{IdentWithToken, Param};
    use rstest::rstest;
    use smallvec::{SmallVec, smallvec};

    use super::*;

    #[test]
    fn test_runtime_value_from() {
        assert_eq!(RuntimeValue::from(true), RuntimeValue::Boolean(true));
        assert_eq!(RuntimeValue::from(false), RuntimeValue::Boolean(false));
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
    #[case(RuntimeValue::Boolean(true), "true")]
    #[case(RuntimeValue::Boolean(false), "false")]
    #[case(RuntimeValue::String("hello".to_string()), r#""hello""#)]
    #[case(RuntimeValue::None, "")]
    #[case(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(Number::from(1.0)),
            RuntimeValue::String("test".to_string())
        ])), r#"[1, "test"]"#)]
    #[case(RuntimeValue::Dict({
            let mut map = BTreeMap::new();
            map.insert(Ident::new("key1"), RuntimeValue::String("value1".to_string()));
            map.insert(Ident::new("key2"), RuntimeValue::Number(Number::from(42.0)));
            Shared::new(map)
        }), r#"{"key1": "value1", "key2": 42}"#)]
    fn test_string_method(#[case] value: RuntimeValue, #[case] expected: &str) {
        assert_eq!(value.string(), expected);
    }

    #[test]
    fn test_runtime_value_display() {
        assert_eq!(format!("{}", RuntimeValue::Boolean(true)), "true");
        assert_eq!(format!("{}", RuntimeValue::Number(Number::from(42.0))), "42");
        assert_eq!(format!("{}", RuntimeValue::String(String::from("test"))), "test");
        assert_eq!(format!("{}", RuntimeValue::None), "");
        let map_val = RuntimeValue::Dict(Shared::new(BTreeMap::default()));
        assert_eq!(format!("{}", map_val), "{}");
    }

    #[test]
    fn test_runtime_value_debug() {
        assert_eq!(format!("{:?}", RuntimeValue::Boolean(true)), "true");
        assert_eq!(format!("{:?}", RuntimeValue::Number(Number::from(42.0))), "42");
        assert_eq!(format!("{:?}", RuntimeValue::String(String::from("test"))), "\"test\"");
        assert_eq!(format!("{:?}", RuntimeValue::None), "None");

        let mut map = BTreeMap::default();
        map.insert(Ident::new("name"), RuntimeValue::String("MQ".to_string()));
        map.insert(Ident::new("version"), RuntimeValue::Number(Number::from(1.0)));
        let map_val = RuntimeValue::Dict(Shared::new(map));
        let debug_str = format!("{:?}", map_val);
        assert!(debug_str == r#"{"name": "MQ", "version": 1}"# || debug_str == r#"{"version": 1, "name": "MQ"}"#);
    }

    #[test]
    fn test_runtime_value_name() {
        assert_eq!(RuntimeValue::Boolean(true).name(), "bool");
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).name(), "number");
        assert_eq!(RuntimeValue::String(String::from("test")).name(), "string");
        assert_eq!(RuntimeValue::None.name(), "None");
        assert_eq!(
            RuntimeValue::Function(
                Box::new(SmallVec::new()),
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
                Box::new(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "".to_string(),
                    position: None
                })),
                None
            )
            .name(),
            "markdown"
        );
        assert_eq!(RuntimeValue::Dict(Shared::new(BTreeMap::default())).name(), "dict");
    }

    #[test]
    fn test_runtime_value_is_true() {
        assert!(RuntimeValue::Boolean(true).is_truthy());
        assert!(!RuntimeValue::Boolean(false).is_truthy());
        assert!(RuntimeValue::Number(Number::from(42.0)).is_truthy());
        assert!(!RuntimeValue::Number(Number::from(0.0)).is_truthy());
        assert!(RuntimeValue::String(String::from("test")).is_truthy());
        assert!(!RuntimeValue::String(String::from("")).is_truthy());
        assert!(RuntimeValue::Array(Shared::new(vec!["".to_string().into()])).is_truthy());
        assert!(!RuntimeValue::Array(Shared::new(Vec::new())).is_truthy());
        assert!(
            RuntimeValue::Markdown(
                Box::new(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "".to_string(),
                    position: None
                })),
                None
            )
            .is_truthy()
        );
        assert!(
            !RuntimeValue::Markdown(
                Box::new(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "".to_string(),
                    position: None
                })),
                Some(Selector::Index(1))
            )
            .is_truthy()
        );
        assert!(!RuntimeValue::Array(Shared::new(Vec::new())).is_truthy());
        assert!(!RuntimeValue::None.is_truthy());
        assert!(RuntimeValue::NativeFunction(Ident::new("name")).is_truthy());
        assert!(
            RuntimeValue::Function(
                Box::new(SmallVec::new()),
                Vec::new(),
                Shared::new(SharedCell::new(Env::default()))
            )
            .is_truthy()
        );
        assert!(RuntimeValue::Dict(Shared::new(BTreeMap::default())).is_truthy());
    }

    #[test]
    fn test_runtime_value_partial_ord() {
        assert!(RuntimeValue::Number(Number::from(1.0)) < RuntimeValue::Number(Number::from(2.0)));
        assert!(RuntimeValue::String(String::from("a")) < RuntimeValue::String(String::from("b")));
        assert!(
            RuntimeValue::Array(Shared::new(Vec::new()))
                < RuntimeValue::Array(Shared::new(vec!["a".to_string().into()]))
        );
        assert!(
            RuntimeValue::Markdown(
                Box::new(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "test".to_string(),
                    position: None
                })),
                None
            ) < RuntimeValue::Markdown(
                Box::new(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "test2".to_string(),
                    position: None
                })),
                None
            )
        );
        assert!(RuntimeValue::Boolean(false) < RuntimeValue::Boolean(true));
        assert!(
            RuntimeValue::Function(
                Box::new(SmallVec::new()),
                Vec::new(),
                Shared::new(SharedCell::new(Env::default()))
            ) < RuntimeValue::Function(
                Box::new(smallvec![Param::new(IdentWithToken::new("test"))]),
                Vec::new(),
                Shared::new(SharedCell::new(Env::default()))
            )
        );
    }

    #[test]
    fn test_runtime_value_len() {
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).len(), 42);
        assert_eq!(RuntimeValue::String(String::from("test")).len(), 4);
        assert_eq!(RuntimeValue::Boolean(true).len(), 1);
        assert_eq!(RuntimeValue::Array(Shared::new(vec![RuntimeValue::None])).len(), 1);
        assert_eq!(
            RuntimeValue::Markdown(
                Box::new(mq_markdown::Node::Text(mq_markdown::Text {
                    value: "a".to_string(),
                    position: None
                })),
                None
            )
            .len(),
            1
        );
        let mut map = BTreeMap::default();
        map.insert(Ident::new("a"), RuntimeValue::String("alpha".to_string()));
        map.insert(Ident::new("b"), RuntimeValue::String("beta".to_string()));
        assert_eq!(RuntimeValue::Dict(Shared::new(map)).len(), 2);
    }

    #[test]
    fn test_negated() {
        assert_eq!(
            RuntimeValue::Number(Number::from(42.0)).negated(),
            RuntimeValue::Number(Number::from(-42.0))
        );
        assert_eq!(RuntimeValue::Boolean(true).negated(), RuntimeValue::Boolean(false));
        assert_eq!(RuntimeValue::Boolean(false).negated(), RuntimeValue::Boolean(true));
    }

    #[test]
    fn test_runtime_value_debug_output() {
        let array = RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(Number::from(1.0)),
            RuntimeValue::String("hello".to_string()),
        ]));
        assert_eq!(format!("{:?}", array), r#"[1, "hello"]"#);

        let node = mq_markdown::Node::Text(mq_markdown::Text {
            value: "test markdown".to_string(),
            position: None,
        });
        let markdown = RuntimeValue::new_markdown(node);
        assert_eq!(format!("{:?}", markdown), "test markdown");

        let function = RuntimeValue::Function(
            Box::new(SmallVec::new()),
            Vec::new(),
            Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(format!("{:?}", function), "function/0");

        let native_fn = RuntimeValue::NativeFunction(Ident::new("debug"));
        assert_eq!(format!("{:?}", native_fn), "native_function");

        let mut map = BTreeMap::default();
        map.insert(Ident::new("a"), RuntimeValue::String("alpha".to_string()));
        let map_val = RuntimeValue::Dict(Shared::new(map));
        assert_eq!(format!("{:?}", map_val), r#"{"a": "alpha"}"#);
    }

    #[test]
    fn test_runtime_value_markdown() {
        let markdown = RuntimeValue::new_markdown("test markdown".to_string().into());
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

        let markdown_with_selector = RuntimeValue::Markdown(Box::new(parent.clone()), Some(Selector::Index(1)));

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
            RuntimeValue::Boolean(true).update_markdown_value("test"),
            RuntimeValue::NONE
        );
        assert_eq!(RuntimeValue::None.update_markdown_value("test"), RuntimeValue::NONE);
    }

    #[test]
    fn test_runtime_value_map_creation_and_equality() {
        let mut map1_data = BTreeMap::default();
        map1_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        map1_data.insert(Ident::new("b"), RuntimeValue::String("hello".to_string()));
        let map1 = RuntimeValue::Dict(Shared::new(map1_data));

        let mut map2_data = BTreeMap::default();
        map2_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        map2_data.insert(Ident::new("b"), RuntimeValue::String("hello".to_string()));
        let map2 = RuntimeValue::Dict(Shared::new(map2_data));

        let mut map3_data = BTreeMap::default();
        map3_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        map3_data.insert(Ident::new("c"), RuntimeValue::String("world".to_string()));
        let map3 = RuntimeValue::Dict(Shared::new(map3_data));

        assert_eq!(map1, map2);
        assert_ne!(map1, map3);
    }

    #[test]
    fn test_runtime_value_map_is_empty() {
        let empty_map = RuntimeValue::Dict(Shared::new(BTreeMap::default()));
        assert!(empty_map.is_empty());

        let mut map_data = BTreeMap::default();
        map_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        let non_empty_map = RuntimeValue::Dict(Shared::new(map_data));
        assert!(!non_empty_map.is_empty());
    }

    #[test]
    fn test_runtime_value_map_partial_ord() {
        let mut map1_data = BTreeMap::default();
        map1_data.insert(Ident::new("a"), RuntimeValue::Number(Number::from(1.0)));
        let map1 = RuntimeValue::Dict(Shared::new(map1_data));

        let mut map2_data = BTreeMap::default();
        map2_data.insert(Ident::new("b"), RuntimeValue::Number(Number::from(2.0)));
        let map2 = RuntimeValue::Dict(Shared::new(map2_data));

        assert_eq!(map1.partial_cmp(&map2), None);
        assert_eq!(map2.partial_cmp(&map1), None);
        assert_eq!(map1.partial_cmp(&map1), None);

        let num_val = RuntimeValue::Number(Number::from(5.0));
        assert_eq!(map1.partial_cmp(&num_val), None);
        assert_eq!(num_val.partial_cmp(&map1), None);
    }

    #[test]
    fn test_bytes_name() {
        assert_eq!(RuntimeValue::Bytes(vec![]).name(), "bytes");
        assert_eq!(RuntimeValue::Bytes(vec![1, 2, 3]).name(), "bytes");
    }

    #[test]
    fn test_bytes_is_empty() {
        assert!(RuntimeValue::Bytes(vec![]).is_empty());
        assert!(!RuntimeValue::Bytes(vec![0]).is_empty());
    }

    #[test]
    fn test_bytes_is_truthy() {
        assert!(!RuntimeValue::Bytes(vec![]).is_truthy());
        assert!(RuntimeValue::Bytes(vec![0]).is_truthy());
        assert!(RuntimeValue::Bytes(vec![1, 2, 3]).is_truthy());
    }

    #[test]
    fn test_bytes_len() {
        assert_eq!(RuntimeValue::Bytes(vec![]).len(), 0);
        assert_eq!(RuntimeValue::Bytes(vec![1, 2, 3]).len(), 3);
    }

    #[test]
    fn test_bytes_display() {
        assert_eq!(
            format!("{}", RuntimeValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])),
            "deadbeef"
        );
        assert_eq!(format!("{}", RuntimeValue::Bytes(vec![])), "");
    }

    #[test]
    fn test_bytes_debug() {
        assert_eq!(format!("{:?}", RuntimeValue::Bytes(vec![0xca, 0xfe])), "bytes(cafe)");
    }

    #[test]
    fn test_bytes_partial_eq() {
        assert_eq!(RuntimeValue::Bytes(vec![1, 2]), RuntimeValue::Bytes(vec![1, 2]));
        assert_ne!(RuntimeValue::Bytes(vec![1, 2]), RuntimeValue::Bytes(vec![1, 3]));
        assert_ne!(
            RuntimeValue::Bytes(vec![1, 2]),
            RuntimeValue::String("0102".to_string())
        );
    }

    #[test]
    fn test_bytes_partial_ord() {
        assert!(RuntimeValue::Bytes(vec![1]) < RuntimeValue::Bytes(vec![2]));
        assert!(RuntimeValue::Bytes(vec![1, 2]) > RuntimeValue::Bytes(vec![1]));
        assert_eq!(
            RuntimeValue::Bytes(vec![1]).partial_cmp(&RuntimeValue::Bytes(vec![1])),
            Some(std::cmp::Ordering::Equal)
        );
        assert_eq!(RuntimeValue::Bytes(vec![]).partial_cmp(&RuntimeValue::None), None);
    }

    #[rstest]
    #[case(RuntimeValue::None, serde_json::Value::Null)]
    #[case(RuntimeValue::Boolean(true), serde_json::Value::Bool(true))]
    #[case(RuntimeValue::Boolean(false), serde_json::Value::Bool(false))]
    #[case(RuntimeValue::String("hi".to_string()), serde_json::Value::String("hi".to_string()))]
    #[case(RuntimeValue::Symbol(Ident::new("sym")), serde_json::Value::String("sym".to_string()))]
    #[case(RuntimeValue::NativeFunction(Ident::new("f")), serde_json::Value::Null)]
    fn test_to_json_value_scalars(#[case] value: RuntimeValue, #[case] expected: serde_json::Value) {
        assert_eq!(value.to_json_value(), expected);
    }

    #[test]
    fn test_to_json_value_array() {
        let arr = RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Boolean(true),
            RuntimeValue::String("x".to_string()),
        ]));
        match arr.to_json_value() {
            serde_json::Value::Array(items) => {
                assert_eq!(items[0], serde_json::Value::Bool(true));
                assert_eq!(items[1], serde_json::Value::String("x".to_string()));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn test_to_json_value_dict() {
        let mut map = BTreeMap::new();
        map.insert(Ident::new("k"), RuntimeValue::Boolean(false));
        let obj = RuntimeValue::Dict(Shared::new(map)).to_json_value();
        match obj {
            serde_json::Value::Object(m) => {
                assert_eq!(m["k"], serde_json::Value::Bool(false));
            }
            other => panic!("expected Object, got {other:?}"),
        }
    }

    #[test]
    fn test_to_json_value_bytes_base64() {
        let b = RuntimeValue::Bytes(vec![0x00, 0xff]);
        match b.to_json_value() {
            serde_json::Value::String(s) => assert!(!s.is_empty()),
            other => panic!("expected String, got {other:?}"),
        }
    }

    #[test]
    fn test_to_json_value_markdown() {
        let node = Node::Text(mq_markdown::Text {
            value: "hi".to_string(),
            position: None,
        });
        let value = RuntimeValue::Markdown(Box::new(node), None).to_json_value();
        assert_eq!(value["type"], serde_json::Value::String("Text".to_string()));
        assert_eq!(value["value"], serde_json::Value::String("hi".to_string()));
    }

    #[test]
    fn test_to_json_value_array_of_markdown() {
        // Regression test: nested Markdown values inside an Array (e.g. the array
        // returned by `from_html()`) must serialize to their node structure, not `null`.
        let node = Node::Text(mq_markdown::Text {
            value: "hi".to_string(),
            position: None,
        });
        let arr = RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Box::new(node), None)]));
        match arr.to_json_value() {
            serde_json::Value::Array(items) => {
                assert_ne!(items[0], serde_json::Value::Null);
                assert_eq!(items[0]["type"], serde_json::Value::String("Text".to_string()));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[rstest]
    #[case(RuntimeValue::None, true)]
    #[case(RuntimeValue::Boolean(true), false)]
    #[case(RuntimeValue::String("".to_string()), true)]
    #[case(RuntimeValue::Array(Shared::new(vec![])), true)]
    #[case(RuntimeValue::Dict(Shared::new(BTreeMap::new())), true)]
    #[case(RuntimeValue::Bytes(vec![]), true)]
    #[case(RuntimeValue::Bytes(vec![1]), false)]
    fn test_is_empty(#[case] value: RuntimeValue, #[case] expected: bool) {
        assert_eq!(value.is_empty(), expected);
    }

    #[rstest]
    #[case(RuntimeValue::None, false)]
    #[case(RuntimeValue::Boolean(true), true)]
    #[case(RuntimeValue::Boolean(false), false)]
    #[case(RuntimeValue::String("hi".to_string()), true)]
    #[case(RuntimeValue::String("".to_string()), false)]
    #[case(RuntimeValue::Array(Shared::new(vec![RuntimeValue::None])), true)]
    #[case(RuntimeValue::Array(Shared::new(vec![])), false)]
    #[case(RuntimeValue::Symbol(Ident::new("s")), true)]
    #[case(RuntimeValue::NativeFunction(Ident::new("f")), true)]
    fn test_is_truthy_variants(#[case] value: RuntimeValue, #[case] expected: bool) {
        assert_eq!(value.is_truthy(), expected);
    }

    #[rstest]
    #[case(RuntimeValue::Symbol(Ident::new("abc")), 3)]
    #[case(RuntimeValue::NativeFunction(Ident::new("f")), 0)]
    #[case(RuntimeValue::Ast(crate::Shared::new(crate::AstNode { token_id: crate::arena::ArenaId::new(0), expr: crate::Shared::new(crate::AstExpr::Self_) })), 0)]
    fn test_len_less_common(#[case] value: RuntimeValue, #[case] expected: usize) {
        assert_eq!(value.len(), expected);
    }

    #[test]
    fn test_is_none_predicate() {
        assert!(RuntimeValue::None.is_none());
        assert!(!RuntimeValue::Boolean(false).is_none());
    }

    #[test]
    fn test_is_function_native() {
        assert!(RuntimeValue::NativeFunction(Ident::new("f")).is_native_function());
        assert!(!RuntimeValue::NativeFunction(Ident::new("f")).is_function());
        assert!(!RuntimeValue::None.is_native_function());
    }

    #[test]
    fn test_is_array_dict() {
        assert!(RuntimeValue::Array(Shared::new(vec![])).is_array());
        assert!(!RuntimeValue::None.is_array());
        assert!(RuntimeValue::Dict(Shared::new(BTreeMap::new())).is_dict());
        assert!(!RuntimeValue::None.is_dict());
    }

    #[test]
    fn test_new_dict_and_new_markdown() {
        assert!(RuntimeValue::new_dict().is_dict());
        let node = mq_markdown::Node::Empty;
        assert!(matches!(
            RuntimeValue::new_markdown(node),
            RuntimeValue::Markdown(_, None)
        ));
    }

    #[test]
    fn test_from_vec_runtime_value() {
        let arr: RuntimeValue = vec![RuntimeValue::None, RuntimeValue::Boolean(true)].into();
        assert!(arr.is_array());
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_from_btree_map() {
        let mut map = BTreeMap::new();
        map.insert(Ident::new("x"), RuntimeValue::Boolean(true));
        let dict: RuntimeValue = map.into();
        assert!(dict.is_dict());
    }

    #[test]
    fn test_from_usize() {
        let v: RuntimeValue = 42usize.into();
        assert!(matches!(v, RuntimeValue::Number(_)));
        assert_eq!(v.len(), 42);
    }

    #[test]
    fn test_markdown_node_with_no_selector() {
        let node = mq_markdown::Node::Empty;
        let v = RuntimeValue::Markdown(Box::new(node), None);
        assert!(v.markdown_node().is_some());
    }

    #[test]
    fn test_markdown_node_non_markdown_returns_none() {
        assert!(RuntimeValue::None.markdown_node().is_none());
        assert!(RuntimeValue::String("x".to_string()).markdown_node().is_none());
    }

    #[test]
    fn test_runtime_values_index() {
        let values: RuntimeValues =
            vec![RuntimeValue::Boolean(true), RuntimeValue::String("second".to_string())].into();
        assert_eq!(values[0], RuntimeValue::Boolean(true));
        assert_eq!(values[1], RuntimeValue::String("second".to_string()));
    }

    #[test]
    fn test_runtime_values_index_mut() {
        let mut values: RuntimeValues = vec![RuntimeValue::None, RuntimeValue::None].into();
        values[0] = RuntimeValue::Boolean(true);
        assert_eq!(values[0], RuntimeValue::Boolean(true));
    }

    #[test]
    fn test_runtime_values_is_empty() {
        let empty: RuntimeValues = vec![].into();
        assert!(empty.is_empty());
        let non_empty: RuntimeValues = vec![RuntimeValue::None].into();
        assert!(!non_empty.is_empty());
    }

    fn text_node(s: &str) -> mq_markdown::Node {
        mq_markdown::Node::Text(mq_markdown::Text {
            value: s.to_string(),
            position: None,
        })
    }

    fn md(s: &str) -> RuntimeValue {
        RuntimeValue::new_markdown(text_node(s))
    }

    #[test]
    fn test_negated_string_reverses() {
        let v = RuntimeValue::String("abc".to_string()).negated();
        assert_eq!(v, RuntimeValue::String("cba".to_string()));
    }

    #[test]
    fn test_negated_none_returns_self() {
        assert_eq!(RuntimeValue::None.negated(), RuntimeValue::None);
    }

    #[test]
    fn test_negated_array_returns_self() {
        let arr = RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into())]));
        assert_eq!(arr.clone().negated(), arr);
    }

    #[test]
    fn test_position_non_markdown_returns_none() {
        assert!(RuntimeValue::None.position().is_none());
        assert!(RuntimeValue::Number(1.into()).position().is_none());
        assert!(RuntimeValue::String("x".to_string()).position().is_none());
    }

    #[test]
    fn test_set_position_non_markdown_is_noop() {
        let mut v = RuntimeValue::Number(1.into());
        v.set_position(None); // should not panic
        assert_eq!(v, RuntimeValue::Number(1.into()));
    }

    #[test]
    fn test_to_cbor_value_scalars() {
        assert_eq!(RuntimeValue::None.to_cbor_value(), ciborium::Value::Null);
        assert_eq!(RuntimeValue::Boolean(true).to_cbor_value(), ciborium::Value::Bool(true));
        assert_eq!(
            RuntimeValue::Number(1.5.into()).to_cbor_value(),
            ciborium::Value::Float(1.5)
        );
        assert_eq!(
            RuntimeValue::String("hi".to_string()).to_cbor_value(),
            ciborium::Value::Text("hi".to_string())
        );
        assert_eq!(
            RuntimeValue::Symbol(Ident::new("s")).to_cbor_value(),
            ciborium::Value::Text("s".to_string())
        );
        assert_eq!(
            RuntimeValue::Bytes(vec![0x01, 0x02]).to_cbor_value(),
            ciborium::Value::Bytes(vec![0x01, 0x02])
        );
    }

    #[test]
    fn test_to_cbor_value_array() {
        let arr = RuntimeValue::Array(Shared::new(vec![RuntimeValue::Boolean(false)]));
        match arr.to_cbor_value() {
            ciborium::Value::Array(items) => {
                assert_eq!(items[0], ciborium::Value::Bool(false));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn test_to_cbor_value_dict() {
        let mut map = BTreeMap::new();
        map.insert(Ident::new("k"), RuntimeValue::Boolean(true));
        let obj = RuntimeValue::Dict(Shared::new(map)).to_cbor_value();
        match obj {
            ciborium::Value::Map(pairs) => {
                assert_eq!(pairs[0].0, ciborium::Value::Text("k".to_string()));
                assert_eq!(pairs[0].1, ciborium::Value::Bool(true));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[test]
    fn test_to_cbor_value_other_is_null() {
        let native = RuntimeValue::NativeFunction(Ident::new("f")).to_cbor_value();
        assert_eq!(native, ciborium::Value::Null);
    }

    #[test]
    fn test_from_yaml_scalars() {
        assert_eq!(RuntimeValue::from(yaml_rust2::Yaml::Null), RuntimeValue::NONE);
        assert_eq!(
            RuntimeValue::from(yaml_rust2::Yaml::Boolean(true)),
            RuntimeValue::Boolean(true)
        );
        assert_eq!(
            RuntimeValue::from(yaml_rust2::Yaml::Integer(42)),
            RuntimeValue::Number((42.0_f64).into())
        );
        assert_eq!(
            RuntimeValue::from(yaml_rust2::Yaml::String("hi".to_string())),
            RuntimeValue::String("hi".to_string())
        );
        assert_eq!(RuntimeValue::from(yaml_rust2::Yaml::BadValue), RuntimeValue::NONE);
    }

    #[test]
    fn test_from_yaml_real() {
        let v = RuntimeValue::from(yaml_rust2::Yaml::Real("3.14".to_string()));
        assert!(matches!(v, RuntimeValue::Number(_)));
    }

    #[test]
    fn test_from_yaml_array() {
        let yaml_arr = yaml_rust2::Yaml::Array(vec![yaml_rust2::Yaml::Integer(1), yaml_rust2::Yaml::Integer(2)]);
        let v = RuntimeValue::from(yaml_arr);
        assert!(matches!(v, RuntimeValue::Array(_)));
        if let RuntimeValue::Array(items) = v {
            assert_eq!(items.len(), 2);
        }
    }

    #[test]
    fn test_from_yaml_hash() {
        let mut hash = yaml_rust2::yaml::Hash::new();
        hash.insert(
            yaml_rust2::Yaml::String("key".to_string()),
            yaml_rust2::Yaml::Integer(99),
        );
        let v = RuntimeValue::from(yaml_rust2::Yaml::Hash(hash));
        assert!(matches!(v, RuntimeValue::Dict(_)));
    }

    #[test]
    fn test_from_yaml_alias() {
        let v = RuntimeValue::from(yaml_rust2::Yaml::Alias(0));
        assert_eq!(v, RuntimeValue::NONE);
    }

    #[test]
    fn test_from_ciborium_tag_unwraps_inner() {
        let inner = Box::new(ciborium::Value::Bool(true));
        let tagged = ciborium::Value::Tag(1, inner);
        let v = RuntimeValue::from(tagged);
        assert_eq!(v, RuntimeValue::Boolean(true));
    }

    #[test]
    fn test_from_ciborium_integer() {
        let v = RuntimeValue::from(ciborium::Value::Integer(42.into()));
        assert!(matches!(v, RuntimeValue::Number(_)));
    }

    #[test]
    fn test_from_ciborium_null_and_unknowns() {
        assert_eq!(RuntimeValue::from(ciborium::Value::Null), RuntimeValue::NONE);
    }

    #[test]
    fn test_from_ciborium_map() {
        let pairs = vec![(ciborium::Value::Text("k".to_string()), ciborium::Value::Bool(false))];
        let v = RuntimeValue::from(ciborium::Value::Map(pairs));
        assert!(matches!(v, RuntimeValue::Dict(_)));
    }

    #[test]
    fn test_from_ciborium_map_non_text_key() {
        let pairs = vec![(ciborium::Value::Integer(1.into()), ciborium::Value::Bool(true))];
        let v = RuntimeValue::from(ciborium::Value::Map(pairs));
        assert!(matches!(v, RuntimeValue::Dict(_)));
    }

    #[rstest]
    #[case(mq_markdown::AttrValue::String("s".to_string()), RuntimeValue::String("s".to_string()))]
    #[case(mq_markdown::AttrValue::Number(1.0), RuntimeValue::Number(1.0.into()))]
    #[case(mq_markdown::AttrValue::Boolean(true), RuntimeValue::Boolean(true))]
    #[case(mq_markdown::AttrValue::Null, RuntimeValue::NONE)]
    fn test_from_attr_value(#[case] attr: mq_markdown::AttrValue, #[case] expected: RuntimeValue) {
        assert_eq!(RuntimeValue::from(attr), expected);
    }

    #[test]
    fn test_from_attr_value_integer() {
        let v = RuntimeValue::from(mq_markdown::AttrValue::Integer(42));
        assert!(matches!(v, RuntimeValue::Number(_)));
    }

    #[test]
    fn test_from_attr_value_array() {
        let arr = mq_markdown::AttrValue::Array(vec![text_node("item")]);
        let v = RuntimeValue::from(arr);
        assert!(matches!(v, RuntimeValue::Array(_)));
    }

    #[test]
    fn test_from_serde_json_number_f64() {
        let n = serde_json::Number::from_f64(1.5).unwrap();
        let v = RuntimeValue::from(serde_json::Value::Number(n));
        assert!(matches!(v, RuntimeValue::Number(_)));
    }

    #[test]
    fn test_from_serde_json_object() {
        let mut obj = serde_json::Map::new();
        obj.insert("x".to_string(), serde_json::Value::Bool(true));
        let v = RuntimeValue::from(serde_json::Value::Object(obj));
        assert!(matches!(v, RuntimeValue::Dict(_)));
    }

    #[test]
    fn test_from_vec_tuple_number() {
        let v = RuntimeValue::from(vec![("count".to_string(), Number::from(7.0))]);
        if let RuntimeValue::Dict(map) = v {
            assert_eq!(map.get(&Ident::new("count")), Some(&RuntimeValue::Number(7.0.into())));
        } else {
            panic!("expected dict");
        }
    }

    #[test]
    fn test_update_with_non_markdown_returns_updated() {
        let orig: RuntimeValues = vec![RuntimeValue::Number(1.into())].into();
        let updated: RuntimeValues = vec![RuntimeValue::Number(99.into())].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0], RuntimeValue::Number(99.into()));
    }

    #[test]
    fn test_update_with_markdown_to_none_returns_original() {
        let orig: RuntimeValues = vec![md("original")].into();
        let updated: RuntimeValues = vec![RuntimeValue::None].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0], md("original"));
    }

    #[test]
    fn test_update_with_markdown_to_string() {
        let orig: RuntimeValues = vec![md("old")].into();
        let updated: RuntimeValues = vec![RuntimeValue::String("new".to_string())].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0].markdown_node().unwrap().value(), "new");
    }

    #[test]
    fn test_update_with_markdown_to_number() {
        let orig: RuntimeValues = vec![md("0")].into();
        let updated: RuntimeValues = vec![RuntimeValue::Number(42.into())].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0].markdown_node().unwrap().value(), "42");
    }

    #[test]
    fn test_update_with_markdown_to_boolean() {
        let orig: RuntimeValues = vec![md("false")].into();
        let updated: RuntimeValues = vec![RuntimeValue::Boolean(true)].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0].markdown_node().unwrap().value(), "true");
    }

    #[test]
    fn test_update_with_markdown_to_symbol() {
        let orig: RuntimeValues = vec![md("sym")].into();
        let updated: RuntimeValues = vec![RuntimeValue::Symbol(Ident::new("hello"))].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0].markdown_node().unwrap().value(), "hello");
    }

    #[test]
    fn test_update_with_markdown_to_bytes() {
        let orig: RuntimeValues = vec![md("bytes")].into();
        let updated: RuntimeValues = vec![RuntimeValue::Bytes(vec![0xff])].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0].markdown_node().unwrap().value(), "ff");
    }

    #[test]
    fn test_update_with_markdown_to_array_with_none_filtered() {
        let orig: RuntimeValues = vec![md("item")].into();
        let updated: RuntimeValues = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::None,
            RuntimeValue::String("b".to_string()),
        ]))]
        .into();
        let result = orig.update_with(updated);
        if let RuntimeValue::Array(items) = &result[0] {
            assert_eq!(items.len(), 2);
        } else {
            panic!("expected Array");
        }
    }

    #[test]
    fn test_update_with_markdown_to_dict() {
        let orig: RuntimeValues = vec![md("d")].into();
        let mut map = BTreeMap::new();
        map.insert(Ident::new("a"), RuntimeValue::String("val".to_string()));
        map.insert(Ident::new("b"), RuntimeValue::None);
        let updated: RuntimeValues = vec![RuntimeValue::Dict(Shared::new(map))].into();
        let result = orig.update_with(updated);
        if let RuntimeValue::Dict(m) = &result[0] {
            assert!(m.contains_key(&Ident::new("a")));
            assert!(!m.contains_key(&Ident::new("b"))); // None filtered out
        } else {
            panic!("expected Dict");
        }
    }

    #[test]
    fn test_update_with_markdown_to_native_function_returns_original() {
        let orig: RuntimeValues = vec![md("orig")].into();
        let updated: RuntimeValues = vec![RuntimeValue::NativeFunction(Ident::new("f"))].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0], md("orig"));
    }

    #[test]
    fn test_update_with_markdown_to_empty_markdown_returns_original() {
        let orig: RuntimeValues = vec![md("orig")].into();
        let updated: RuntimeValues = vec![RuntimeValue::Markdown(Box::new(mq_markdown::Node::Empty), None)].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0], md("orig"));
    }

    #[test]
    fn test_update_with_markdown_to_non_empty_markdown_returns_updated() {
        let orig: RuntimeValues = vec![md("old")].into();
        let updated: RuntimeValues = vec![md("new")].into();
        let result = orig.update_with(updated);
        assert_eq!(result[0].markdown_node().unwrap().value(), "new");
    }

    #[test]
    fn test_runtime_values_compact() {
        let values: RuntimeValues = vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::None,
            RuntimeValue::String("".to_string()),
            RuntimeValue::String("x".to_string()),
        ]
        .into();
        let compact = values.compact();
        assert_eq!(compact.len(), 2); // only Number(1) and String("x") survive
    }

    #[test]
    fn test_runtime_values_values() {
        let vals = vec![RuntimeValue::Boolean(true), RuntimeValue::None];
        let rv: RuntimeValues = vals.clone().into();
        assert_eq!(rv.values(), &vals);
    }

    #[test]
    fn test_runtime_values_into_iter() {
        let items: RuntimeValues = vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())].into();
        let sum: Vec<_> = items.into_iter().collect();
        assert_eq!(sum.len(), 2);
    }

    #[test]
    fn test_module_env_name_and_len() {
        use crate::SharedCell;
        let env = Shared::new(SharedCell::new(Env::default()));
        let m = ModuleEnv::new("mymod", env);
        assert_eq!(m.name(), "mymod");
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn test_module_partial_cmp() {
        use crate::SharedCell;
        let e1 = Shared::new(SharedCell::new(Env::default()));
        let e2 = Shared::new(SharedCell::new(Env::default()));
        let m1 = RuntimeValue::Module(ModuleEnv::new("alpha", e1));
        let m2 = RuntimeValue::Module(ModuleEnv::new("beta", e2));
        assert!(m1 < m2);
    }

    #[test]
    fn test_cross_type_partial_cmp_is_none() {
        let n = RuntimeValue::Number(1.into());
        let s = RuntimeValue::String("a".to_string());
        assert_eq!(n.partial_cmp(&s), None);
    }

    #[test]
    fn test_ast_partial_cmp_is_none() {
        let ast = RuntimeValue::Ast(crate::Shared::new(crate::AstNode {
            token_id: crate::arena::ArenaId::new(0),
            expr: crate::Shared::new(crate::AstExpr::Self_),
        }));
        assert_eq!(ast.partial_cmp(&RuntimeValue::None), None);
        assert_eq!(RuntimeValue::None.partial_cmp(&ast), None);
    }
}
