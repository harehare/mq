use crate::Ident;
use crate::eval::runtime_value::RuntimeValue;
use regex_lite::{Regex, RegexBuilder};
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::collections::BTreeMap;
use std::sync::{LazyLock, RwLock};

use super::Error;

pub(super) static REGEX_CACHE: LazyLock<RwLock<FxHashMap<String, Regex>>> =
    LazyLock::new(|| RwLock::new(FxHashMap::with_hasher(FxBuildHasher)));

pub(super) fn match_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        let matches: Vec<RuntimeValue> = re
            .find_iter(input)
            .map(|m| RuntimeValue::String(m.as_str().to_string()))
            .collect();
        return Ok(RuntimeValue::Array(matches));
    }
    let re = RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|_| Error::InvalidRegularExpression(pattern.to_string()))?;
    REGEX_CACHE.write().unwrap().insert(pattern.to_string(), re.clone());
    let matches: Vec<RuntimeValue> = re
        .find_iter(input)
        .map(|m| RuntimeValue::String(m.as_str().to_string()))
        .collect();
    Ok(RuntimeValue::Array(matches))
}

pub(super) fn is_match_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return Ok(re.is_match(input).into());
    }
    let re = RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|_| Error::InvalidRegularExpression(pattern.to_string()))?;
    REGEX_CACHE.write().unwrap().insert(pattern.to_string(), re.clone());
    Ok(re.is_match(input).into())
}

pub(super) fn capture_re_inner(re: &Regex, input: &str) -> Result<RuntimeValue, Error> {
    match (re.capture_names(), re.captures(input)) {
        (names, Some(caps)) => {
            let mut result = BTreeMap::new();
            for name in names.flatten() {
                if let Some(m) = caps.name(name) {
                    result.insert(Ident::new(name), RuntimeValue::String(m.as_str().to_string()));
                }
            }
            Ok(RuntimeValue::Dict(result))
        }
        _ => Ok(RuntimeValue::new_dict()),
    }
}

pub(super) fn capture_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return capture_re_inner(&re, input);
    }
    let re = RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|_| Error::InvalidRegularExpression(pattern.to_string()))?;
    REGEX_CACHE.write().unwrap().insert(pattern.to_string(), re.clone());
    capture_re_inner(&re, input)
}

pub(super) fn replace_re(input: &str, pattern: &str, replacement: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return Ok(re.replace_all(input, replacement).to_string().into());
    }
    let re = RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|_| Error::InvalidRegularExpression(pattern.to_string()))?;
    REGEX_CACHE.write().unwrap().insert(pattern.to_string(), re.clone());
    Ok(re.replace_all(input, replacement).to_string().into())
}

