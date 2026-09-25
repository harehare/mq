#[cfg(any(feature = "file-io", feature = "http"))]
use crate::runtime::reader_handle::ReaderHandle;
use crate::{
    Ident, Shared,
    number::Number,
    tarn::interpreter::coroutine::{CoroutineHandle, CoroutineWeakHandle},
    tarn::value::ClosureValue,
};
use indexmap::IndexMap;
use mq_markdown::Node;
use rustc_hash::FxBuildHasher;
use std::{
    borrow::Cow,
    cmp::Ordering,
    ops::{Index, IndexMut},
};

/// The backing map for [`RuntimeValue::Dict`]: insertion-ordered, `FxHash`-based.
pub type DictMap = IndexMap<Ident, RuntimeValue, FxBuildHasher>;

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

/// A coroutine resumption helper represented as a first-class runtime value.
///
/// These helpers require VM execution state and therefore cannot use the ordinary native
/// builtin dispatcher. Keeping them distinct from [`RuntimeValue::NativeFunction`] also leaves
/// the dynamic native-builtin hot path free of coroutine-specific checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResumeBuiltin {
    /// Advances a coroutine without sending a value to its suspended `yield` expression.
    Next,
    /// Advances a coroutine and sends a value to its suspended `yield` expression.
    Send,
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
    /// A first-class coroutine resumption helper (`next` or `send`).
    CoroutineBuiltin(ResumeBuiltin),
    /// A VM closure that has crossed into plain-value territory (stored in an array/dict,
    /// passed to `partial`, ...) — see `tarn::value::ClosureValue`. `ClosureValue` is
    /// deliberately `pub(crate)` — this variant is constructible only from within the crate.
    ///
    /// `Shared`-wrapped, not inline: `ClosureValue` is 64 bytes (chunks/upvalues/bound_args),
    /// which would otherwise force every `RuntimeValue` variant to that size.
    #[doc(hidden)]
    #[allow(private_interfaces)]
    Closure(Shared<ClosureValue>),
    /// A dictionary mapping identifiers to runtime values.
    ///
    /// Same clone-on-write scheme as [`RuntimeValue::Array`]; see [`dict_mut`].
    Dict(Shared<DictMap>),
    /// Raw binary data (e.g. CBOR byte strings).
    ///
    /// Same clone-on-write scheme as [`RuntimeValue::Array`]; see [`bytes_mut`].
    Bytes(Shared<Vec<u8>>),
    /// A generator function's coroutine, see `tarn::interpreter::coroutine::CoroutineState`.
    /// `CoroutineState` is deliberately `pub(crate)`, so this variant is constructible only
    /// from within the crate. Cloning shares progress: every clone drives the same coroutine.
    #[doc(hidden)]
    #[allow(private_interfaces)]
    Coroutine(CoroutineHandle),
    /// A coroutine downgraded to break a capture cycle. Only ever lives inside a
    /// `StackValue::NestedWeakCoroutine` cell, resolved before any other code sees it.
    #[doc(hidden)]
    #[allow(private_interfaces)]
    WeakCoroutine(CoroutineWeakHandle),
    /// An open read-only file handle from `open_file`. Cloning shares the handle.
    #[cfg(any(feature = "file-io", feature = "http"))]
    #[doc(hidden)]
    #[allow(private_interfaces)]
    ReaderHandle(Shared<ReaderHandle>),
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
            (RuntimeValue::Closure(a), RuntimeValue::Closure(b)) => {
                Shared::ptr_eq(&a.chunks, &b.chunks) && a.chunk_index == b.chunk_index && a.bound_args == b.bound_args
            }
            (RuntimeValue::NativeFunction(a), RuntimeValue::NativeFunction(b)) => a == b,
            (RuntimeValue::CoroutineBuiltin(a), RuntimeValue::CoroutineBuiltin(b)) => a == b,
            (RuntimeValue::Dict(a), RuntimeValue::Dict(b)) => a == b,
            (RuntimeValue::Bytes(a), RuntimeValue::Bytes(b)) => a == b,
            (RuntimeValue::Coroutine(a), RuntimeValue::Coroutine(b)) => Shared::ptr_eq(a, b),
            (RuntimeValue::WeakCoroutine(a), RuntimeValue::WeakCoroutine(b)) => a.ptr_eq(b),
            #[cfg(any(feature = "file-io", feature = "http"))]
            (RuntimeValue::ReaderHandle(a), RuntimeValue::ReaderHandle(b)) => Shared::ptr_eq(a, b),
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

