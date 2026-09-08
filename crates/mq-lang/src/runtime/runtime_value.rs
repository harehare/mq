use crate::{Ident, Shared, number::Number};
use mq_markdown::Node;
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::BTreeMap,
    ops::{Index, IndexMut},
};

/// Runtime selector for indexing into markdown nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Selector {
    Index(std::num::NonZeroU32),
}

impl Selector {
    /// `None` if `i` doesn't fit (indices `>= u32::MAX`); callers treat that the same as any
    /// other out-of-range index (e.g. `RuntimeValue::NONE`), not an error. `NonZeroU32` was
    /// chosen over a plain `usize` because `Option<Selector>` still fits in the padding
    /// alongside `Shared<Node>`, so `RuntimeValue` stays 16 bytes instead of growing to 24.
    #[inline(always)]
    pub(crate) fn index(i: usize) -> Option<Self> {
        i.checked_add(1)
            .and_then(|n| u32::try_from(n).ok())
            .and_then(std::num::NonZeroU32::new)
            .map(Selector::Index)
    }

    #[inline(always)]
    pub(crate) fn index_value(self) -> usize {
        let Selector::Index(n) = self;
        (n.get() - 1) as usize
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
    ///
    /// Same clone-on-write scheme as [`RuntimeValue::Array`]; see [`string_mut`].
    String(Shared<String>),
    /// A symbol (interned identifier).
    Symbol(Ident),
    /// An array of runtime values.
    ///
    /// Behind [`Shared`] for clone-on-write: cloning is an O(1) refcount bump; mutating
    /// builtins must go through [`array_mut`] instead of mutating directly.
    Array(Shared<Vec<RuntimeValue>>),
    /// A markdown node with an optional selector for indexing.
    ///
    /// Same clone-on-write scheme as [`RuntimeValue::Array`]; see [`markdown_mut`].
    Markdown(Shared<Node>, Option<Selector>),
    /// A built-in native function identified by name.
    NativeFunction(Ident),
    /// A VM closure that has crossed into plain-value territory (stored in an array/dict,
    /// passed to `partial`, ...) — see `tarn::value::VmClosureValue`. `VmClosureValue` is
    /// deliberately `pub(crate)` — this variant is constructible only from within the crate.
    ///
    /// `Shared`-wrapped, not inline: `VmClosureValue` is 64 bytes (chunks/upvalues/bound_args),
    /// which would otherwise force every `RuntimeValue` variant to that size.
    #[allow(private_interfaces)]
    VmClosure(Shared<crate::tarn::value::VmClosureValue>),
    /// A dictionary mapping identifiers to runtime values.
    ///
    /// Same clone-on-write scheme as [`RuntimeValue::Array`]; see [`dict_mut`].
    Dict(Shared<BTreeMap<Ident, RuntimeValue>>),
    /// Raw binary data (e.g. CBOR byte strings).
    ///
    /// Same clone-on-write scheme as [`RuntimeValue::Array`]; see [`bytes_mut`].
    Bytes(Shared<Vec<u8>>),
    /// An empty or null value.
    #[default]
    None,
}

impl PartialEq for RuntimeValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a == b,
            (RuntimeValue::Boolean(a), RuntimeValue::Boolean(b)) => a == b,
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
            (RuntimeValue::Symbol(a), RuntimeValue::Symbol(b)) => a == b,
            (RuntimeValue::Array(a), RuntimeValue::Array(b)) => a == b,
            (RuntimeValue::Markdown(a, sa), RuntimeValue::Markdown(b, sb)) => a == b && sa == sb,
            (RuntimeValue::VmClosure(a), RuntimeValue::VmClosure(b)) => {
                Shared::ptr_eq(&a.chunks, &b.chunks) && a.chunk_index == b.chunk_index && a.bound_args == b.bound_args
            }
            (RuntimeValue::NativeFunction(a), RuntimeValue::NativeFunction(b)) => a == b,
            (RuntimeValue::Dict(a), RuntimeValue::Dict(b)) => a == b,
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
        RuntimeValue::String(Shared::new(s))
    }
}

impl From<&str> for RuntimeValue {
    fn from(s: &str) -> Self {
        RuntimeValue::String(Shared::new(s.to_string()))
    }
}

impl From<&mut str> for RuntimeValue {
    fn from(s: &mut str) -> Self {
        RuntimeValue::String(Shared::new(s.to_string()))
    }
}

impl From<Shared<String>> for RuntimeValue {
    fn from(s: Shared<String>) -> Self {
        RuntimeValue::String(s)
    }
}

impl From<Vec<u8>> for RuntimeValue {
    fn from(b: Vec<u8>) -> Self {
        RuntimeValue::Bytes(Shared::new(b))
    }
}

