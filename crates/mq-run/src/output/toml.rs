//! TOML output rendering for the `--output-format toml` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into TOML by first merging them into a
//! [`serde_json::Value`] (see [`crate::json`]) and serializing that with the `toml` crate.
//!
//! TOML documents must be a table at the top level, so the merged value must be
//! a dict (JSON object); anything else is a descriptive error rather than a panic.

use miette::miette;

/// Converts a list of [`mq_lang::RuntimeValue`]s into a TOML string.
pub(crate) fn runtime_values_to_toml(runtime_values: &[mq_lang::RuntimeValue]) -> miette::Result<String> {
    let value = super::json::runtime_values_to_json_value(runtime_values);

    if !value.is_object() {
        return Err(miette!(
            "TOML output requires the top-level value to be a dict (TOML documents are tables), got: {}",
            value
        ));
    }

    toml::to_string_pretty(&value).map_err(|e| miette!("Failed to serialize to TOML: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::RuntimeValue;

    #[test]
    fn test_dict_value() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(mq_lang::Ident::new("name"), RuntimeValue::String("Alice".to_string()));
        let values = vec![RuntimeValue::Dict(map)];
        let result = runtime_values_to_toml(&values).unwrap();
        assert!(result.contains("name = \"Alice\""));
    }

    #[test]
    fn test_nested_dict() {
        let mut inner = std::collections::BTreeMap::new();
        inner.insert(mq_lang::Ident::new("city"), RuntimeValue::String("NYC".to_string()));
        let mut outer = std::collections::BTreeMap::new();
        outer.insert(mq_lang::Ident::new("address"), RuntimeValue::Dict(inner));
        let values = vec![RuntimeValue::Dict(outer)];
        let result = runtime_values_to_toml(&values).unwrap();
        assert!(result.contains("[address]"));
        assert!(result.contains("city = \"NYC\""));
    }

    #[test]
    fn test_non_dict_top_level_errors() {
        let values = vec![RuntimeValue::String("hello".to_string())];
        let result = runtime_values_to_toml(&values);
        assert!(result.is_err());
    }

    #[test]
    fn test_array_top_level_errors() {
        let values = vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string())])];
        let result = runtime_values_to_toml(&values);
        assert!(result.is_err());
    }
}
