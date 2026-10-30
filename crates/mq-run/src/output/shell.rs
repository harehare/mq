//! Shell variable output rendering for the `-F shell` CLI option.
//!
//! Flattens the same normalized JSON tree used by `-F json` into `key=value`
//! assignment statements (one per line, `eval`-able), following the
//! conventions of yq's `-o=shell` output
//! (<https://mikefarah.gitbook.io/yq/usage/shellvariables>): nested dict keys
//! and array indices join with `_`, and empty dicts/arrays are skipped.

use std::collections::HashSet;

use super::json::runtime_values_to_json_value;

/// Prefix prepended to every variable name unless `--shell-prefix` overrides it.
pub(crate) const DEFAULT_PREFIX: &str = "MQ_";

/// Renders `runtime_values` as `eval`-able assignments, each variable name starting with `prefix`.
///
/// Fails when `prefix` is empty or not a valid identifier prefix, or when two keys flatten to
/// the same variable name.
pub(crate) fn runtime_values_to_shell(
    runtime_values: &[mq_lang::RuntimeValue],
    prefix: &str,
) -> miette::Result<String> {
    validate_prefix(prefix)?;
    let value = runtime_values_to_json_value(runtime_values);
    let mut out = String::new();
    let mut seen = HashSet::new();
    write_shell(&mut out, &mut seen, prefix, "", &value)?;
    Ok(out)
}

fn validate_prefix(prefix: &str) -> miette::Result<()> {
    let mut chars = prefix.chars();
    let valid = chars
        .next()
        .is_some_and(|c| (c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_'));
    if valid {
        Ok(())
    } else {
        Err(miette::miette!(
            "invalid --shell-prefix `{prefix}`: must be non-empty and match [A-Za-z_][A-Za-z0-9_]*"
        ))
    }
}

fn write_shell(
    out: &mut String,
    seen: &mut HashSet<String>,
    prefix: &str,
    path: &str,
    value: &serde_json::Value,
) -> miette::Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                write_shell(out, seen, prefix, &join_key(path, key), child)?;
            }
        }
        serde_json::Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                write_shell(out, seen, prefix, &join_key(path, &i.to_string()), child)?;
            }
        }
        serde_json::Value::Null => write_assignment(out, seen, prefix, path, "")?,
        serde_json::Value::Bool(b) => write_assignment(out, seen, prefix, path, &b.to_string())?,
        serde_json::Value::Number(n) => write_assignment(out, seen, prefix, path, &n.to_string())?,
        serde_json::Value::String(s) => write_assignment(out, seen, prefix, path, &shell_quote(s))?,
    }
    Ok(())
}

