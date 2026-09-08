//! TOON output rendering for the `--output-format toon` CLI option.
//!
//! Delegates to the `toon-format` crate (the same one `mq-lang`'s `toon` module uses
//! for `-I toon` parsing) over the normalized JSON tree `-F json` uses, rather than
//! the mq evaluator: `toon_format::encode` is plain Rust recursion, so it can't blow
//! the thread stack the way evaluating a heavily-recursive mq function can on deep
//! Markdown ASTs.

use miette::miette;

pub(crate) fn runtime_values_to_toon(runtime_values: &[mq_lang::RuntimeValue]) -> miette::Result<String> {
    let value = super::json::runtime_values_to_json_value(runtime_values);
    toon_format::encode_default(&value).map_err(|e| miette!("Failed to encode TOON: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::{Ident, RuntimeValue, Shared};
    use rstest::rstest;
    use std::collections::BTreeMap;

    fn single_key_dict(key: &str, value: RuntimeValue) -> RuntimeValue {
        let mut map = BTreeMap::new();
        map.insert(Ident::new(key), value);
        RuntimeValue::Dict(Shared::new(map))
    }

    // Each case is a lone `RuntimeValue`: multi-key dicts are exercised separately in
    // `test_round_trip` instead of here, since `RuntimeValue::Dict` (a `BTreeMap<Ident, _>`)
    // orders keys by interned symbol id, which isn't stable across a test binary run and
    // would make an exact-string assertion on more than one key flaky.
    #[rstest]
    #[case::string_no_quoting(RuntimeValue::String(Shared::new("hello".to_string())), "hello")]
    #[case::number(RuntimeValue::from(42usize), "42")]
    #[case::bool_true(RuntimeValue::Boolean(true), "true")]
    #[case::bool_false(RuntimeValue::Boolean(false), "false")]
    #[case::none(RuntimeValue::None, "null")]
    #[case::single_key_dict(single_key_dict("name", RuntimeValue::String(Shared::new("Alice".to_string()))), "name: Alice")]
    #[case::array_of_primitives(
        RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String(Shared::new("a".to_string())),
            RuntimeValue::String(Shared::new("b".to_string())),
        ])),
        "[2]: a,b"
    )]
    #[case::empty_array(RuntimeValue::Array(Shared::new(vec![])), "[0]:")]
    #[case::empty_dict(RuntimeValue::Dict(Shared::new(BTreeMap::new())), "")]
    #[case::empty_string_needs_quoting(RuntimeValue::String(Shared::new("".to_string())), "\"\"")]
    #[case::numeric_like_string_needs_quoting(RuntimeValue::String(Shared::new("123".to_string())), "\"123\"")]
    #[case::keyword_like_string_needs_quoting(RuntimeValue::String(Shared::new("true".to_string())), "\"true\"")]
    #[case::string_with_colon_needs_quoting(RuntimeValue::String(Shared::new("a:b".to_string())), "\"a:b\"")]
    #[case::string_with_delimiter_needs_quoting(RuntimeValue::String(Shared::new("a,b".to_string())), "\"a,b\"")]
    #[case::string_starting_with_dash_needs_quoting(RuntimeValue::String(Shared::new("-x".to_string())), "\"-x\"")]
    #[case::string_with_leading_space_needs_quoting(RuntimeValue::String(Shared::new(" x".to_string())), "\" x\"")]
    fn test_runtime_values_to_toon(#[case] value: RuntimeValue, #[case] expected: &str) {
        assert_eq!(runtime_values_to_toon(&[value]).unwrap(), expected);
    }

    fn tabular_row(id: usize, name: &str) -> RuntimeValue {
        let mut map = BTreeMap::new();
        map.insert(Ident::new("id"), RuntimeValue::from(id));
        map.insert(Ident::new("name"), RuntimeValue::String(Shared::new(name.to_string())));
        RuntimeValue::Dict(Shared::new(map))
    }

    // Structural round trip (encode, then re-decode with the same `toon-format` crate) rather
    // than an exact string: `serde_json::Value`'s `Map` equality ignores key order, so this
    // stays robust regardless of how `RuntimeValue::Dict`'s symbol-id ordering shakes out.
    #[rstest]
    #[case::tabular_array(RuntimeValue::Array(Shared::new(vec![
        tabular_row(1, "Blue Lake"),
        tabular_row(2, "Ridge Trail"),
    ])))]
    #[case::nested_dict(single_key_dict("outer", single_key_dict("inner", RuntimeValue::from(1usize))))]
    #[case::non_uniform_array(RuntimeValue::Array(Shared::new(vec![
        RuntimeValue::from(1usize),
        tabular_row(2, "Ridge Trail"),
        RuntimeValue::String(Shared::new("text".to_string())),
    ])))]
    fn test_round_trip(#[case] original: RuntimeValue) {
        let expected = super::super::json::runtime_values_to_json_value(std::slice::from_ref(&original));
        let encoded = runtime_values_to_toon(&[original]).unwrap();
        let decoded: serde_json::Value = toon_format::decode(&encoded, &toon_format::DecodeOptions::default()).unwrap();
        assert_eq!(decoded, expected);
    }
}
