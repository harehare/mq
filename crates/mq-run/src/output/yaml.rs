//! YAML output rendering for the `--output-format yaml` CLI option.
//!
//! Converts [`mq_lang::RuntimeValue`]s into YAML by first merging them into a
//! [`serde_json::Value`] (see [`crate::json`]) and serializing that with `yaml-rust2`.

use miette::miette;
use yaml_rust2::{Yaml, YamlEmitter, yaml::Hash};

/// Whether another YAML parser would read `s` as a non-string even though `yaml-rust2` leaves it
/// unquoted: `0o17` (1.2 octal), `1_000` (1.1 integer, optionally signed) and `2024-01-01` (1.1 date).
fn needs_forced_quotes(s: &str) -> bool {
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    let is_octal = unsigned
        .strip_prefix("0o")
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| (b'0'..=b'7').contains(&b)));
    let is_underscored_number = unsigned.contains('_')
        && unsigned.starts_with(|c: char| c.is_ascii_digit())
        && unsigned.bytes().all(|b| b.is_ascii_digit() || b == b'_');
    let mut parts = s.split('-');
    let is_date = matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(y), Some(m), Some(d), None)
            if y.len() == 4
                && (1..=2).contains(&m.len())
                && (1..=2).contains(&d.len())
                && [y, m, d].iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
    );

    is_octal || is_underscored_number || is_date
}

/// Builds a string node, double-quoting it when the emitter would not.
///
/// `Yaml::Real` is emitted verbatim, and a JSON string is a valid YAML double-quoted scalar.
fn string_to_yaml(s: &str) -> Yaml {
    if needs_forced_quotes(s) {
        Yaml::Real(serde_json::Value::String(s.to_string()).to_string())
    } else {
        Yaml::String(s.to_string())
    }
}

fn json_to_yaml(value: &serde_json::Value) -> Yaml {
    match value {
        serde_json::Value::Null => Yaml::Null,
        serde_json::Value::Bool(b) => Yaml::Boolean(*b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Yaml::Integer(i),
            None => Yaml::Real(n.to_string()),
        },
        serde_json::Value::String(s) => string_to_yaml(s),
        serde_json::Value::Array(items) => Yaml::Array(items.iter().map(json_to_yaml).collect()),
        serde_json::Value::Object(map) => {
            let mut hash = Hash::new();
            for (k, v) in map {
                hash.insert(string_to_yaml(k), json_to_yaml(v));
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

    #[rstest::rstest]
    #[case("0o17", "\"0o17\"")]
    #[case("1_000", "\"1_000\"")]
    #[case("+1_000", "\"+1_000\"")]
    #[case("-1_000", "\"-1_000\"")]
    #[case("+0o17", "\"+0o17\"")]
    #[case("-0o17", "\"-0o17\"")]
    #[case("2024-01-01", "\"2024-01-01\"")]
    #[case("2024-1-1", "\"2024-1-1\"")]
    #[case("0o18", "0o18")]
    #[case("a_1", "a_1")]
    #[case("2024-01-01x", "2024-01-01x")]
    #[case("1-2-3", "1-2-3")]
    fn test_ambiguous_strings_are_quoted(#[case] input: &str, #[case] expected: &str) {
        let values = vec![RuntimeValue::String(Shared::new(input.to_string()))];
        assert_eq!(runtime_values_to_yaml(&values).unwrap().trim(), expected);
    }

    #[test]
    fn test_string_value() {
        let values = vec![RuntimeValue::String(Shared::new("hello".to_string()))];
        let result = runtime_values_to_yaml(&values).unwrap();
        assert_eq!(result.trim(), "hello");
    }

    #[test]
    fn test_dict_value() {
        let mut map = mq_lang::DictMap::default();
        map.insert(
            mq_lang::Ident::new("name"),
            RuntimeValue::String(Shared::new("Alice".to_string())),
        );
        let values = vec![RuntimeValue::Dict(Shared::new(map))];
        let result = runtime_values_to_yaml(&values).unwrap();
        assert!(result.contains("name: Alice"));
    }

    #[test]
    fn test_array_value() {
        let values = vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String(Shared::new("a".to_string())),
            RuntimeValue::String(Shared::new("b".to_string())),
        ]))];
        let result = runtime_values_to_yaml(&values).unwrap();
        assert!(result.contains("- a"));
        assert!(result.contains("- b"));
    }

    #[test]
    fn test_multiple_values_becomes_sequence() {
        let values = vec![
            RuntimeValue::String(Shared::new("a".to_string())),
            RuntimeValue::String(Shared::new("b".to_string())),
        ];
        let result = runtime_values_to_yaml(&values).unwrap();
        let parsed = yaml_rust2::YamlLoader::load_from_str(&result).unwrap();
        assert!(parsed[0].is_array());
    }
}
