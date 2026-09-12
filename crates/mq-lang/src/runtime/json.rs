//! JSON decoding into mq runtime values.

use crate::runtime::runtime_value::RuntimeValue;
use crate::{DictMap, Ident, Shared};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};

/// Decodes one complete JSON document directly into a runtime value.
///
/// This avoids constructing an intermediate `serde_json::Value` tree before converting it to mq
/// values. Object entries are accumulated directly into a `DictMap` to preserve document order
/// (`RuntimeValue::Dict`'s own contract), with duplicate keys keeping their first position and
/// last value.
pub(super) fn parse_json_runtime_value(input: &str) -> Result<RuntimeValue, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = JsonRuntimeValueSeed.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

/// A serde seed that decodes JSON directly into [`RuntimeValue`].
struct JsonRuntimeValueSeed;

impl<'de> DeserializeSeed<'de> for JsonRuntimeValueSeed {
    type Value = RuntimeValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(JsonRuntimeValueVisitor)
    }
}

/// A visitor for decoding individual JSON values into mq runtime values.
struct JsonRuntimeValueVisitor;

impl<'de> Visitor<'de> for JsonRuntimeValueVisitor {
    type Value = RuntimeValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RuntimeValue::NONE)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RuntimeValue::Boolean(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RuntimeValue::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RuntimeValue::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RuntimeValue::Number(value.into()))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RuntimeValue::String(Shared::new(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(RuntimeValue::String(Shared::new(value)))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or_default());
        while let Some(value) = sequence.next_element_seed(JsonRuntimeValueSeed)? {
            values.push(value);
        }
        Ok(RuntimeValue::Array(Shared::new(values)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = DictMap::default();
        while let Some(key) = map.next_key::<String>()? {
            values.insert(Ident::new(&key), map.next_value_seed(JsonRuntimeValueSeed)?);
        }
        Ok(RuntimeValue::Dict(Shared::new(values)))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_json_runtime_value;
    use crate::Ident;
    use crate::runtime::runtime_value::RuntimeValue;
    use rstest::rstest;

    /// `RuntimeValue::Dict`'s `PartialEq` compares as a set (order-independent, see
    /// `IndexMap`'s own contract), so key order must be asserted separately from value equality.
    fn dict_keys(value: &RuntimeValue) -> Vec<String> {
        let RuntimeValue::Dict(map) = value else {
            panic!("expected a dict, got {value:?}");
        };
        map.keys().map(|k| k.to_string()).collect()
    }

    #[rstest]
    #[case::already_sorted(r#"{"a": 1, "z": 2}"#, &["a", "z"])]
    #[case::reverse_sorted(r#"{"z": 1, "a": 2}"#, &["z", "a"])]
    #[case::nested_object(r#"{"z": {"b": 1, "a": 2}, "a": 3}"#, &["z", "a"])]
    fn preserves_document_key_order(#[case] json: &str, #[case] expected_order: &[&str]) {
        let value = parse_json_runtime_value(json).unwrap();
        assert_eq!(dict_keys(&value), expected_order);
    }

    #[test]
    fn duplicate_keys_keep_their_first_position_and_last_value() {
        let value = parse_json_runtime_value(r#"{"z": 1, "a": 2, "z": 3}"#).unwrap();

        assert_eq!(
            dict_keys(&value),
            vec!["z", "a"],
            "the first occurrence's position wins"
        );
        let RuntimeValue::Dict(map) = &value else {
            unreachable!()
        };
        assert_eq!(
            map.get(&Ident::new("z")),
            Some(&RuntimeValue::Number(3.into())),
            "the last value wins"
        );
        assert_eq!(map.get(&Ident::new("a")), Some(&RuntimeValue::Number(2.into())));
    }
}
