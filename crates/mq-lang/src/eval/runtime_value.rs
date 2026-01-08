use super::env::Env;
use crate::{AstParams, Ident, Program, Shared, SharedCell, ast, number::Number};
use mq_markdown::Node;
use smol_str::SmolStr;
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::BTreeMap,
    fmt::Write,
    ops::{Index, IndexMut},
};

use ast::node::{Expr, Literal, StringSegment};

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
    Array(Vec<RuntimeValue>),
    /// A markdown node with an optional selector for indexing.
    Markdown(Node, Option<Selector>),
    /// A user-defined function with parameters, body (program), and captured environment.
    Function(AstParams, Program, Shared<SharedCell<Env>>),
    /// A built-in native function identified by name.
    NativeFunction(Ident),
    /// A dictionary mapping identifiers to runtime values.
    Dict(BTreeMap<Ident, RuntimeValue>),
    /// A module with its exports.
    Module(ModuleEnv),
    /// An AST node (quoted expression).
    Ast(Shared<ast::node::Node>),
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
        RuntimeValue::Array(arr)
    }
}

impl From<BTreeMap<Ident, RuntimeValue>> for RuntimeValue {
    fn from(map: BTreeMap<Ident, RuntimeValue>) -> Self {
        RuntimeValue::Dict(map)
    }
}

impl From<Vec<(String, Number)>> for RuntimeValue {
    fn from(v: Vec<(String, Number)>) -> Self {
        RuntimeValue::Dict(
            v.into_iter()
                .map(|(k, v)| (Ident::new(&k), RuntimeValue::Number(v)))
                .collect::<BTreeMap<Ident, RuntimeValue>>(),
        )
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
                RuntimeValue::Array(arr.into_iter().map(RuntimeValue::from).collect())
            }
            mq_markdown::AttrValue::Null => RuntimeValue::NONE,
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
        match self {
            Self::Ast(node) => {
                let mut buf = String::new();
                Self::format_ast_node(node, &mut buf)?;
                write!(f, "{}", buf)
            }
            _ => {
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
                    Self::Ast(_) => unreachable!(),
                };
                write!(f, "{}", value)
            }
        }
    }
}

impl std::fmt::Debug for RuntimeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let v: Cow<'_, str> = match self {
            Self::None => Cow::Borrowed("None"),
            Self::String(s) => Cow::Owned(format!("{:?}", s)),
            Self::Array(arr) => Cow::Owned(format!("{:?}", arr)),
            a => a.string(),
        };
        write!(f, "{}", v)
    }
}

impl RuntimeValue {
    /// An empty array constant.
    pub const EMPTY_ARRAY: RuntimeValue = Self::Array(Vec::new());
    /// The boolean `false` value.
    pub const FALSE: RuntimeValue = Self::Boolean(false);
    /// The `None` (null) value.
    pub const NONE: RuntimeValue = Self::None;
    /// The boolean `true` value.
    pub const TRUE: RuntimeValue = Self::Boolean(true);

    /// Creates a new empty dictionary.
    #[inline(always)]
    pub fn new_dict() -> RuntimeValue {
        RuntimeValue::Dict(BTreeMap::new())
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
            RuntimeValue::Markdown(n, _) => Some(n.clone()),
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
                RuntimeValue::Markdown(n.with_children_value(value, *i), Some(Selector::Index(*i)))
            }
            RuntimeValue::Markdown(n, selector) => RuntimeValue::Markdown(n.with_value(value), selector.clone()),
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
            Self::Ast(node) => {
                let mut buf = String::new();
                let _ = Self::format_ast_node(node, &mut buf);
                Cow::Owned(buf)
            }
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

    fn format_ident(ident: &Ident, buf: &mut String) -> std::fmt::Result {
        write!(buf, "Ident(\"{}\")", ident)
    }

