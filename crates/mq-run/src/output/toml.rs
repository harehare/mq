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
//! Values with no JSON form (`nan()`, `infinite()`, functions) are errors, not omissions.

use miette::miette;
use mq_lang::{DictMap, RuntimeValue, Shared};

/// Formats a dict key as a path component: bare when it is a plain TOML key, quoted otherwise.
fn child_key_path(path: &str, key: &str) -> String {
    let bare = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let key = if bare {
        key.to_string()
    } else {
        format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\""))
    };
    if path.is_empty() { key } else { format!("{path}.{key}") }
}

/// Returns whether a non-container value has a JSON (and so TOML) representation.
fn is_representable(value: &RuntimeValue) -> bool {
    match value {
        RuntimeValue::Number(n) => n.value().is_finite(),
        RuntimeValue::Boolean(_)
        | RuntimeValue::String(_)
        | RuntimeValue::Symbol(_)
        | RuntimeValue::Bytes(_)
        | RuntimeValue::Markdown(_, _) => true,
        _ => false,
    }
}

/// Drops `None`-valued dict keys, or returns an error naming the path of a `None` inside an
/// array or of a value TOML cannot represent (e.g. `nan()`, functions).
fn strip_none(value: &RuntimeValue, path: &str) -> Result<RuntimeValue, String> {
    match value {
        RuntimeValue::Dict(map) => {
            let mut out = DictMap::default();
            for (k, v) in map.iter() {
                if matches!(v, RuntimeValue::None) {
                    continue;
                }
                out.insert(*k, strip_none(v, &child_key_path(path, &k.to_string()))?);
            }
            Ok(RuntimeValue::Dict(Shared::new(out)))
        }
        RuntimeValue::Array(arr) => arr
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let child = format!("{path}[{i}]");
                if matches!(v, RuntimeValue::None) {
                    Err(format!("TOML has no null: found None at '{child}'"))
                } else {
                    strip_none(v, &child)
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|items| RuntimeValue::Array(Shared::new(items))),
        v if is_representable(v) => Ok(v.clone()),
        _ => Err(format!("TOML cannot represent this value at '{path}'")),
    }
}

/// Converts a list of [`mq_lang::RuntimeValue`]s into a TOML string.
pub(crate) fn runtime_values_to_toml(runtime_values: &[RuntimeValue]) -> miette::Result<String> {
    let stripped = runtime_values
        .iter()
        .map(|v| match v {
            RuntimeValue::Dict(_) | RuntimeValue::Array(_) => strip_none(v, ""),
            _ => Ok(v.clone()),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| miette!("{}", e))?;
    let value = super::json::runtime_values_to_json_value(&stripped);

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

    fn dict(entries: Vec<(&str, RuntimeValue)>) -> RuntimeValue {
        let mut map = DictMap::default();
        for (k, v) in entries {
            map.insert(mq_lang::Ident::new(k), v);
        }
        RuntimeValue::Dict(Shared::new(map))
    }

    #[test]
    fn test_non_finite_numbers_error_instead_of_being_omitted() {
        let nan = RuntimeValue::Number(f64::NAN.into());
        let inf = RuntimeValue::Number(f64::INFINITY.into());
        let nested = dict(vec![("inner", inf.clone())]);
        let in_array = RuntimeValue::Array(Shared::new(vec![dict(vec![("limit", nan.clone())])]));
        for (value, path) in [
            (nan, "limit"),
            (inf, "limit"),
            (nested, "t.inner"),
            (in_array, "t[0].limit"),
        ] {
            let top = if path.starts_with("t") {
                dict(vec![("t", value)])
            } else {
                dict(vec![
                    ("limit", value),
                    ("name", RuntimeValue::String(Shared::new("worker".into()))),
                ])
            };
            let err = runtime_values_to_toml(&[top]).unwrap_err();
            assert!(err.to_string().contains(path), "{err}");
        }
    }

    #[test]
    fn test_none_in_array_dict_is_omitted() {
        let arr = RuntimeValue::Array(Shared::new(vec![dict(vec![
            ("x", RuntimeValue::None),
            ("y", RuntimeValue::Number(2.into())),
        ])]));
        let result = runtime_values_to_toml(&[dict(vec![("items", arr)])]).unwrap();
        assert!(result.contains("y = 2"));
        assert!(!result.contains("x ="));
    }

    #[test]
    fn test_error_path_quotes_keys_with_path_syntax() {
        let literal = dict(vec![(
            "a.b",
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::None])),
        )]);
        let err = runtime_values_to_toml(&[literal]).unwrap_err();
        assert!(err.to_string().contains(r#"'"a.b"[0]'"#), "{err}");

        let nested = dict(vec![(
            "a",
            dict(vec![("b", RuntimeValue::Array(Shared::new(vec![RuntimeValue::None])))]),
        )]);
        let err = runtime_values_to_toml(&[nested]).unwrap_err();
        assert!(err.to_string().contains("'a.b[0]'"), "{err}");

        let quoted = dict(vec![(
            r#"q"\"#,
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::None])),
        )]);
        let err = runtime_values_to_toml(&[quoted]).unwrap_err();
        assert!(err.to_string().contains(r#"'"q\"\\"[0]'"#), "{err}");
    }
}
