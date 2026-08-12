//! Shell variable output rendering for the `-F shell` CLI option.
//!
//! Flattens the same normalized JSON tree used by `-F json` into `key=value`
//! assignment statements (one per line, `eval`-able), following the
//! conventions of yq's `-o=shell` output
//! (<https://mikefarah.gitbook.io/yq/usage/shellvariables>): nested dict keys
//! and array indices join with `_`, and empty dicts/arrays are skipped.

use super::json::runtime_values_to_json_value;

pub(crate) fn runtime_values_to_shell(runtime_values: &[mq_lang::RuntimeValue]) -> String {
    let value = runtime_values_to_json_value(runtime_values);
    let mut out = String::new();
    write_shell(&mut out, "", &value);
    out
}

fn write_shell(out: &mut String, path: &str, value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                write_shell(out, &join_key(path, key), child);
            }
        }
        serde_json::Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                write_shell(out, &join_key(path, &i.to_string()), child);
            }
        }
        serde_json::Value::Null => write_assignment(out, path, ""),
        serde_json::Value::Bool(b) => write_assignment(out, path, &b.to_string()),
        serde_json::Value::Number(n) => write_assignment(out, path, &n.to_string()),
        serde_json::Value::String(s) => write_assignment(out, path, &shell_quote(s)),
    }
}

#[inline(always)]
fn write_assignment(out: &mut String, path: &str, value: &str) {
    out.push_str(&shell_var_name(path));
    out.push('=');
    out.push_str(value);
    out.push('\n');
}

/// `value` when there's no enclosing key; digit-led paths (bare array indices)
/// get a `_` prefix since shell variable names can't start with a digit.
fn shell_var_name(path: &str) -> String {
    if path.is_empty() {
        "value".to_string()
    } else if path.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{}", path)
    } else {
        path.to_string()
    }
}

fn join_key(path: &str, key: &str) -> String {
    let sanitized = sanitize_key(key);
    if path.is_empty() {
        sanitized
    } else {
        format!("{}_{}", path, sanitized)
    }
}

fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// Single-quotes `s` unless it's already safe bare; embedded `'` escapes via
/// close-quote/escaped-quote/reopen-quote (`'"'"'`), the standard POSIX trick.
fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    if s.chars().all(is_safe_unquoted_char) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r#"'"'"'"#))
}

fn is_safe_unquoted_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::Shared;
    use rstest::rstest;
    use std::collections::BTreeMap;

    #[rstest]
    #[case("", "foo", "foo")]
    #[case("foo", "bar", "foo_bar")]
    #[case("", "1foo", "1foo")]
    #[case("", "foo bar", "foo_bar")]
    #[case("", "", "")]
    fn test_join_key(#[case] path: &str, #[case] key: &str, #[case] expected: &str) {
        assert_eq!(join_key(path, key), expected);
    }

    #[rstest]
    #[case("", "value")]
    #[case("foo", "foo")]
    #[case("0", "_0")]
    #[case("friends_0", "friends_0")]
    fn test_shell_var_name(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(shell_var_name(path), expected);
    }

    #[rstest]
    #[case("turquoise", "turquoise")]
    #[case("Mike Wazowski", "'Mike Wazowski'")]
    #[case("", "")]
    #[case("Miles O'Brien", r#"'Miles O'"'"'Brien'"#)]
    fn test_shell_quote(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(shell_quote(input), expected);
    }

    #[rstest]
    #[case(vec![mq_lang::RuntimeValue::String("hello".to_string())], "value=hello\n")]
    #[case(vec![mq_lang::RuntimeValue::String("hello world".to_string())], "value='hello world'\n")]
    #[case(vec![mq_lang::RuntimeValue::Boolean(true)], "value=true\n")]
    #[case(vec![mq_lang::RuntimeValue::Boolean(false)], "value=false\n")]
    #[case(vec![mq_lang::RuntimeValue::Number(3i64.into())], "value=3\n")]
    #[case(vec![mq_lang::RuntimeValue::None], "value=\n")]
    #[case(vec![mq_lang::RuntimeValue::Array(Shared::new(vec![]))], "")]
    #[case(vec![mq_lang::RuntimeValue::Dict(Shared::new(BTreeMap::new()))], "")]
    #[case(
        vec![mq_lang::RuntimeValue::Array(Shared::new(vec![
            mq_lang::RuntimeValue::String("x".to_string()),
            mq_lang::RuntimeValue::String("y".to_string()),
        ]))],
        "_0=x\n_1=y\n"
    )]
    fn test_runtime_values_to_shell(#[case] values: Vec<mq_lang::RuntimeValue>, #[case] expected: &str) {
        assert_eq!(runtime_values_to_shell(&values), expected);
    }

    #[test]
    fn test_shell_nested_dict() {
        let mut inner = BTreeMap::new();
        inner.insert(
            mq_lang::Ident::new("color"),
            mq_lang::RuntimeValue::String("turquoise".to_string()),
        );
        let mut outer = BTreeMap::new();
        outer.insert(
            mq_lang::Ident::new("eyes"),
            mq_lang::RuntimeValue::Dict(Shared::new(inner)),
        );
        let values = vec![mq_lang::RuntimeValue::Dict(Shared::new(outer))];
        assert_eq!(runtime_values_to_shell(&values), "eyes_color=turquoise\n");
    }

    #[test]
    fn test_shell_dict_key_needing_sanitization() {
        let mut m = BTreeMap::new();
        m.insert(
            mq_lang::Ident::new("weird key!"),
            mq_lang::RuntimeValue::String("v".to_string()),
        );
        let values = vec![mq_lang::RuntimeValue::Dict(Shared::new(m))];
        assert_eq!(runtime_values_to_shell(&values), "weird_key_=v\n");
    }

    #[test]
    fn test_shell_array_under_dict_key_has_no_double_underscore() {
        let mut friend = BTreeMap::new();
        friend.insert(
            mq_lang::Ident::new("name"),
            mq_lang::RuntimeValue::String("James P. Sullivan".to_string()),
        );
        let mut outer = BTreeMap::new();
        outer.insert(
            mq_lang::Ident::new("friends"),
            mq_lang::RuntimeValue::Array(Shared::new(vec![mq_lang::RuntimeValue::Dict(Shared::new(friend))])),
        );
        let values = vec![mq_lang::RuntimeValue::Dict(Shared::new(outer))];
        assert_eq!(runtime_values_to_shell(&values), "friends_0_name='James P. Sullivan'\n");
    }

    #[test]
    fn test_shell_top_level_array_gets_digit_prefix() {
        let values = vec![mq_lang::RuntimeValue::Array(Shared::new(vec![
            mq_lang::RuntimeValue::String("x".to_string()),
        ]))];
        assert_eq!(runtime_values_to_shell(&values), "_0=x\n");
    }
}
