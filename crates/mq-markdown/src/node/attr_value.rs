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
    #[cfg(feature = "callout")]
    pub(crate) const KIND: &str = "kind";
    pub(crate) const ROW: &str = "row";
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;
    use rstest::rstest;

    #[rstest]
    #[case("hello".to_string(), AttrValue::String("hello".to_string()))]
    #[case("".to_string(), AttrValue::String("".to_string()))]
    fn test_from_string(#[case] input: String, #[case] expected: AttrValue) {
        assert_eq!(AttrValue::from(input), expected);
    }

    #[rstest]
    #[case("world", AttrValue::String("world".to_string()))]
    #[case("", AttrValue::String("".to_string()))]
    fn test_from_str(#[case] input: &str, #[case] expected: AttrValue) {
        assert_eq!(AttrValue::from(input), expected);
    }

    #[rstest]
    #[case(1.5f64, AttrValue::Number(1.5))]
    #[case(0.0f64, AttrValue::Number(0.0))]
    #[case(-1.5f64, AttrValue::Number(-1.5))]
    fn test_from_f64(#[case] input: f64, #[case] expected: AttrValue) {
        assert_eq!(AttrValue::from(input), expected);
    }

    #[rstest]
    #[case(42i64, AttrValue::Integer(42))]
    #[case(0i64, AttrValue::Integer(0))]
    #[case(-1i64, AttrValue::Integer(-1))]
    fn test_from_i64(#[case] input: i64, #[case] expected: AttrValue) {
        assert_eq!(AttrValue::from(input), expected);
    }

    #[rstest]
    #[case(7i32, AttrValue::Integer(7))]
    #[case(-3i32, AttrValue::Integer(-3))]
    fn test_from_i32(#[case] input: i32, #[case] expected: AttrValue) {
        assert_eq!(AttrValue::from(input), expected);
    }

    #[rstest]
    #[case(0usize, AttrValue::Integer(0))]
    #[case(100usize, AttrValue::Integer(100))]
    fn test_from_usize(#[case] input: usize, #[case] expected: AttrValue) {
        assert_eq!(AttrValue::from(input), expected);
    }

    #[rstest]
    #[case(0u8, AttrValue::Integer(0))]
    #[case(255u8, AttrValue::Integer(255))]
    fn test_from_u8(#[case] input: u8, #[case] expected: AttrValue) {
        assert_eq!(AttrValue::from(input), expected);
    }

    #[rstest]
    #[case(true, AttrValue::Boolean(true))]
    #[case(false, AttrValue::Boolean(false))]
    fn test_from_bool(#[case] input: bool, #[case] expected: AttrValue) {
        assert_eq!(AttrValue::from(input), expected);
    }

    #[rstest]
    #[case(AttrValue::String("hi".to_string()), "hi")]
    #[case(AttrValue::Number(1.5), "1.5")]
    #[case(AttrValue::Integer(-3), "-3")]
    #[case(AttrValue::Boolean(true), "true")]
    #[case(AttrValue::Boolean(false), "false")]
    #[case(AttrValue::Null, "")]
    fn test_as_string(#[case] input: AttrValue, #[case] expected: &str) {
        assert_eq!(input.as_string(), expected);
    }

    #[test]
    fn test_as_string_array() {
        assert_eq!(AttrValue::Array(vec![]).as_string(), "");
        assert_eq!(AttrValue::Array(vec![Node::Empty]).as_string(), "");
    }

    #[rstest]
    #[case(AttrValue::String("42".to_string()), Some(42i64))]
    #[case(AttrValue::String("-5".to_string()), Some(-5i64))]
    #[case(AttrValue::String("bad".to_string()), None)]
    #[case(AttrValue::Number(9.9), Some(9i64))]
    #[case(AttrValue::Integer(100), Some(100i64))]
    #[case(AttrValue::Boolean(true), Some(1i64))]
    #[case(AttrValue::Boolean(false), Some(0i64))]
    #[case(AttrValue::Null, None)]
    fn test_as_i64(#[case] input: AttrValue, #[case] expected: Option<i64>) {
        assert_eq!(input.as_i64(), expected);
    }

    #[test]
    fn test_as_i64_array() {
        assert_eq!(AttrValue::Array(vec![]).as_i64(), Some(0));
        assert_eq!(AttrValue::Array(vec![Node::Empty, Node::Empty]).as_i64(), Some(2));
    }

    #[rstest]
    #[case(AttrValue::String("1.5".to_string()), Some(1.5f64))]
    #[case(AttrValue::String("bad".to_string()), None)]
    #[case(AttrValue::Number(2.5), Some(2.5f64))]
    #[case(AttrValue::Integer(5), Some(5.0f64))]
    #[case(AttrValue::Boolean(true), None)]
    #[case(AttrValue::Null, None)]
    fn test_as_f64(#[case] input: AttrValue, #[case] expected: Option<f64>) {
        assert_eq!(input.as_f64(), expected);
    }

    #[test]
    fn test_as_f64_array() {
        assert_eq!(AttrValue::Array(vec![]).as_f64(), Some(0.0));
        assert_eq!(AttrValue::Array(vec![Node::Empty]).as_f64(), Some(1.0));
    }

    #[rstest]
    #[case(AttrValue::String("s".to_string()), true, false, false, false, false)]
    #[case(AttrValue::Number(1.0), false, true, false, false, false)]
    #[case(AttrValue::Integer(1), false, false, true, false, false)]
    #[case(AttrValue::Boolean(false), false, false, false, true, false)]
    #[case(AttrValue::Null, false, false, false, false, true)]
    fn test_type_predicates(
        #[case] value: AttrValue,
        #[case] is_str: bool,
        #[case] is_num: bool,
        #[case] is_int: bool,
        #[case] is_bool: bool,
        #[case] is_null: bool,
    ) {
        assert_eq!(value.is_string(), is_str);
        assert_eq!(value.is_number(), is_num);
        assert_eq!(value.is_integer(), is_int);
        assert_eq!(value.is_boolean(), is_bool);
        assert_eq!(value.is_null(), is_null);
    }
}