impl From<Shared<Vec<u8>>> for RuntimeValue {
    fn from(b: Shared<Vec<u8>>) -> Self {
        RuntimeValue::Bytes(b)
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
            mq_markdown::AttrValue::String(s) => RuntimeValue::String(Shared::new(s)),
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
            yaml_rust2::Yaml::String(s) => RuntimeValue::String(Shared::new(s)),
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
            serde_json::Value::String(s) => RuntimeValue::String(Shared::new(s)),
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
            ciborium::Value::Text(s) => RuntimeValue::String(Shared::new(s)),
            ciborium::Value::Bytes(b) => RuntimeValue::Bytes(Shared::new(b)),
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
            (RuntimeValue::Bytes(a), RuntimeValue::Bytes(b)) => a.partial_cmp(b),
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
            Self::Boolean(b) => Cow::Owned(b.to_string()),
            Self::String(s) => Cow::Borrowed(s),
            Self::Symbol(i) => Cow::Owned(format!(":{}", i)),
            Self::Array(_) => self.string(),
            Self::Markdown(m, ..) => Cow::Owned(m.to_string()),
            Self::None => Cow::Borrowed(""),
            Self::NativeFunction(_) => Cow::Borrowed("native_function"),
            Self::VmClosure(_) => Cow::Borrowed("function"),
            Self::Dict(_) => self.string(),
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

/// Clone-on-write access to a markdown node; see [`array_mut`].
#[inline(always)]
pub(crate) fn markdown_mut(node: &mut Shared<Node>) -> &mut Node {
    Shared::<Node>::make_mut(node)
}

/// Clone-on-write access to a string's contents; see [`array_mut`].
#[inline(always)]
pub(crate) fn string_mut(s: &mut Shared<String>) -> &mut String {
    Shared::<String>::make_mut(s)
}

/// Clone-on-write access to a byte buffer's contents; see [`array_mut`].
#[inline(always)]
pub(crate) fn bytes_mut(b: &mut Shared<Vec<u8>>) -> &mut Vec<u8> {
    Shared::<Vec<u8>>::make_mut(b)
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
        RuntimeValue::Markdown(Shared::new(node), None)
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
            RuntimeValue::NativeFunction(_) => "native_function",
            RuntimeValue::VmClosure(_) => "function",
            RuntimeValue::Dict(_) => "dict",
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
        matches!(self, RuntimeValue::VmClosure(_))
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
                Some(sel) => node.find_at_index(sel.index_value()).is_some(),
                None => true,
            },
            RuntimeValue::Symbol(_) | RuntimeValue::NativeFunction(_) | RuntimeValue::Dict(_) => true,
            RuntimeValue::VmClosure(_) => true,
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
            RuntimeValue::NativeFunction(..) => 0,
            RuntimeValue::VmClosure(..) => 0,
        }
    }

    /// Extracts the markdown node from this value, if it is a markdown value.
    ///
    /// If a selector is present, returns the selected child node.
    #[inline(always)]
    pub fn markdown_node(&self) -> Option<Node> {
        match self {
            RuntimeValue::Markdown(n, Some(sel)) => n.find_at_index(sel.index_value()),
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
            RuntimeValue::Markdown(n, Some(sel)) => {
                RuntimeValue::Markdown(Shared::new(n.with_children_value(value, sel.index_value())), Some(*sel))
            }
            RuntimeValue::Markdown(n, selector) => RuntimeValue::Markdown(Shared::new(n.with_value(value)), *selector),
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
            markdown_mut(node).set_position(position);
        }
    }

    /// Clears position information from a markdown node (recursively).
    ///
    /// Only affects markdown values; other value types are unaffected.
    #[inline(always)]
    pub fn strip_positions(&mut self) {
        if let RuntimeValue::Markdown(node, _) = self {
            markdown_mut(node).strip_positions();
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
            Self::NativeFunction(_) => Cow::Borrowed("native_function"),
            Self::VmClosure(_) => Cow::Borrowed("function"),
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
            RuntimeValue::String(s) => RuntimeValue::String(Shared::new(s.chars().rev().collect())),
            _ => self.clone(),
        }
    }

    pub fn to_json_value(self) -> serde_json::Value {
        use base64::Engine;
        match self {
            RuntimeValue::None => serde_json::Value::Null,
            RuntimeValue::Boolean(b) => serde_json::Value::Bool(b),
            RuntimeValue::Number(n) => {
                if n.is_int() {
                    serde_json::Value::Number(serde_json::Number::from(n.to_int()))
                } else {
                    serde_json::Number::from_f64(n.value())
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                }
            }
            RuntimeValue::String(s) => serde_json::Value::String(Shared::unwrap_or_clone(s)),
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
            RuntimeValue::Bytes(b) => serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&*b)),
            RuntimeValue::Markdown(node, _) => serde_json::to_value(node.as_ref()).unwrap_or(serde_json::Value::Null),
            _ => serde_json::Value::Null,
        }
    }

    pub fn to_cbor_value(self) -> ciborium::Value {
        match self {
            RuntimeValue::None => ciborium::Value::Null,
            RuntimeValue::Boolean(b) => ciborium::Value::Bool(b),
            RuntimeValue::Number(n) => ciborium::Value::Float(n.value()),
            RuntimeValue::String(s) => ciborium::Value::Text(Shared::unwrap_or_clone(s)),
            RuntimeValue::Symbol(i) => ciborium::Value::Text(i.to_string()),
            RuntimeValue::Bytes(b) => ciborium::Value::Bytes(Shared::unwrap_or_clone(b)),
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
                        RuntimeValue::None | RuntimeValue::NativeFunction(_) => current_value.clone(),
                        RuntimeValue::VmClosure(_) => current_value.clone(),
                        RuntimeValue::Markdown(node, _) if node.is_empty() => current_value.clone(),
                        RuntimeValue::Markdown(node, _) => {
                            if node.is_fragment() {
                                if let RuntimeValue::Markdown(mut current_node, selector) = current_value {
                                    markdown_mut(&mut current_node).apply_fragment((**node).clone());
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
                                            Shared::new(node.with_value(o.to_string().as_str())),
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
