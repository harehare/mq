//! JSON decoding into mq runtime values.

use crate::runtime::runtime_value::RuntimeValue;
use crate::{DictMap, Ident, Shared};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};

/// Decodes one complete JSON document directly into a runtime value.
///
/// This avoids constructing an intermediate `serde_json::Value` tree before converting it to mq
/// values. Object entries are accumulated in a `BTreeMap` to retain serde_json's default key
/// ordering and duplicate-key (last value wins) semantics.
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
        let mut values = std::collections::BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            values.insert(key, map.next_value_seed(JsonRuntimeValueSeed)?);
        }
        let values: DictMap = values
            .into_iter()
            .map(|(key, value)| (Ident::new(&key), value))
            .collect();
        Ok(RuntimeValue::Dict(Shared::new(values)))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_json_runtime_value;
    use crate::runtime::runtime_value::RuntimeValue;
    use crate::{DictMap, Ident, Shared};

    #[test]
    fn retains_sorted_keys_and_last_duplicate_value() {
        let mut expected = DictMap::default();
        expected.insert(Ident::new("a"), RuntimeValue::Number(2.into()));
        expected.insert(Ident::new("z"), RuntimeValue::Number(3.into()));

        assert_eq!(
            parse_json_runtime_value(r#"{"z": 1, "a": 2, "z": 3}"#).unwrap(),
            RuntimeValue::Dict(Shared::new(expected))
        );
    }
}
