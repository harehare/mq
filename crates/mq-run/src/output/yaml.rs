//! YAML output rendering for the `--output-format yaml` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into YAML by first merging them into a
//! [`serde_json::Value`] (see [`crate::json`]) and serializing that with `serde_yaml`.

use miette::miette;

/// Converts a list of [`mq_lang::RuntimeValue`]s into a YAML string.
pub(crate) fn runtime_values_to_yaml(runtime_values: &[mq_lang::RuntimeValue]) -> miette::Result<String> {
    let value = super::json::runtime_values_to_json_value(runtime_values);
    serde_yaml::to_string(&value).map_err(|e| miette!("Failed to serialize to YAML: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::RuntimeValue;

    #[test]
    fn test_string_value() {
        let values = vec![RuntimeValue::String("hello".to_string())];
        let result = runtime_values_to_yaml(&values).unwrap();
        assert_eq!(result.trim(), "hello");
    }

    #[test]
    fn test_dict_value() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(mq_lang::Ident::new("name"), RuntimeValue::String("Alice".to_string()));
        let values = vec![RuntimeValue::Dict(map)];
        let result = runtime_values_to_yaml(&values).unwrap();
        assert!(result.contains("name: Alice"));
    }

    #[test]
    fn test_array_value() {
        let values = vec![RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
        ])];
        let result = runtime_values_to_yaml(&values).unwrap();
        assert!(result.contains("- a"));
        assert!(result.contains("- b"));
    }

    #[test]
    fn test_multiple_values_becomes_sequence() {
        let values = vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
        ];
        let result = runtime_values_to_yaml(&values).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        assert!(parsed.is_sequence());
    }
}
