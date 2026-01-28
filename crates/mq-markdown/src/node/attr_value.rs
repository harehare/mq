use crate::{Node, RenderOptions, node::values_to_string};

pub mod attr_keys {
    pub(crate) const IDENT: &str = "ident";
    pub(crate) const LABEL: &str = "label";
    pub(crate) const NAME: &str = "name";
    pub(crate) const VALUE: &str = "value";
    pub(crate) const VALUES: &str = "values";
    pub(crate) const CHILDREN: &str = "children";
    pub(crate) const TITLE: &str = "title";
    pub(crate) const URL: &str = "url";
    pub(crate) const ALT: &str = "alt";
    pub(crate) const LANG: &str = "lang";
    pub(crate) const META: &str = "meta";
    pub(crate) const FENCE: &str = "fence";
    pub(crate) const DEPTH: &str = "depth";
    pub(crate) const LEVEL: &str = "level";
    pub(crate) const INDEX: &str = "index";
    pub(crate) const ALIGN: &str = "align";
    pub(crate) const ORDERED: &str = "ordered";
    pub(crate) const CHECKED: &str = "checked";
    pub(crate) const COLUMN: &str = "column";
    pub(crate) const ROW: &str = "row";
    pub(crate) const LAST_CELL_IN_ROW: &str = "last_cell_in_row";
    pub(crate) const LAST_CELL_OF_IN_TABLE: &str = "last_cell_of_in_table";
}

/// Represents a typed attribute value that can be returned from or passed to attr/set_attr methods.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize), serde(untagged))]
pub enum AttrValue {
    Array(Vec<Node>),
    String(String),
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Null,
}

impl From<String> for AttrValue {
    fn from(s: String) -> Self {
        AttrValue::String(s)
    }
}

impl From<&str> for AttrValue {
    fn from(s: &str) -> Self {
        AttrValue::String(s.to_string())
    }
}

impl From<f64> for AttrValue {
    fn from(n: f64) -> Self {
        AttrValue::Number(n)
    }
}

impl From<i64> for AttrValue {
    fn from(n: i64) -> Self {
        AttrValue::Integer(n)
    }
}

impl From<i32> for AttrValue {
    fn from(n: i32) -> Self {
        AttrValue::Integer(n as i64)
    }
}

impl From<usize> for AttrValue {
    fn from(n: usize) -> Self {
        AttrValue::Integer(n as i64)
    }
}

impl From<u8> for AttrValue {
    fn from(n: u8) -> Self {
        AttrValue::Integer(n as i64)
    }
}

impl From<bool> for AttrValue {
    fn from(b: bool) -> Self {
        AttrValue::Boolean(b)
    }
}

impl AttrValue {
    /// Converts the attribute value to a string representation.
    pub fn as_string(&self) -> String {
        match self {
            AttrValue::String(s) => s.clone(),
            AttrValue::Number(n) => n.to_string(),
            AttrValue::Integer(i) => i.to_string(),
            AttrValue::Boolean(b) => b.to_string(),
            AttrValue::Array(arr) => values_to_string(arr, &RenderOptions::default()),
            AttrValue::Null => String::new(),
        }
    }

    /// Converts the attribute value to an integer representation.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            AttrValue::String(s) => s.parse().ok(),
            AttrValue::Number(n) => Some(*n as i64),
            AttrValue::Integer(i) => Some(*i),
            AttrValue::Boolean(b) => Some(*b as i64),
            AttrValue::Array(arr) => Some(arr.len() as i64),
            AttrValue::Null => None,
        }
    }

    /// Converts the attribute value to a number representation.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            AttrValue::String(s) => s.parse().ok(),
            AttrValue::Number(n) => Some(*n),
            AttrValue::Integer(i) => Some(*i as f64),
            AttrValue::Boolean(_) => None,
            AttrValue::Array(arr) => Some(arr.len() as f64),
            AttrValue::Null => None,
        }
    }

    /// Returns `true` if the attribute value is a string.
    pub fn is_string(&self) -> bool {
        matches!(self, AttrValue::String(_))
    }

    /// Returns `true` if the attribute value is a number.
    pub fn is_number(&self) -> bool {
        matches!(self, AttrValue::Number(_))
    }

    /// Returns `true` if the attribute value is an integer.
    pub fn is_integer(&self) -> bool {
        matches!(self, AttrValue::Integer(_))
    }

    /// Returns `true` if the attribute value is a boolean.
    pub fn is_boolean(&self) -> bool {
        matches!(self, AttrValue::Boolean(_))
    }

    /// Returns `true` if the attribute value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, AttrValue::Null)
    }
}