#[inline(always)]
pub(super) fn split_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return Ok(RuntimeValue::Array(
            re.split(input).map(|s| s.to_owned().into()).collect::<Vec<_>>(),
        ));
    }
    let re = Regex::new(pattern).map_err(|_| Error::InvalidRegularExpression(pattern.to_string()))?;
    REGEX_CACHE.write().unwrap().insert(pattern.to_string(), re.clone());
    Ok(RuntimeValue::Array(
        re.split(input).map(|s| s.to_owned().into()).collect::<Vec<_>>(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn strings(v: Vec<&str>) -> RuntimeValue {
        RuntimeValue::Array(v.into_iter().map(|s| RuntimeValue::String(s.to_string())).collect())
    }

    #[rstest]
    #[case("hello world", r"match_re_test_word\w*", vec![])]
    #[case("abc123", r"match_re_test_\d+", vec![])]
    #[case("hello world", r"match_re_test_hello", vec![])]
    fn test_match_re_cache_hit_same_result(#[case] input: &str, #[case] pattern: &str, #[case] expected: Vec<&str>) {
        // first call: cache miss → compiles regex
        let result1 = match_re(input, pattern).unwrap();
        assert_eq!(result1, strings(expected.clone()));
        // second call: cache hit → should produce identical result
        let result2 = match_re(input, pattern).unwrap();
        assert_eq!(result1, result2);
    }

    #[rstest]
    #[case("hello world", r"\w+", vec!["hello", "world"])]
    #[case("abc123", r"\d+", vec!["123"])]
    #[case("no digits here", r"^\d+$", vec![])]
    fn test_match_re_results(#[case] input: &str, #[case] pattern: &str, #[case] expected: Vec<&str>) {
        let result = match_re(input, pattern).unwrap();
        assert_eq!(result, strings(expected));
    }

    #[test]
    fn test_match_re_invalid_pattern() {
        assert!(match_re("text", "[invalid").is_err());
    }

    #[rstest]
    #[case("hello", r"hel+o", true)]
    #[case("world", r"^\d+$", false)]
    fn test_is_match_re(#[case] input: &str, #[case] pattern: &str, #[case] expected: bool) {
        let result = is_match_re(input, pattern).unwrap();
        assert_eq!(result, RuntimeValue::Boolean(expected));
        // second call hits cache — same result expected
        let result2 = is_match_re(input, pattern).unwrap();
        assert_eq!(result, result2);
    }

    #[test]
    fn test_is_match_re_invalid_pattern() {
        assert!(is_match_re("text", "(invalid").is_err());
    }

    #[test]
    fn test_capture_re_named_groups() {
        let pattern = r"(?P<year>\d{4})-(?P<month>\d{2})";
        let result = capture_re("2024-06", pattern).unwrap();
        // second call hits cache
        let result2 = capture_re("2024-06", pattern).unwrap();
        assert_eq!(result, result2);
        match result {
            RuntimeValue::Dict(map) => {
                assert_eq!(map[&Ident::new("year")], RuntimeValue::String("2024".to_string()));
                assert_eq!(map[&Ident::new("month")], RuntimeValue::String("06".to_string()));
            }
            other => panic!("expected Dict, got {:?}", other),
        }
    }

    #[test]
    fn test_capture_re_no_match() {
        let pattern = r"(?P<n>\d+)unique_capture_pattern_xyz";
        let result = capture_re("no numbers here", pattern).unwrap();
        assert_eq!(result, RuntimeValue::new_dict());
    }

    #[test]
    fn test_capture_re_invalid_pattern() {
        assert!(capture_re("text", "[bad").is_err());
    }

    #[rstest]
    #[case("hello world", r"\s+", "_", "hello_world")]
    #[case("aaa", "a", "b", "bbb")]
    #[case("no match", r"\d+", "X", "no match")]
    fn test_replace_re(#[case] input: &str, #[case] pattern: &str, #[case] replacement: &str, #[case] expected: &str) {
        let result = replace_re(input, pattern, replacement).unwrap();
        assert_eq!(result, RuntimeValue::String(expected.to_string()));
        // second call hits cache — same result expected
        let result2 = replace_re(input, pattern, replacement).unwrap();
        assert_eq!(result, result2);
    }

    #[test]
    fn test_replace_re_invalid_pattern() {
        assert!(replace_re("text", "[invalid", "x").is_err());
    }

    #[rstest]
    #[case("a,b,c", ",", vec!["a", "b", "c"])]
    #[case("hello", r"\s+", vec!["hello"])]
    #[case("one two three", r"\s+", vec!["one", "two", "three"])]
    fn test_split_re(#[case] input: &str, #[case] pattern: &str, #[case] expected: Vec<&str>) {
        let result = split_re(input, pattern).unwrap();
        assert_eq!(result, strings(expected.clone()));
        // second call hits cache — same result expected
        let result2 = split_re(input, pattern).unwrap();
        assert_eq!(result, result2);
    }

    #[test]
    fn test_split_re_invalid_pattern() {
        assert!(split_re("text", "[invalid").is_err());
    }
}