impl From<DictMap> for RuntimeValue {
    fn from(map: DictMap) -> Self {
        RuntimeValue::Dict(Shared::new(map))
    }
}

impl From<Vec<(String, Number)>> for RuntimeValue {
    fn from(v: Vec<(String, Number)>) -> Self {
        RuntimeValue::Dict(Shared::new(
            v.into_iter()
                .map(|(k, v)| (Ident::new(&k), RuntimeValue::Number(v)))
                .collect::<DictMap>(),
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
                let mut btree = DictMap::default();
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
                let mut map = DictMap::default();
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
                RuntimeValue::Number(Number::from(n as f64))
            }
            ciborium::Value::Float(f) => RuntimeValue::Number(Number::from(f)),
            ciborium::Value::Text(s) => RuntimeValue::String(Shared::new(s)),
            ciborium::Value::Bytes(b) => RuntimeValue::Bytes(Shared::new(b)),
            ciborium::Value::Array(arr) => {
                let items = arr.into_iter().map(Into::into).collect();
                RuntimeValue::Array(Shared::new(items))
            }
            ciborium::Value::Map(pairs) => {
                let mut map = DictMap::default();
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
            (RuntimeValue::Markdown(a, _), RuntimeValue::Markdown(b, _)) => a.to_string().partial_cmp(&b.to_string()),
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
            Self::CoroutineBuiltin(_) => Cow::Borrowed("native_function"),
            Self::Closure(_) => Cow::Borrowed("function"),
            Self::Dict(_) => self.string(),
            Self::Bytes(b) => Cow::Owned(bytes_to_hex(b)),
            Self::Coroutine(_) | Self::WeakCoroutine(_) => Cow::Borrowed("coroutine"),
            #[cfg(any(feature = "file-io", feature = "http"))]
            Self::ReaderHandle(handle) => Cow::Borrowed(handle.kind().as_str()),
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
pub(crate) fn dict_mut(map: &mut Shared<DictMap>) -> &mut DictMap {
    Shared::<DictMap>::make_mut(map)
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
    pub(crate) fn vm_bound_value_kind(&self) -> Option<&'static str> {
        let mut pending = vec![self];
        while let Some(value) = pending.pop() {
            match value {
                Self::Coroutine(_) => return Some("coroutine"),
                Self::Closure(_) => return Some("VM closure"),
                Self::Array(values) => pending.extend(values.iter()),
                Self::Dict(values) => pending.extend(values.values()),
                _ => {}
            }
        }
        None
    }

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

    /// Returns a new empty array with enough storage for `capacity` values.
    #[inline(always)]
    pub(crate) fn array_with_capacity(capacity: usize) -> RuntimeValue {
        RuntimeValue::Array(Shared::new(Vec::with_capacity(capacity)))
    }

    /// Creates a new empty dictionary.
    #[inline(always)]
    pub fn new_dict() -> RuntimeValue {
        RuntimeValue::Dict(Shared::new(DictMap::default()))
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
            RuntimeValue::CoroutineBuiltin(_) => "native_function",
            RuntimeValue::Closure(_) => "function",
            RuntimeValue::Dict(_) => "dict",
            RuntimeValue::Bytes(_) => "bytes",
            RuntimeValue::Coroutine(_) => "coroutine",
            RuntimeValue::WeakCoroutine(_) => "coroutine",
            #[cfg(any(feature = "file-io", feature = "http"))]
            RuntimeValue::ReaderHandle(handle) => handle.kind().as_str(),
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
        matches!(self, RuntimeValue::Closure(_))
    }

    /// Returns `true` if this value is a resumable coroutine.
    ///
    /// Weak coroutine references exist only while the VM breaks internal capture cycles and
    /// cannot be resumed by callers, so they deliberately return `false` here.
    #[inline(always)]
    pub fn is_coroutine(&self) -> bool {
        matches!(self, RuntimeValue::Coroutine(_))
    }

    /// Returns `true` if this value is a native (built-in) function.
    #[inline(always)]
    pub fn is_native_function(&self) -> bool {
        matches!(
            self,
            RuntimeValue::NativeFunction(_) | RuntimeValue::CoroutineBuiltin(_)
        )
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
            RuntimeValue::Symbol(_)
            | RuntimeValue::NativeFunction(_)
            | RuntimeValue::CoroutineBuiltin(_)
            | RuntimeValue::Dict(_) => true,
            RuntimeValue::Closure(_) => true,
            RuntimeValue::Bytes(b) => !b.is_empty(),
            RuntimeValue::Coroutine(_) | RuntimeValue::WeakCoroutine(_) => true,
            #[cfg(any(feature = "file-io", feature = "http"))]
            RuntimeValue::ReaderHandle(_) => true,
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
            RuntimeValue::CoroutineBuiltin(..) => 0,
            RuntimeValue::Closure(..) => 0,
            RuntimeValue::Coroutine(..) | RuntimeValue::WeakCoroutine(..) => 0,
            #[cfg(any(feature = "file-io", feature = "http"))]
            RuntimeValue::ReaderHandle(..) => 0,
        }
    }

    /// Returns the string if this is a `String`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            RuntimeValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the number if this is a `Number`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            RuntimeValue::Number(n) => Some(n.value()),
            _ => None,
        }
    }

