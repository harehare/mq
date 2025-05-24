use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::HashMap,
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    rc::Rc,
};

use crate::{AstIdent, AstParams, Program, Value, number::Number};

use mq_markdown::Node;

use super::env::Env;

#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Index(usize),
}

#[derive(Clone)] // PartialEq will be implemented manually
pub enum RuntimeValue {
    Number(Number),
    Bool(bool),
    String(String),
    Array(Vec<RuntimeValue>),
    Markdown(Node, Option<Selector>),
    Function(AstParams, Program, Rc<RefCell<Env>>),
    NativeFunction(AstIdent),
    Map(Rc<RefCell<HashMap<RuntimeValue, RuntimeValue>>>),
    None,
}

impl Hash for RuntimeValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state); // Hash the enum variant
        match self {
            RuntimeValue::Number(n) => n.hash(state),
            RuntimeValue::Bool(b) => b.hash(state),
            RuntimeValue::String(s) => s.hash(state),
            RuntimeValue::None => ().hash(state), // None variant has no data to hash beyond discriminant
            // For other types, we make them unhashable for now.
            // Attempting to use them as HashMap keys will fail at runtime if not prevented earlier.
            // This aligns with the "Simplification for first pass"
            RuntimeValue::Array(_)
            | RuntimeValue::Markdown(_, _)
            | RuntimeValue::Function(_, _, _)
            | RuntimeValue::NativeFunction(_)
            | RuntimeValue::Map(_) => {
                // Or simply do nothing, making them effectively not produce a useful hash.
                // For safety, let's ensure a fixed "unhashable" value or panic if these are hashed.
                // For now, just hashing the discriminant is a minimal approach.
                // A more robust solution would be to panic or ensure these types
                // are never passed to a context requiring Hash if they aren't truly hashable.
                // Let's use a fixed arbitrary value for unhashable types for now,
                // but ideally, the type system or runtime checks should prevent hashing these.
                0u8.hash(state); // Arbitrary fixed hash for unhashable types
            }
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

impl From<Number> for RuntimeValue {
    fn from(n: Number) -> Self {
        RuntimeValue::Number(n)
    }
}

impl From<Vec<RuntimeValue>> for RuntimeValue {
    fn from(arr: Vec<RuntimeValue>) -> Self {
        RuntimeValue::Array(arr)
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
            // Value::Map is not defined in Value enum yet, so cannot convert from it.
            Value::None => RuntimeValue::None,
        }
    }
}

