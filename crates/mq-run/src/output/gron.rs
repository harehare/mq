//! gron-like flat/greppable output rendering for the `-F gron` CLI option.
//!
//! Flattens the same normalized JSON tree used by `-F json`
//! ([`super::json::runtime_values_to_json_value`]) into a sequence of
//! `path = value;` assignment statements (following the conventions of the
//! `gron` tool), so a specific AST path can be located with `grep` without
//! knowing the mq selector syntax. `-I gron` (via the `gron` module) parses
//! these statements back into a data structure.

use super::json::runtime_values_to_json_value;

/// Converts a list of [`mq_lang::RuntimeValue`]s into gron-style assignment statements.
pub(crate) fn runtime_values_to_gron(runtime_values: &[mq_lang::RuntimeValue]) -> String {
    let value = runtime_values_to_json_value(runtime_values);
    let mut out = String::new();
    write_gron(&mut out, "json", &value);
    out
}

fn write_gron(out: &mut String, path: &str, value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            out.push_str(path);
            out.push_str(" = {};\n");
            for (key, child) in map {
                write_gron(out, &join_key(path, key), child);
            }
        }
        serde_json::Value::Array(items) => {
            out.push_str(path);
            out.push_str(" = [];\n");
            for (i, child) in items.iter().enumerate() {
                write_gron(out, &format!("{}[{}]", path, i), child);
            }
        }
        scalar => {
            out.push_str(path);
            out.push_str(" = ");
            out.push_str(&scalar.to_string());
            out.push_str(";\n");
        }
    }
}

/// Joins a gron path with the next key, using dot notation (`path.key`) when
/// `key` is a valid bare identifier, and bracket notation (`path["key"]`)
/// otherwise (matching `gron`'s own escaping rules).
fn join_key(path: &str, key: &str) -> String {
    if is_bare_ident(key) {
        format!("{}.{}", path, key)
    } else {
        format!("{}[{}]", path, serde_json::to_string(key).unwrap())
    }
}

fn is_bare_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::Shared;
    use rstest::rstest;
    use std::collections::BTreeMap;

    #[rstest]
    #[case("json", "foo", "json.foo")]
    #[case("json", "_foo1", "json._foo1")]
    #[case("json", "foo bar", "json[\"foo bar\"]")]
    #[case("json", "1foo", "json[\"1foo\"]")]
    #[case("json", "", "json[\"\"]")]
    fn test_join_key(#[case] path: &str, #[case] key: &str, #[case] expected: &str) {
        assert_eq!(join_key(path, key), expected);
    }

    #[rstest]
    #[case(vec![mq_lang::RuntimeValue::String(Shared::new("hello".to_string()))], "json = \"hello\";\n")]
    #[case(vec![mq_lang::RuntimeValue::Boolean(true)], "json = true;\n")]
    #[case(vec![mq_lang::RuntimeValue::Boolean(false)], "json = false;\n")]
    #[case(vec![mq_lang::RuntimeValue::Number(3i64.into())], "json = 3;\n")]
    #[case(vec![mq_lang::RuntimeValue::None], "json = null;\n")]
    #[case(vec![mq_lang::RuntimeValue::Array(Shared::new(vec![]))], "json = [];\n")]
    #[case(
        vec![mq_lang::RuntimeValue::Array(Shared::new(vec![
            mq_lang::RuntimeValue::String(Shared::new("x".to_string())),
            mq_lang::RuntimeValue::String(Shared::new("y".to_string())),
        ]))],
        "json = [];\njson[0] = \"x\";\njson[1] = \"y\";\n"
    )]
    fn test_runtime_values_to_gron(#[case] values: Vec<mq_lang::RuntimeValue>, #[case] expected: &str) {
        assert_eq!(runtime_values_to_gron(&values), expected);
    }

    #[test]
    fn test_gron_nested_dict() {
        let mut inner = BTreeMap::new();
        inner.insert(
            mq_lang::Ident::new("b"),
            mq_lang::RuntimeValue::String(Shared::new("deep".to_string())),
        );
        let mut outer = BTreeMap::new();
        outer.insert(
            mq_lang::Ident::new("a"),
            mq_lang::RuntimeValue::Dict(Shared::new(inner)),
        );
        let values = vec![mq_lang::RuntimeValue::Dict(Shared::new(outer))];
        assert_eq!(
            runtime_values_to_gron(&values),
            "json = {};\njson.a = {};\njson.a.b = \"deep\";\n"
        );
    }

    #[test]
    fn test_gron_dict_key_needing_brackets() {
        let mut m = BTreeMap::new();
        m.insert(
            mq_lang::Ident::new("weird key"),
            mq_lang::RuntimeValue::String(Shared::new("v".to_string())),
        );
        let values = vec![mq_lang::RuntimeValue::Dict(Shared::new(m))];
        assert_eq!(
            runtime_values_to_gron(&values),
            "json = {};\njson[\"weird key\"] = \"v\";\n"
        );
    }
}