fn write_assignment(
    out: &mut String,
    seen: &mut HashSet<String>,
    prefix: &str,
    path: &str,
    value: &str,
) -> miette::Result<()> {
    let name = format!("{prefix}{}", shell_var_name(path));
    if !seen.insert(name.clone()) {
        return Err(miette::miette!(
            "variable name `{name}` is produced by more than one key; rename the keys"
        ));
    }
    out.push_str(&name);
    out.push('=');
    out.push_str(value);
    out.push('\n');
    Ok(())
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
    use mq_lang::DictMap;
    use mq_lang::Shared;
    use rstest::rstest;

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
    #[case(vec![mq_lang::RuntimeValue::String(Shared::new("hello".to_string()))], "MQ_value=hello\n")]
    #[case(vec![mq_lang::RuntimeValue::String(Shared::new("hello world".to_string()))], "MQ_value='hello world'\n")]
    #[case(vec![mq_lang::RuntimeValue::Boolean(true)], "MQ_value=true\n")]
    #[case(vec![mq_lang::RuntimeValue::Boolean(false)], "MQ_value=false\n")]
    #[case(vec![mq_lang::RuntimeValue::Number(3i64.into())], "MQ_value=3\n")]
    #[case(vec![mq_lang::RuntimeValue::None], "MQ_value=\n")]
    #[case(vec![mq_lang::RuntimeValue::Array(Shared::new(vec![]))], "")]
    #[case(vec![mq_lang::RuntimeValue::Dict(Shared::new(DictMap::default()))], "")]
    #[case(
        vec![mq_lang::RuntimeValue::Array(Shared::new(vec![
            mq_lang::RuntimeValue::String(Shared::new("x".to_string())),
            mq_lang::RuntimeValue::String(Shared::new("y".to_string())),
        ]))],
        "MQ__0=x\nMQ__1=y\n"
    )]
    fn test_runtime_values_to_shell(#[case] values: Vec<mq_lang::RuntimeValue>, #[case] expected: &str) {
        assert_eq!(runtime_values_to_shell(&values, DEFAULT_PREFIX).unwrap(), expected);
    }

    #[test]
    fn test_shell_nested_dict() {
        let mut inner = DictMap::default();
        inner.insert(
            mq_lang::Ident::new("color"),
            mq_lang::RuntimeValue::String(Shared::new("turquoise".to_string())),
        );
        let mut outer = DictMap::default();
        outer.insert(
            mq_lang::Ident::new("eyes"),
            mq_lang::RuntimeValue::Dict(Shared::new(inner)),
        );
        let values = vec![mq_lang::RuntimeValue::Dict(Shared::new(outer))];
        assert_eq!(
            runtime_values_to_shell(&values, DEFAULT_PREFIX).unwrap(),
            "MQ_eyes_color=turquoise\n"
        );
    }

    #[test]
    fn test_shell_dict_key_needing_sanitization() {
        let mut m = DictMap::default();
        m.insert(
            mq_lang::Ident::new("weird key!"),
            mq_lang::RuntimeValue::String(Shared::new("v".to_string())),
        );
        let values = vec![mq_lang::RuntimeValue::Dict(Shared::new(m))];
        assert_eq!(
            runtime_values_to_shell(&values, DEFAULT_PREFIX).unwrap(),
            "MQ_weird_key_=v\n"
        );
    }

    #[test]
    fn test_shell_array_under_dict_key_has_no_double_underscore() {
        let mut friend = DictMap::default();
        friend.insert(
            mq_lang::Ident::new("name"),
            mq_lang::RuntimeValue::String(Shared::new("James P. Sullivan".to_string())),
        );
        let mut outer = DictMap::default();
        outer.insert(
            mq_lang::Ident::new("friends"),
            mq_lang::RuntimeValue::Array(Shared::new(vec![mq_lang::RuntimeValue::Dict(Shared::new(friend))])),
        );
        let values = vec![mq_lang::RuntimeValue::Dict(Shared::new(outer))];
        assert_eq!(
            runtime_values_to_shell(&values, DEFAULT_PREFIX).unwrap(),
            "MQ_friends_0_name='James P. Sullivan'\n"
        );
    }

    #[test]
    fn test_shell_top_level_array_gets_digit_prefix() {
        let values = vec![mq_lang::RuntimeValue::Array(Shared::new(vec![
            mq_lang::RuntimeValue::String(Shared::new("x".to_string())),
        ]))];
        assert_eq!(runtime_values_to_shell(&values, DEFAULT_PREFIX).unwrap(), "MQ__0=x\n");
    }

    fn dict(pairs: &[(&str, &str)]) -> Vec<mq_lang::RuntimeValue> {
        let mut m = DictMap::default();
        for (k, v) in pairs {
            m.insert(
                mq_lang::Ident::new(k),
                mq_lang::RuntimeValue::String(Shared::new(v.to_string())),
            );
        }
        vec![mq_lang::RuntimeValue::Dict(Shared::new(m))]
    }

    #[test]
    fn test_shell_prefix_applied() {
        assert_eq!(
            runtime_values_to_shell(&dict(&[("PATH", "/evil")]), DEFAULT_PREFIX).unwrap(),
            "MQ_PATH=/evil\n"
        );
    }

    #[rstest]
    #[case(&[("a b", "1"), ("a-b", "2")])]
    #[case(&[("é", "1"), ("è", "2")])]
    #[case(&[("", "1"), ("value", "2")])]
    fn test_shell_collision_is_error(#[case] pairs: &[(&str, &str)]) {
        assert!(runtime_values_to_shell(&dict(pairs), DEFAULT_PREFIX).is_err());
    }

    #[rstest]
    #[case("")]
    #[case("1bad")]
    #[case("a-b")]
    #[case("a b")]
    fn test_shell_invalid_prefix(#[case] prefix: &str) {
        assert!(runtime_values_to_shell(&dict(&[("a", "1")]), prefix).is_err());
    }
}