    fn format_ast_node(node: &ast::node::Node, buf: &mut String) -> std::fmt::Result {
        match &*node.expr {
            Expr::Literal(lit) => {
                write!(buf, "Literal(")?;
                match lit {
                    Literal::String(s) => write!(buf, "String(\"{}\")", s)?,
                    Literal::Number(n) => write!(buf, "Number({})", n)?,
                    Literal::Symbol(i) => write!(buf, "Symbol({})", i)?,
                    Literal::Bool(b) => write!(buf, "Bool({})", b)?,
                    Literal::None => write!(buf, "None")?,
                }
                write!(buf, ")")?;
            }
            Expr::Ident(ident) => {
                Self::format_ident(&ident.name, buf)?;
            }
            Expr::Call(ident, args) => {
                write!(buf, "Call(\"{}\", [", ident)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(arg, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::CallDynamic(callable, args) => {
                write!(buf, "CallDynamic(")?;
                Self::format_ast_node(callable, buf)?;
                write!(buf, ", [")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(arg, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::Block(program) => {
                write!(buf, "Block([")?;
                for (i, node) in program.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(node, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::Def(ident, params, body) => {
                write!(buf, "Def(\"{}\", [", ident)?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }

                    Self::format_ident(&param.ident.name, buf)?;

                    if let Some(default) = &param.default {
                        write!(buf, " = ")?;
                        Self::format_ast_node(default, buf)?;
                    }
                }
                write!(buf, "], [")?;
                for (i, node) in body.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(node, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::Macro(ident, params, block) => {
                write!(buf, "Macro(\"{}\", [", ident)?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }

                    Self::format_ident(&param.ident.name, buf)?;

                    if let Some(default) = &param.default {
                        write!(buf, " = ")?;
                        Self::format_ast_node(default, buf)?;
                    }
                }
                write!(buf, "], ")?;
                Self::format_ast_node(block, buf)?;
                write!(buf, ")")?;
            }
            Expr::Fn(params, body) => {
                write!(buf, "Fn([")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }

                    Self::format_ident(&param.ident.name, buf)?;

                    if let Some(default) = &param.default {
                        write!(buf, " = ")?;
                        Self::format_ast_node(default, buf)?;
                    }
                }
                write!(buf, "], [")?;
                for (i, node) in body.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(node, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::Let(ident, value) => {
                write!(buf, "Let(\"{}\", ", ident)?;
                Self::format_ast_node(value, buf)?;
                write!(buf, ")")?;
            }
            Expr::Var(ident, value) => {
                write!(buf, "Var(\"{}\", ", ident)?;
                Self::format_ast_node(value, buf)?;
                write!(buf, ")")?;
            }
            Expr::Assign(ident, value) => {
                write!(buf, "Assign(\"{}\", ", ident)?;
                Self::format_ast_node(value, buf)?;
                write!(buf, ")")?;
            }
            Expr::And(left, right) => {
                write!(buf, "And(")?;
                Self::format_ast_node(left, buf)?;
                write!(buf, ", ")?;
                Self::format_ast_node(right, buf)?;
                write!(buf, ")")?;
            }
            Expr::Or(left, right) => {
                write!(buf, "Or(")?;
                Self::format_ast_node(left, buf)?;
                write!(buf, ", ")?;
                Self::format_ast_node(right, buf)?;
                write!(buf, ")")?;
            }
            Expr::InterpolatedString(segments) => {
                write!(buf, "InterpolatedString([")?;
                for (i, segment) in segments.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    match segment {
                        StringSegment::Text(s) => write!(buf, "Text(\"{}\")", s)?,
                        StringSegment::Expr(expr) => {
                            write!(buf, "Expr(")?;
                            Self::format_ast_node(expr, buf)?;
                            write!(buf, ")")?;
                        }
                        StringSegment::Env(e) => write!(buf, "Env(\"{}\")", e)?,
                        StringSegment::Self_ => write!(buf, "Self")?,
                    }
                }
                write!(buf, "])")?;
            }
            Expr::Selector(selector) => {
                write!(buf, "Selector({:?})", selector)?;
            }
            Expr::While(cond, body) => {
                write!(buf, "While(")?;
                Self::format_ast_node(cond, buf)?;
                write!(buf, ", [")?;
                for (i, node) in body.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(node, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::Loop(body) => {
                write!(buf, "Loop([")?;
                for (i, node) in body.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(node, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::Foreach(ident, iter, body) => {
                write!(buf, "Foreach(\"{}\", ", ident)?;
                Self::format_ast_node(iter, buf)?;
                write!(buf, ", [")?;
                for (i, node) in body.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(node, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::If(branches) => {
                write!(buf, "If([")?;
                for (i, (cond, body)) in branches.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    write!(buf, "Branch(")?;
                    if let Some(c) = cond {
                        Self::format_ast_node(c, buf)?;
                    } else {
                        write!(buf, "None")?;
                    }
                    write!(buf, ", ")?;
                    Self::format_ast_node(body, buf)?;
                    write!(buf, ")")?;
                }
                write!(buf, "])")?;
            }
            Expr::Match(value, arms) => {
                write!(buf, "Match(")?;
                Self::format_ast_node(value, buf)?;
                write!(buf, ", [")?;
                for (i, arm) in arms.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    write!(buf, "Arm(")?;
                    Self::format_pattern(&arm.pattern, buf)?;
                    if let Some(guard) = &arm.guard {
                        write!(buf, ", Guard(")?;
                        Self::format_ast_node(guard, buf)?;
                        write!(buf, ")")?;
                    }
                    write!(buf, ", ")?;
                    Self::format_ast_node(&arm.body, buf)?;
                    write!(buf, ")")?;
                }
                write!(buf, "])")?;
            }
            Expr::Include(lit) | Expr::Import(lit) => {
                let kind = if matches!(&*node.expr, Expr::Include(_)) {
                    "Include"
                } else {
                    "Import"
                };
                write!(buf, "{}(", kind)?;
                match lit {
                    Literal::String(s) => write!(buf, "\"{}\"", s)?,
                    Literal::Number(n) => write!(buf, "{}", n)?,
                    Literal::Symbol(i) => write!(buf, "{}", i)?,
                    Literal::Bool(b) => write!(buf, "{}", b)?,
                    Literal::None => write!(buf, "None")?,
                }
                write!(buf, ")")?;
            }
            Expr::Module(ident, body) => {
                write!(buf, "Module(\"{}\", [", ident)?;
                for (i, node) in body.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_ast_node(node, buf)?;
                }
                write!(buf, "])")?;
            }
            Expr::QualifiedAccess(path, target) => {
                write!(buf, "QualifiedAccess([")?;
                for (i, ident) in path.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    write!(buf, "\"{}\"", ident)?;
                }
                write!(buf, "], ")?;
                match target {
                    ast::node::AccessTarget::Call(ident, args) => {
                        write!(buf, "Call(\"{}\", [", ident)?;
                        for (i, arg) in args.iter().enumerate() {
                            if i > 0 {
                                write!(buf, ", ")?;
                            }
                            Self::format_ast_node(arg, buf)?;
                        }
                        write!(buf, "])")?;
                    }
                    ast::node::AccessTarget::Ident(ident) => {
                        write!(buf, "Ident(\"{}\")", ident)?;
                    }
                }
                write!(buf, ")")?;
            }
            Expr::Self_ => {
                write!(buf, "Self")?;
            }
            Expr::Nodes => {
                write!(buf, "Nodes")?;
            }
            Expr::Paren(inner) => {
                write!(buf, "Paren(")?;
                Self::format_ast_node(inner, buf)?;
                write!(buf, ")")?;
            }
            Expr::Quote(inner) => {
                write!(buf, "Quote(")?;
                Self::format_ast_node(inner, buf)?;
                write!(buf, ")")?;
            }
            Expr::Unquote(inner) => {
                write!(buf, "Unquote(")?;
                Self::format_ast_node(inner, buf)?;
                write!(buf, ")")?;
            }
            Expr::Try(try_expr, catch_expr) => {
                write!(buf, "Try(")?;
                Self::format_ast_node(try_expr, buf)?;
                write!(buf, ", Catch(")?;
                Self::format_ast_node(catch_expr, buf)?;
                write!(buf, "))")?;
            }
            Expr::Break => {
                write!(buf, "Break")?;
            }
            Expr::Continue => {
                write!(buf, "Continue")?;
            }
        }
        Ok(())
    }

    fn format_pattern(pattern: &ast::node::Pattern, buf: &mut String) -> std::fmt::Result {
        use ast::node::{Literal, Pattern};

        match pattern {
            Pattern::Literal(lit) => {
                write!(buf, "Literal(")?;
                match lit {
                    Literal::String(s) => write!(buf, "String(\"{}\")", s)?,
                    Literal::Number(n) => write!(buf, "Number({})", n)?,
                    Literal::Symbol(i) => write!(buf, "Symbol({})", i)?,
                    Literal::Bool(b) => write!(buf, "Bool({})", b)?,
                    Literal::None => write!(buf, "None")?,
                }
                write!(buf, ")")?;
            }
            Pattern::Ident(ident) => {
                write!(buf, "Ident(\"{}\")", ident)?;
            }
            Pattern::Wildcard => {
                write!(buf, "Wildcard")?;
            }
            Pattern::Array(patterns) => {
                write!(buf, "Array([")?;
                for (i, p) in patterns.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_pattern(p, buf)?;
                }
                write!(buf, "])")?;
            }
            Pattern::ArrayRest(patterns, rest) => {
                write!(buf, "ArrayRest([")?;
                for (i, p) in patterns.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    Self::format_pattern(p, buf)?;
                }
                write!(buf, "], \"{}\")", rest)?;
            }
            Pattern::Dict(entries) => {
                write!(buf, "Dict({{")?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(buf, ", ")?;
                    }
                    write!(buf, "\"{}\": ", key)?;
                    Self::format_pattern(value, buf)?;
                }
                write!(buf, "}})")?;
            }
            Pattern::Type(type_name) => {
                write!(buf, "Type({})", type_name)?;
            }
        }
        Ok(())
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
                                    current_node.apply_fragment(node.clone());
                                    RuntimeValue::Markdown(current_node, selector)
                                } else {
                                    updated_value
                                }
                            } else {
                                updated_value
                            }
                        }
                        RuntimeValue::String(s) => RuntimeValue::Markdown(node.clone().with_value(s), None),
                        RuntimeValue::Symbol(i) => RuntimeValue::Markdown(node.clone().with_value(&i.as_str()), None),
                        RuntimeValue::Boolean(b) => {
                            RuntimeValue::Markdown(node.clone().with_value(b.to_string().as_str()), None)
                        }
                        RuntimeValue::Number(n) => {
                            RuntimeValue::Markdown(node.clone().with_value(n.to_string().as_str()), None)
                        }
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
                                        RuntimeValue::Markdown(node.clone().with_value(v.to_string().as_str()), None),
                                    );
                                }
                            }
                            RuntimeValue::Dict(new_dict)
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
        assert_eq!(format!("{}", RuntimeValue::Boolean(true)), "true");
        assert_eq!(format!("{}", RuntimeValue::Number(Number::from(42.0))), "42");
        assert_eq!(format!("{}", RuntimeValue::String(String::from("test"))), "test");
        assert_eq!(format!("{}", RuntimeValue::None), "");
        let map_val = RuntimeValue::Dict(BTreeMap::default());
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
        let map_val = RuntimeValue::Dict(map);
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
        assert!(RuntimeValue::Boolean(true).is_truthy());
        assert!(!RuntimeValue::Boolean(false).is_truthy());
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
        assert!(RuntimeValue::Array(Vec::new()) < RuntimeValue::Array(vec!["a".to_string().into()]));
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
        assert!(RuntimeValue::Boolean(false) < RuntimeValue::Boolean(true));
        assert!(
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Shared::new(SharedCell::new(Env::default()))
            ) < RuntimeValue::Function(
                smallvec![Param::new(IdentWithToken::new("test"))],
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
        assert_eq!(format!("{:?}", function), "function/0");

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

        let markdown_with_selector = RuntimeValue::Markdown(parent.clone(), Some(Selector::Index(1)));

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

    /// Helper function to create an AST literal node
    fn ast_literal(lit: ast::node::Literal) -> Shared<ast::node::Node> {
        use crate::arena::ArenaId;
        Shared::new(ast::node::Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(ast::node::Expr::Literal(lit)),
        })
    }

    /// Helper function to create an AST ident node
    fn ast_ident(name: &str) -> Shared<ast::node::Node> {
        use crate::arena::ArenaId;
        Shared::new(ast::node::Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(ast::node::Expr::Ident(ast::node::IdentWithToken::new(name))),
        })
    }

    /// Helper function to create an AST node from expr
    fn ast_node(expr: ast::node::Expr) -> Shared<ast::node::Node> {
        use crate::arena::ArenaId;
        Shared::new(ast::node::Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        })
    }

    #[rstest]
    // Literal variants
    #[case::number_literal(ast_literal(ast::node::Literal::Number(Number::from(42.0))), "Literal(Number(42))")]
    #[case::string_literal(
        ast_literal(ast::node::Literal::String("hello".to_string())),
        "Literal(String(\"hello\"))"
    )]
    #[case::bool_literal(ast_literal(ast::node::Literal::Bool(true)), "Literal(Bool(true))")]
    #[case::symbol_literal(ast_literal(ast::node::Literal::Symbol(Ident::new("test"))), "Literal(Symbol(test))")]
    #[case::none_literal(ast_literal(ast::node::Literal::None), "Literal(None)")]
    // Ident
    #[case::ident(ast_ident("x"), "Ident(\"x\")")]
    // Call
    #[case::call(
        ast_node(ast::node::Expr::Call(
            ast::node::IdentWithToken::new("map"),
            smallvec![
                ast_literal(ast::node::Literal::Number(Number::from(1.0))),
                ast_literal(ast::node::Literal::Number(Number::from(2.0))),
            ],
        )),
        "Call(\"map\", [Literal(Number(1)), Literal(Number(2))])"
    )]
    // CallDynamic
    #[case::call_dynamic(
        ast_node(ast::node::Expr::CallDynamic(
            ast_ident("fn"),
            smallvec![
                ast_literal(ast::node::Literal::Number(Number::from(1.0))),
            ],
        )),
        "CallDynamic(Ident(\"fn\"), [Literal(Number(1))])"
    )]
    // Block
    #[case::block(
        ast_node(ast::node::Expr::Block(vec![
            ast_ident("x"),
            ast_literal(ast::node::Literal::Number(Number::from(42.0))),
        ])),
        "Block([Ident(\"x\"), Literal(Number(42))])"
    )]
    // Def
    #[case::def(
        ast_node(ast::node::Expr::Def(
            ast::node::IdentWithToken::new("add"),
            smallvec![Param::new(ast::node::IdentWithToken::new("a")), Param::new(ast::node::IdentWithToken::new("b"))],
            vec![ast_ident("result")],
        )),
        "Def(\"add\", [Ident(\"a\"), Ident(\"b\")], [Ident(\"result\")])"
    )]
    // Macro
    #[case::macro_def(
        ast_node(ast::node::Expr::Macro(
            ast::node::IdentWithToken::new("my_macro"),
            smallvec![Param::new(ast::node::IdentWithToken::new("x"))],
            ast_ident("body"),
        )),
        "Macro(\"my_macro\", [Ident(\"x\")], Ident(\"body\"))"
    )]
    // Fn
    #[case::fn_expr(
        ast_node(ast::node::Expr::Fn(
            smallvec![Param::new(ast::node::IdentWithToken::new("x"))],
            vec![ast_literal(ast::node::Literal::Number(Number::from(1.0)))],
        )),
        "Fn([Ident(\"x\")], [Literal(Number(1))])"
    )]
    // Let, Var, Assign
    #[case::let_binding(
        ast_node(ast::node::Expr::Let(
            ast::node::IdentWithToken::new("x"),
            ast_literal(ast::node::Literal::Number(Number::from(10.0))),
        )),
        "Let(\"x\", Literal(Number(10)))"
    )]
    #[case::var_binding(
        ast_node(ast::node::Expr::Var(
            ast::node::IdentWithToken::new("y"),
            ast_literal(ast::node::Literal::Number(Number::from(5.0))),
        )),
        "Var(\"y\", Literal(Number(5)))"
    )]
    #[case::assign(
        ast_node(ast::node::Expr::Assign(
            ast::node::IdentWithToken::new("z"),
            ast_literal(ast::node::Literal::Number(Number::from(3.0))),
        )),
        "Assign(\"z\", Literal(Number(3)))"
    )]
    // Binary operators
    #[case::and_op(
        ast_node(ast::node::Expr::And(
            ast_literal(ast::node::Literal::Bool(true)),
            ast_literal(ast::node::Literal::Bool(false)),
        )),
        "And(Literal(Bool(true)), Literal(Bool(false)))"
    )]
    #[case::or_op(
        ast_node(ast::node::Expr::Or(
            ast_literal(ast::node::Literal::Bool(true)),
            ast_literal(ast::node::Literal::Bool(false)),
        )),
        "Or(Literal(Bool(true)), Literal(Bool(false)))"
    )]
    // InterpolatedString
    #[case::interpolated_string(
        ast_node(ast::node::Expr::InterpolatedString(vec![
            ast::node::StringSegment::Text("Hello ".to_string()),
            ast::node::StringSegment::Expr(ast_ident("name")),
            ast::node::StringSegment::Env(smol_str::SmolStr::new("HOME")),
            ast::node::StringSegment::Self_,
        ])),
        "InterpolatedString([Text(\"Hello \"), Expr(Ident(\"name\")), Env(\"HOME\"), Self])"
    )]
    // Selector
    #[case::selector(
        ast_node(ast::node::Expr::Selector(crate::selector::Selector::Heading(Some(1)))),
        "Selector(Heading(Some(1)))"
    )]
    // While
    #[case::while_loop(
        ast_node(ast::node::Expr::While(
            ast_literal(ast::node::Literal::Bool(true)),
            vec![ast_ident("body")],
        )),
        "While(Literal(Bool(true)), [Ident(\"body\")])"
    )]
    // Foreach
    #[case::foreach_loop(
        ast_node(ast::node::Expr::Foreach(
            ast::node::IdentWithToken::new("item"),
            ast_ident("items"),
            vec![ast_ident("process")],
        )),
        "Foreach(\"item\", Ident(\"items\"), [Ident(\"process\")])"
    )]
    // If
    #[case::if_expr(
        ast_node(ast::node::Expr::If(smallvec![
            (
                Some(ast_literal(ast::node::Literal::Bool(true))),
                ast_literal(ast::node::Literal::Number(Number::from(1.0)))
            ),
            (
                None,
                ast_literal(ast::node::Literal::Number(Number::from(2.0)))
            ),
        ])),
        "If([Branch(Literal(Bool(true)), Literal(Number(1))), Branch(None, Literal(Number(2)))])"
    )]
    // Match
    #[case::match_expr(
        ast_node(ast::node::Expr::Match(
            ast_ident("x"),
            smallvec![
                ast::node::MatchArm {
                    pattern: ast::node::Pattern::Literal(ast::node::Literal::Number(Number::from(1.0))),
                    guard: None,
                    body: ast_literal(ast::node::Literal::String("one".to_string())),
                },
                ast::node::MatchArm {
                    pattern: ast::node::Pattern::Wildcard,
                    guard: Some(ast_literal(ast::node::Literal::Bool(true))),
                    body: ast_literal(ast::node::Literal::String("other".to_string())),
                },
            ],
        )),
        "Match(Ident(\"x\"), [Arm(Literal(Number(1)), Literal(String(\"one\"))), Arm(Wildcard, Guard(Literal(Bool(true))), Literal(String(\"other\")))])"
    )]
    // Include and Import
    #[case::include_expr(
        ast_node(ast::node::Expr::Include(ast::node::Literal::String("lib.mq".to_string()))),
        "Include(\"lib.mq\")"
    )]
    #[case::import_expr(
        ast_node(ast::node::Expr::Import(ast::node::Literal::String("module.mq".to_string()))),
        "Import(\"module.mq\")"
    )]
    // Module
    #[case::module_expr(
        ast_node(ast::node::Expr::Module(
            ast::node::IdentWithToken::new("MyModule"),
            vec![ast_ident("export")],
        )),
        "Module(\"MyModule\", [Ident(\"export\")])"
    )]
    // QualifiedAccess
    #[case::qualified_access_ident(
        ast_node(ast::node::Expr::QualifiedAccess(
            vec![
                ast::node::IdentWithToken::new("Module"),
                ast::node::IdentWithToken::new("SubModule"),
            ],
            ast::node::AccessTarget::Ident(ast::node::IdentWithToken::new("value")),
        )),
        "QualifiedAccess([\"Module\", \"SubModule\"], Ident(\"value\"))"
    )]
    #[case::qualified_access_call(
        ast_node(ast::node::Expr::QualifiedAccess(
            vec![ast::node::IdentWithToken::new("Module")],
            ast::node::AccessTarget::Call(
                ast::node::IdentWithToken::new("fn"),
                smallvec![ast_literal(ast::node::Literal::Number(Number::from(1.0)))],
            ),
        )),
        "QualifiedAccess([\"Module\"], Call(\"fn\", [Literal(Number(1))]))"
    )]
    // Quote and Unquote
    #[case::quote(ast_node(ast::node::Expr::Quote(ast_ident("x"))), "Quote(Ident(\"x\"))")]
    #[case::unquote(ast_node(ast::node::Expr::Unquote(ast_ident("y"))), "Unquote(Ident(\"y\"))")]
    // Try
    #[case::try_catch(
        ast_node(ast::node::Expr::Try(ast_ident("risky"), ast_ident("fallback"),)),
        "Try(Ident(\"risky\"), Catch(Ident(\"fallback\")))"
    )]
    // Simple expressions
    #[case::nodes(ast_node(ast::node::Expr::Nodes), "Nodes")]
    #[case::self_expr(ast_node(ast::node::Expr::Self_), "Self")]
    #[case::break_stmt(ast_node(ast::node::Expr::Break), "Break")]
    #[case::continue_stmt(ast_node(ast::node::Expr::Continue), "Continue")]
    // Paren
    #[case::paren(ast_node(ast::node::Expr::Paren(ast_ident("x"))), "Paren(Ident(\"x\"))")]
    fn test_ast_display(#[case] node: Shared<ast::node::Node>, #[case] expected: &str) {
        let ast_value = RuntimeValue::Ast(node);
        assert_eq!(format!("{}", ast_value), expected);
    }
}
