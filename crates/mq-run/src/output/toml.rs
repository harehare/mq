//! TOML output rendering for the `--output-format toml` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into TOML by first merging them into a
//! [`serde_json::Value`] (see [`crate::json`]) and serializing that with the `toml` crate.
//!
//! TOML documents must be a table at the top level, so the merged value must be
//! a dict (JSON object); anything else is a descriptive error rather than a panic.
//!
//! TOML has no null, so dict keys whose value is `None` are omitted. A `None` inside
//! an array cannot be omitted without shifting indices, so it is an error naming its path.

use miette::miette;
#[cfg(test)]
use mq_lang::Shared;

/// Removes null-valued dict keys in place, or returns the path of a null inside an array.
fn strip_nulls(value: &mut serde_json::Value, path: &str) -> Result<(), String> {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for (k, v) in map.iter_mut() {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                strip_nulls(v, &child)?;
            }
            Ok(())
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter_mut().enumerate() {
                let child = format!("{path}[{i}]");
                if v.is_null() {
                    return Err(child);
                }
                strip_nulls(v, &child)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Converts a list of [`mq_lang::RuntimeValue`]s into a TOML string.
pub(crate) fn runtime_values_to_toml(runtime_values: &[mq_lang::RuntimeValue]) -> miette::Result<String> {
    let mut value = super::json::runtime_values_to_json_value(runtime_values);

    if !value.is_object() {
        return Err(miette!(
            "TOML output requires the top-level value to be a dict (TOML documents are tables), got: {}",
            value
        ));
    }

    strip_nulls(&mut value, "").map_err(|path| miette!("TOML has no null: found None at '{}'", path))?;

    toml::to_string_pretty(&value).map_err(|e| miette!("Failed to serialize to TOML: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::RuntimeValue;

    #[test]
    fn test_dict_value() {
        let mut map = mq_lang::DictMap::default();
        map.insert(
            mq_lang::Ident::new("name"),
            RuntimeValue::String(Shared::new("Alice".to_string())),
        );
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_toml(&values).unwrap();
        assert!(result.contains("name = \"Alice\""));
    }

    #[test]
    fn test_nested_dict() {
        let mut inner = mq_lang::DictMap::default();
        inner.insert(
            mq_lang::Ident::new("city"),
            RuntimeValue::String(Shared::new("NYC".to_string())),
        );
        let mut outer = mq_lang::DictMap::default();
        outer.insert(mq_lang::Ident::new("address"), RuntimeValue::Dict(Shared::new(inner)));
        let values = vec![RuntimeValue::Dict(Shared::new(outer))];
        let result = runtime_values_to_toml(&values).unwrap();
        assert!(result.contains("[address]"));
        assert!(result.contains("city = \"NYC\""));
    }

    #[test]
    fn test_non_dict_top_level_errors() {
        let values = vec![RuntimeValue::String(Shared::new("hello".to_string()))];
        let result = runtime_values_to_toml(&values);
        assert!(result.is_err());
    }

    #[test]
    fn test_array_top_level_errors() {
        let values = vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(
            Shared::new("a".to_string()),
        )]))];
        let result = runtime_values_to_toml(&values);
        assert!(result.is_err());
    }

    #[test]
    fn test_none_keys_are_omitted() {
        let mut inner = mq_lang::DictMap::default();
        inner.insert(mq_lang::Ident::new("x"), RuntimeValue::None);
        inner.insert(mq_lang::Ident::new("y"), RuntimeValue::Number(2.into()));
        let mut outer = mq_lang::DictMap::default();
        outer.insert(mq_lang::Ident::new("a"), RuntimeValue::Number(1.into()));
        outer.insert(mq_lang::Ident::new("n"), RuntimeValue::None);
        outer.insert(mq_lang::Ident::new("t"), RuntimeValue::Dict(Shared::new(inner)));
        let result = runtime_values_to_toml(&[RuntimeValue::Dict(Shared::new(outer))]).unwrap();
        assert!(result.contains("a = 1"));
        assert!(result.contains("y = 2"));
        assert!(!result.lines().any(|l| l.starts_with("n =")));
        assert!(!result.contains("x ="));
    }

    #[test]
    fn test_none_in_array_errors_with_path() {
        let arr = RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into()), RuntimeValue::None]));
        let mut map = mq_lang::DictMap::default();
        map.insert(mq_lang::Ident::new("items"), arr);
        let err = runtime_values_to_toml(&[RuntimeValue::Dict(Shared::new(map))]).unwrap_err();
        assert!(err.to_string().contains("items[1]"));
    }
}