    /// Returns the boolean if this is a `Boolean`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            RuntimeValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns the elements if this is an `Array`.
    pub fn as_array(&self) -> Option<&[RuntimeValue]> {
        match self {
            RuntimeValue::Array(values) => Some(values.as_slice()),
            _ => None,
        }
    }

    /// Returns the entries if this is a `Dict`.
    pub fn as_dict(&self) -> Option<&DictMap> {
        match self {
            RuntimeValue::Dict(map) => Some(map),
            _ => None,
        }
    }

    /// Looks up `key` if this is a `Dict`.
    pub fn get(&self, key: &str) -> Option<&RuntimeValue> {
        self.as_dict()?.get(&Ident::lookup(key)?)
    }

    /// Converts to a Markdown node: markdown values keep their (selected) node, anything else
    /// becomes a text node of its string form.
    pub fn into_markdown_node(self) -> Node {
        match self {
            RuntimeValue::Markdown(node, None) => Shared::unwrap_or_clone(node),
            value @ RuntimeValue::Markdown(_, Some(_)) => value.markdown_node().unwrap_or_else(|| "".into()),
            value => value.to_string().into(),
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
            RuntimeValue::Markdown(n, Some(sel)) => RuntimeValue::Markdown(
                Shared::new((**n).clone().into_with_children_value(value, sel.index_value())),
                Some(*sel),
            ),
            RuntimeValue::Markdown(n, selector) => {
                RuntimeValue::Markdown(Shared::new((**n).clone().into_with_value(value)), *selector)
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

    /// Clears position only from the `Text` leaf written by a prior `update_markdown_value`
    /// call, so the renderer won't re-escape it. Unlike `strip_positions`, leaves the rest of
    /// the tree untouched.
    #[inline(always)]
    pub fn strip_updated_text_position(&mut self) {
        if let RuntimeValue::Markdown(node, selector) = self {
            let index = (*selector).map(Selector::index_value).unwrap_or(0);
            markdown_mut(node).clear_text_position_at(index);
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
            Self::CoroutineBuiltin(_) => Cow::Borrowed("native_function"),
            Self::Closure(_) => Cow::Borrowed("function"),
            Self::Bytes(b) => Cow::Owned(bytes_to_hex(b)),
            Self::Coroutine(_) | Self::WeakCoroutine(_) => Cow::Borrowed("coroutine"),
            #[cfg(any(feature = "file-io", feature = "http"))]
            Self::ReaderHandle(handle) => Cow::Borrowed(handle.kind().as_str()),
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
                let v = n.value();
                // `i64::MAX as f64` and `u64::MAX as f64` round up to 2^63 and 2^64, so `<` keeps the bounds exact.
                if n.is_int() && v >= i64::MIN as f64 && v < i64::MAX as f64 {
                    serde_json::Value::Number(serde_json::Number::from(n.to_int()))
                } else if n.is_int() && v >= 0.0 && v < u64::MAX as f64 {
                    serde_json::Value::Number(serde_json::Number::from(v as u64))
                } else {
                    serde_json::Number::from_f64(v)
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

/// Error returned by [`from_value`].
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct FromValueError(serde_json::Error);

/// Deserializes a runtime value into `T` through its JSON form (see
/// [`RuntimeValue::to_json_value`]).
///
/// # Examples
///
/// ```rust
/// #[derive(serde::Deserialize)]
/// struct Task {
///     title: String,
///     done: bool,
/// }
///
/// let mut engine = mq_lang::DefaultEngine::default();
/// engine.load_builtin_module();
/// let output = engine
///     .eval(r#"{"title": "build", "done": true}"#, mq_lang::null_input().into_iter())
///     .unwrap();
/// let task: Task = mq_lang::from_value(&output[0]).unwrap();
/// assert_eq!(task.title, "build");
/// assert!(task.done);
/// ```
pub fn from_value<T: serde::de::DeserializeOwned>(value: &RuntimeValue) -> Result<T, FromValueError> {
    serde_json::from_value(value.clone().to_json_value()).map_err(FromValueError)
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

    /// Converts every value with [`RuntimeValue::into_markdown_node`].
    pub fn into_markdown_nodes(self) -> Vec<Node> {
        self.0.into_iter().map(RuntimeValue::into_markdown_node).collect()
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
                        RuntimeValue::None | RuntimeValue::NativeFunction(_) | RuntimeValue::CoroutineBuiltin(_) => {
                            current_value.clone()
                        }
                        RuntimeValue::Closure(_) => current_value.clone(),
                        RuntimeValue::Coroutine(_) | RuntimeValue::WeakCoroutine(_) => current_value.clone(),
                        #[cfg(any(feature = "file-io", feature = "http"))]
                        RuntimeValue::ReaderHandle(_) => current_value.clone(),
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
                            let mut new_dict = DictMap::default();
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
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;
    use serde::Deserialize;

    fn dict(entries: &[(&str, RuntimeValue)]) -> RuntimeValue {
        let mut map = DictMap::default();
        for (key, value) in entries {
            map.insert(Ident::new(key), value.clone());
        }
        RuntimeValue::Dict(Shared::new(map))
    }

    #[rstest]
    #[case::string("a".into(), Some("a"), None, None)]
    #[case::number(RuntimeValue::Number(1.5.into()), None, Some(1.5), None)]
    #[case::boolean(RuntimeValue::TRUE, None, None, Some(true))]
    #[case::none(RuntimeValue::NONE, None, None, None)]
    fn test_scalar_accessors(
        #[case] value: RuntimeValue,
        #[case] string: Option<&str>,
        #[case] number: Option<f64>,
        #[case] boolean: Option<bool>,
    ) {
        assert_eq!(value.as_str(), string);
        assert_eq!(value.as_f64(), number);
        assert_eq!(value.as_bool(), boolean);
    }

    #[rstest]
    #[case::array(vec![RuntimeValue::TRUE].into(), Some(vec![RuntimeValue::TRUE]))]
    #[case::string("a".into(), None)]
    fn test_as_array(#[case] value: RuntimeValue, #[case] expected: Option<Vec<RuntimeValue>>) {
        assert_eq!(value.as_array().map(<[RuntimeValue]>::to_vec), expected);
    }

    #[rstest]
    #[case::present(dict(&[("title", "build".into())]), "title", Some(RuntimeValue::from("build")))]
    #[case::missing(dict(&[("title", "build".into())]), "title_never_interned_anywhere", None)]
    #[case::not_dict("title".into(), "title", None)]
    fn test_get(#[case] value: RuntimeValue, #[case] key: &str, #[case] expected: Option<RuntimeValue>) {
        assert_eq!(value.get(key).cloned(), expected);
        assert_eq!(value.as_dict().is_some(), matches!(value, RuntimeValue::Dict(_)));
    }

    #[test]
    fn test_get_does_not_intern_missing_keys() {
        let value = dict(&[]);
        assert!(value.get("key_only_used_by_this_lookup").is_none());
        assert!(Ident::lookup("key_only_used_by_this_lookup").is_none());
    }

    #[rstest]
    #[case::markdown(RuntimeValue::new_markdown("text".into()), Node::from("text"))]
    #[case::string("text".into(), Node::from("text"))]
    #[case::number(RuntimeValue::Number(2.into()), Node::from("2"))]
    fn test_into_markdown_node(#[case] value: RuntimeValue, #[case] expected: Node) {
        assert_eq!(value.into_markdown_node(), expected);
    }

    #[test]
    fn test_into_markdown_nodes_keeps_order() {
        let values: RuntimeValues = vec![RuntimeValue::new_markdown("a".into()), "b".into()].into();
        assert_eq!(values.into_markdown_nodes(), vec![Node::from("a"), Node::from("b")]);
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct CodeBlock {
        lang: Option<String>,
        code: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Section {
        title: String,
        #[serde(default)]
        description: Option<String>,
        codes: Vec<CodeBlock>,
        level: u8,
    }

    #[test]
    fn test_from_value_into_struct() {
        let value = dict(&[
            ("title", "Build".into()),
            ("level", RuntimeValue::Number(2.into())),
            (
                "codes",
                vec![dict(&[("lang", "sh".into()), ("code", "make".into())])].into(),
            ),
        ]);

        assert_eq!(
            from_value::<Section>(&value).unwrap(),
            Section {
                title: "Build".to_string(),
                description: None,
                codes: vec![CodeBlock {
                    lang: Some("sh".to_string()),
                    code: "make".to_string(),
                }],
                level: 2,
            }
        );
    }

    #[rstest]
    #[case::missing_field(dict(&[("title", "Build".into())]))]
    #[case::wrong_type(dict(&[("title", RuntimeValue::TRUE), ("codes", RuntimeValue::empty_array()), ("level", RuntimeValue::Number(1.into()))]))]
    #[case::fractional_level(dict(&[("title", "a".into()), ("codes", RuntimeValue::empty_array()), ("level", RuntimeValue::Number(1.5.into()))]))]
    fn test_from_value_rejects_mismatch(#[case] value: RuntimeValue) {
        assert!(from_value::<Section>(&value).is_err());
    }

    proptest! {
        #[test]
        fn from_value_round_trips_scalars(s in ".*", n in any::<i32>(), b in any::<bool>()) {
            prop_assert_eq!(from_value::<String>(&RuntimeValue::from(s.clone())).unwrap(), s);
            prop_assert_eq!(from_value::<i32>(&RuntimeValue::Number((n as f64).into())).unwrap(), n);
            prop_assert_eq!(from_value::<bool>(&RuntimeValue::Boolean(b)).unwrap(), b);
        }

        #[test]
        fn from_value_round_trips_string_arrays(items in prop::collection::vec(".*", 0..8)) {
            let value: RuntimeValue = items.iter().cloned().map(RuntimeValue::from).collect::<Vec<_>>().into();
            prop_assert_eq!(from_value::<Vec<String>>(&value).unwrap(), items);
        }
    }
}
