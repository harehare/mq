//! YAML output rendering for the `--output-format yaml` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into YAML by first merging them into a
//! [`serde_json::Value`] (see [`crate::json`]) and serializing that with `yaml-rust2`.

use miette::miette;
use yaml_rust2::{Yaml, YamlEmitter, yaml::Hash};

fn json_to_yaml(value: &serde_json::Value) -> Yaml {
    match value {
        serde_json::Value::Null => Yaml::Null,
        serde_json::Value::Bool(b) => Yaml::Boolean(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Yaml::Integer(i),
            None => Yaml::Real(n.to_string()),
        },
        serde_json::Value::String(s) => Yaml::String(s.clone()),
        serde_json::Value::Array(items) => Yaml::Array(items.iter().map(json_to_yaml).collect()),
        serde_json::Value::Object(map) => {
            let mut hash = Hash::new();
            for (k, v) in map {
                hash.insert(Yaml::String(k.clone()), json_to_yaml(v));
            }
            Yaml::Hash(hash)
        }
    }
}

/// Converts a list of [`mq_lang::RuntimeValue`]s into a YAML string.
pub(crate) fn runtime_values_to_yaml(runtime_values: &[mq_lang::RuntimeValue]) -> miette::Result<String> {
    let value = super::json::runtime_values_to_json_value(runtime_values);
    let yaml_value = json_to_yaml(&value);
    let mut yaml_str = String::new();

    YamlEmitter::new(&mut yaml_str)
        .dump(&yaml_value)
        .map_err(|e| miette!("Failed to serialize to YAML: {}", e))?;

    // `dump` always prefixes the document with a `---` marker; drop it.
    let yaml_str = yaml_str.strip_prefix("---\n").unwrap_or(&yaml_str).to_string();

    Ok(if yaml_str.ends_with('\n') {
        yaml_str
    } else {
        format!("{}\n", yaml_str)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::RuntimeValue;
    use mq_lang::Shared;

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
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_yaml(&values).unwrap();
        assert!(result.contains("name: Alice"));
    }

    #[test]
    fn test_array_value() {
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
        ]))];
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
        let parsed = yaml_rust2::YamlLoader::load_from_str(&result).unwrap();
        assert!(parsed[0].is_array());
    }
}