impl PartialEq for RuntimeValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a == b,
            (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a == b,
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
            (RuntimeValue::Array(a), RuntimeValue::Array(b)) => a == b,
            (RuntimeValue::Markdown(a, sel_a), RuntimeValue::Markdown(b, sel_b)) => {
                a.value() == b.value() && sel_a == sel_b // Simple comparison for now
            }
            (RuntimeValue::Function(p1, prog1, _), RuntimeValue::Function(p2, prog2, _)) => {
                // Functions are equal if their params and program are equal. Environment is not compared.
                p1 == p2 && prog1 == prog2
            }
            (RuntimeValue::NativeFunction(id1), RuntimeValue::NativeFunction(id2)) => id1 == id2,
            (RuntimeValue::Map(a), RuntimeValue::Map(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let map_a = a.borrow();
                let map_b = b.borrow();
                if map_a.len() != map_b.len() {
                    return false;
                }
                for (key, value_a) in map_a.iter() {
                    match map_b.get(key) {
                        Some(value_b) => {
                            if value_a != value_b {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                true
            }
            (RuntimeValue::None, RuntimeValue::None) => true,
            _ => false, // Different types are not equal
        }
    }
}

impl Eq for RuntimeValue {} // Marker trait, relies on PartialEq

impl PartialOrd for RuntimeValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a.partial_cmp(b),
            (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a.partial_cmp(b),
            (RuntimeValue::String(a), RuntimeValue::String(b)) => a.partial_cmp(b),
            (RuntimeValue::Array(a), RuntimeValue::Array(b)) => a.partial_cmp(b),
            (RuntimeValue::Markdown(a, _), RuntimeValue::Markdown(b, _)) => {
                // Convert to string for comparison, consistent with previous logic
                a.to_string().partial_cmp(&b.to_string())
            }
            (RuntimeValue::Function(a1, b1, _), RuntimeValue::Function(a2, b2, _)) => {
                // Compare functions based on params and program structure
                a1.partial_cmp(a2).and_then(|ord| {
                    if ord == Ordering::Equal {
                        b1.partial_cmp(b2)
                    } else {
                        Some(ord)
                    }
                })
            }
            // Maps and other types like NativeFunction, None are not typically ordered
            // Or their ordering is not meaningful in the same way as primitives/collections.
            // Returning None for these cases.
            (RuntimeValue::Map(_), RuntimeValue::Map(_)) => None, // Maps are not ordered
            _ => None, // Different types or unorderable types
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
            RuntimeValue::Map(map_rc) => {
                let map = map_rc.borrow();
                let entries = map
                    .iter()
                    .map(|(k, v)| format!("{:?}: {:?}", k, v)) // Use Debug for keys/values in Debug output
                    .collect::<Vec<String>>()
                    .join(", ");
                write!(f, "Map {{ {} }}", entries)
            }
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
    pub const EMPTY_ARRAY: RuntimeValue = Self::Array(Vec::new());

    pub fn name(&self) -> &str {
        match self {
            RuntimeValue::Number(_) => "number",
            RuntimeValue::Bool(_) => "bool",
            RuntimeValue::String(_) => "string",
            RuntimeValue::Markdown(_, _) => "markdown",
            RuntimeValue::Array(_) => "array",
            RuntimeValue::Map(_) => "map",
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
            RuntimeValue::Map(map_rc) => {
                // Consistent with string() method for Map
                let map = map_rc.borrow();
                let entries = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.string(), v.string()))
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("Map {{ {} }}", entries)
            }
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

    pub fn is_empty(&self) -> bool {
        match self {
            RuntimeValue::Array(a) => a.is_empty(),
            RuntimeValue::String(s) => s.is_empty(),
            RuntimeValue::Markdown(m, _) => m.value().is_empty(),
            RuntimeValue::Map(map_rc) => map_rc.borrow().is_empty(),
            RuntimeValue::None => true,
            _ => false, // Numbers, bools, functions are not considered "empty" in this sense
        }
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
            RuntimeValue::Map(map_rc) => !map_rc.borrow().is_empty(),
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
            RuntimeValue::Map(map_rc) => map_rc.borrow().len(),
            RuntimeValue::Markdown(m, _) => m.value().len(),
            RuntimeValue::None => 0,
            RuntimeValue::Function(..) => 0, // Or perhaps 1, depending on desired semantics
            RuntimeValue::NativeFunction(..) => 0, // Or perhaps 1
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
            RuntimeValue::Map(map_rc) => {
                let map = map_rc.borrow();
                if map.is_empty() {
                    return "Map { }".to_string();
                }
                let entries = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.string(), v.string()))
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("Map {{ {} }}", entries)
            }
            RuntimeValue::Markdown(m, _) => m.to_string(),
            RuntimeValue::None => "None".to_string(),
            RuntimeValue::Function(_, _, _) => "function".to_string(),
            RuntimeValue::NativeFunction(_) => "native_function".to_string(),
        }
    }
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use crate::{AstExpr, AstNode, arena::ArenaId};
    use rstest::rstest;
    use smallvec::{SmallVec, smallvec};
    use std::hash::{DefaultHasher, Hash, Hasher};

    use super::*;

    fn calculate_hash<T: Hash>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    }

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
    #[case(RuntimeValue::Map(Rc::new(RefCell::new(HashMap::new()))), "Map { }")]
    #[case(RuntimeValue::Map(Rc::new(RefCell::new({
        let mut map = HashMap::new();
        map.insert(RuntimeValue::String("key".to_string()), RuntimeValue::Number(1.into()));
        map
    }))), "Map { key: 1 }")]
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
        assert_eq!(format!("{}", RuntimeValue::Map(Rc::new(RefCell::new(HashMap::new())))), "Map { }");
        let mut map = HashMap::new();
        map.insert(RuntimeValue::String("a".to_string()), RuntimeValue::Number(1.into()));
        map.insert(RuntimeValue::Number(2.into()), RuntimeValue::Bool(true));
        // Note: Order in Display for HashMap is not guaranteed.
        let map_val = RuntimeValue::Map(Rc::new(RefCell::new(map)));
        let display_str = format!("{}", map_val);
        assert!(display_str.starts_with("Map { "));
        assert!(display_str.contains("a: 1"));
        assert!(display_str.contains("2: true"));
        assert!(display_str.ends_with(" }"));
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
            "\"test\"" // Debug for String includes quotes
        );
        assert_eq!(format!("{:?}", RuntimeValue::None), "None");
        assert_eq!(format!("{:?}", RuntimeValue::Map(Rc::new(RefCell::new(HashMap::new())))), "Map { }");

        let mut map = HashMap::new();
        map.insert(RuntimeValue::String("key".to_string()), RuntimeValue::Number(123.into()));
        let map_val = RuntimeValue::Map(Rc::new(RefCell::new(map)));
        assert_eq!(format!("{:?}", map_val), "Map { \"key\": 123 }"); // Debug for keys/values
    }


    #[test]
    fn test_runtime_value_name() {
        assert_eq!(RuntimeValue::Bool(true).name(), "bool");
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).name(), "number");
        assert_eq!(RuntimeValue::String(String::from("test")).name(), "string");
        assert_eq!(RuntimeValue::Map(Default::default()).name(), "map");
        assert_eq!(RuntimeValue::None.name(), "None");
        assert_eq!(
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            )
            .name(),
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
        assert_eq!(RuntimeValue::Map(Rc::new(RefCell::new(HashMap::new()))).text(), "Map { }");
        let mut map = HashMap::new();
        map.insert(RuntimeValue::String("key".to_string()), RuntimeValue::String("value".to_string()));
        let map_val = RuntimeValue::Map(Rc::new(RefCell::new(map)));
        assert_eq!(map_val.text(), "Map { key: value }"); // Order might vary
        assert_eq!(RuntimeValue::None.text(), "None");
        assert_eq!(
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            )
            .text(),
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
        assert!(!RuntimeValue::Array(Vec::new()).is_true());
        assert!(!RuntimeValue::Map(Rc::new(RefCell::new(HashMap::new()))).is_true());
        let mut map = HashMap::new();
        map.insert(RuntimeValue::String("key".to_string()), RuntimeValue::Number(1.into()));
        assert!(RuntimeValue::Map(Rc::new(RefCell::new(map))).is_true());
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
        assert!(!RuntimeValue::None.is_true());
        assert!(RuntimeValue::NativeFunction(AstIdent::new("name")).is_true());
        assert!(
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            )
            .is_true()
        );
    }

    #[test]
    fn test_runtime_value_partial_ord() {
        // Only testing orderable types based on current impl
        assert!(RuntimeValue::Number(Number::from(1.0)) < RuntimeValue::Number(Number::from(2.0)));
        assert!(RuntimeValue::String(String::from("a")) < RuntimeValue::String(String::from("b")));
        assert!(RuntimeValue::Bool(false) < RuntimeValue::Bool(true));
        // Array comparison
        assert!(RuntimeValue::Array(vec![RuntimeValue::Number(1.into())]) < RuntimeValue::Array(vec![RuntimeValue::Number(2.into())]));
        // Markdown comparison
        let md1 = RuntimeValue::Markdown(mq_markdown::Node::from("a"), None);
        let md2 = RuntimeValue::Markdown(mq_markdown::Node::from("b"), None);
        assert!(md1 < md2);

        // Ensure non-orderable types (like Map) return None
        let map1 = RuntimeValue::Map(Rc::new(RefCell::new(HashMap::new())));
        let map2 = RuntimeValue::Map(Rc::new(RefCell::new(HashMap::new())));
        assert_eq!(map1.partial_cmp(&map2), None);
        assert_eq!(RuntimeValue::Number(1.into()).partial_cmp(&map1), None); // Different types
    }

    #[test]
    fn test_runtime_value_partial_eq() {
        assert_eq!(RuntimeValue::Number(1.into()), RuntimeValue::Number(1.into()));
        assert_ne!(RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()));
        assert_eq!(RuntimeValue::String("a".into()), RuntimeValue::String("a".into()));
        assert_ne!(RuntimeValue::String("a".into()), RuntimeValue::String("b".into()));
        assert_eq!(RuntimeValue::Bool(true), RuntimeValue::Bool(true));
        assert_ne!(RuntimeValue::Bool(true), RuntimeValue::Bool(false));
        assert_eq!(RuntimeValue::None, RuntimeValue::None);
        assert_ne!(RuntimeValue::Number(1.into()), RuntimeValue::String("1".into()));

        let map1 = RuntimeValue::Map(Rc::new(RefCell::new({
            let mut m = HashMap::new();
            m.insert(RuntimeValue::String("key1".into()), RuntimeValue::Number(1.into()));
            m.insert(RuntimeValue::String("key2".into()), RuntimeValue::Bool(true));
            m
        })));
        let map2 = RuntimeValue::Map(Rc::new(RefCell::new({
            let mut m = HashMap::new();
            m.insert(RuntimeValue::String("key2".into()), RuntimeValue::Bool(true));
            m.insert(RuntimeValue::String("key1".into()), RuntimeValue::Number(1.into()));
            m
        })));
        let map3 = RuntimeValue::Map(Rc::new(RefCell::new({
            let mut m = HashMap::new();
            m.insert(RuntimeValue::String("key1".into()), RuntimeValue::Number(1.into()));
            m.insert(RuntimeValue::String("key2".into()), RuntimeValue::Bool(false)); // Different value
            m
        })));
        let map4 = RuntimeValue::Map(Rc::new(RefCell::new({
            let mut m = HashMap::new();
            m.insert(RuntimeValue::String("key1".into()), RuntimeValue::Number(1.into()));
            // map4 is missing key2
            m
        })));

        assert_eq!(map1, map2); // Same key-value pairs, different insertion order
        assert_ne!(map1, map3); // Different value for key2
        assert_ne!(map1, map4); // map4 missing a key
        assert_ne!(map1, RuntimeValue::Number(1.into())); // Different type
    }

    #[test]
    fn test_runtime_value_hash() {
        // Test that equal primitive values have the same hash
        assert_eq!(calculate_hash(&RuntimeValue::Number(123.into())), calculate_hash(&RuntimeValue::Number(123.into())));
        assert_eq!(calculate_hash(&RuntimeValue::String("test".into())), calculate_hash(&RuntimeValue::String("test".into())));
        assert_eq!(calculate_hash(&RuntimeValue::Bool(true)), calculate_hash(&RuntimeValue::Bool(true)));
        assert_eq!(calculate_hash(&RuntimeValue::None), calculate_hash(&RuntimeValue::None));

        // Test that different primitive values have different hashes (highly probable)
        assert_ne!(calculate_hash(&RuntimeValue::Number(1.into())), calculate_hash(&RuntimeValue::Number(2.into())));
        assert_ne!(calculate_hash(&RuntimeValue::String("a".into())), calculate_hash(&RuntimeValue::String("b".into())));
        assert_ne!(calculate_hash(&RuntimeValue::Bool(true)), calculate_hash(&RuntimeValue::Bool(false)));

        // Test that different types with same underlying representation (if any) have different hashes due to discriminant
        // e.g. Number(0) vs Bool(false) - their direct value might be 0, but type differs.
        // The current hash impl hashes discriminant first, so this should hold.
        assert_ne!(calculate_hash(&RuntimeValue::Number(0.into())), calculate_hash(&RuntimeValue::Bool(false)));

        // Unhashable types (Array, Map, Function, etc.) currently hash to a fixed value (0u8) + discriminant
        // This means two different arrays will have the same hash if this fixed value approach is taken.
        // This is a simplification as per instructions.
        let arr1_hash = calculate_hash(&RuntimeValue::Array(vec![RuntimeValue::Number(1.into())]));
        let arr2_hash = calculate_hash(&RuntimeValue::Array(vec![RuntimeValue::Number(2.into())]));
        let map_hash = calculate_hash(&RuntimeValue::Map(Default::default()));
        // Depending on exact fixed hash for "unhashable part", these might be equal or not.
        // The critical part is that String, Number, Bool are properly hashable for map keys.
        // For instance, if 0u8.hash(state) is used for all unhashable content parts:
        // hash(Array) might != hash(Map) because discriminant is different.
        // hash(Array([1])) might == hash(Array([2])) if only discriminant and fixed value are hashed.
        // This is acceptable under the "Simplification for first pass".
    }


    #[test]
    fn test_runtime_value_len() {
        assert_eq!(RuntimeValue::Number(Number::from(42.0)).len(), 42);
        assert_eq!(RuntimeValue::String(String::from("test")).len(), 4);
        assert_eq!(RuntimeValue::Bool(true).len(), 1);
        assert_eq!(RuntimeValue::Array(vec![RuntimeValue::None]).len(), 1);
        assert_eq!(RuntimeValue::Map(Rc::new(RefCell::new(HashMap::new()))).len(), 0);
        let mut map = HashMap::new();
        map.insert(RuntimeValue::String("k".into()), RuntimeValue::Number(1.into()));
        assert_eq!(RuntimeValue::Map(Rc::new(RefCell::new(map))).len(), 1);
        assert!(
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            ) < RuntimeValue::Function(
                smallvec![Rc::new(AstNode {
                    expr: Rc::new(AstExpr::Ident(AstIdent::new("test"))),
                    token_id: ArenaId::new(0),
                })],
                Vec::new(),
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

        let function = RuntimeValue::Function(
            SmallVec::new(),
            Vec::new(),
            Rc::new(RefCell::new(Env::default())),
        );
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

        let fn_value = Value::Function(SmallVec::new(), Vec::new());
        assert_eq!(
            RuntimeValue::from(fn_value),
            RuntimeValue::Function(
                SmallVec::new(),
                Vec::new(),
                Rc::new(RefCell::new(Env::default()))
            )
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
}
