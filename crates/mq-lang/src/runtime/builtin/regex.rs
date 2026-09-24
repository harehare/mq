use crate::DictMap;
use crate::Ident;
use crate::Shared;
use crate::runtime::runtime_value::RuntimeValue;
use regex::{Regex, RegexBuilder};
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::sync::{LazyLock, RwLock};

use super::Error;

pub(super) static REGEX_CACHE: LazyLock<RwLock<FxHashMap<String, Regex>>> =
    LazyLock::new(|| RwLock::new(FxHashMap::with_hasher(FxBuildHasher)));

/// Maximum user-supplied regex source retained in the process-wide cache.
const MAX_REGEX_PATTERN_BYTES: usize = 16 * 1024;
/// Bounds memory retained by unique patterns from untrusted queries.
const MAX_REGEX_CACHE_ENTRIES: usize = 128;

fn compile_regex(pattern: &str) -> Result<Regex, Error> {
    if pattern.len() > MAX_REGEX_PATTERN_BYTES {
        return Err(Error::Runtime(format!(
            "regular expression pattern exceeds the maximum size of {MAX_REGEX_PATTERN_BYTES} bytes"
        )));
    }
    RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|_| Error::InvalidRegularExpression(pattern.to_string()))
}

fn cache_regex(pattern: &str, regex: Regex) {
    let mut cache = REGEX_CACHE.write().unwrap();
    if !cache.contains_key(pattern)
        && cache.len() >= MAX_REGEX_CACHE_ENTRIES
        && let Some(evicted) = cache.keys().next().cloned()
    {
        cache.remove(&evicted);
    }
    cache.insert(pattern.to_string(), regex);
}

pub(super) fn match_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        let matches: Vec<RuntimeValue> = re
            .find_iter(input)
            .map(|m| RuntimeValue::String(Shared::new(m.as_str().to_string())))
            .collect();
        return Ok(RuntimeValue::Array(Shared::new(matches)));
    }
    let re = compile_regex(pattern)?;
    cache_regex(pattern, re.clone());
    let matches: Vec<RuntimeValue> = re
        .find_iter(input)
        .map(|m| RuntimeValue::String(Shared::new(m.as_str().to_string())))
        .collect();
    Ok(RuntimeValue::Array(Shared::new(matches)))
}

pub(super) fn is_match_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return Ok(re.is_match(input).into());
    }
    let re = compile_regex(pattern)?;
    cache_regex(pattern, re.clone());
    Ok(re.is_match(input).into())
}

pub(super) fn capture_re_inner(re: &Regex, input: &str) -> Result<RuntimeValue, Error> {
    match (re.capture_names(), re.captures(input)) {
        (names, Some(caps)) => {
            let mut result = DictMap::default();
            for name in names.flatten() {
                if let Some(m) = caps.name(name) {
                    result.insert(
                        Ident::new(name),
                        RuntimeValue::String(Shared::new(m.as_str().to_string())),
                    );
                }
            }
            Ok(RuntimeValue::Dict(Shared::new(result)))
        }
        _ => Ok(RuntimeValue::new_dict()),
    }
}

pub(super) fn capture_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return capture_re_inner(&re, input);
    }
    let re = compile_regex(pattern)?;
    cache_regex(pattern, re.clone());
    capture_re_inner(&re, input)
}

pub(super) fn replace_re(input: &str, pattern: &str, replacement: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return Ok(re.replace_all(input, replacement).to_string().into());
    }
    let re = compile_regex(pattern)?;
    cache_regex(pattern, re.clone());
    Ok(re.replace_all(input, replacement).to_string().into())
}

fn scan_re_inner(re: &Regex, input: &str) -> RuntimeValue {
    let has_groups = re.captures_len() > 1;
    let matches: Vec<RuntimeValue> = re
        .captures_iter(input)
        .map(|caps| {
            if has_groups {
                RuntimeValue::Array(Shared::new(
                    caps.iter()
                        .skip(1)
                        .map(|m| {
                            m.map(|m| RuntimeValue::String(Shared::new(m.as_str().to_string())))
                                .unwrap_or(RuntimeValue::NONE)
                        })
                        .collect(),
                ))
            } else {
                RuntimeValue::String(Shared::new(
                    caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default(),
                ))
            }
        })
        .collect();
    RuntimeValue::Array(Shared::new(matches))
}

pub(super) fn scan_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return Ok(scan_re_inner(&re, input));
    }
    let re = compile_regex(pattern)?;
    cache_regex(pattern, re.clone());
    Ok(scan_re_inner(&re, input))
}

/// Caps matches collected per `regex_replace_matches` call.
const MAX_REGEX_REPLACE_MATCHES: usize = 10_000;

fn match_info(re: &Regex, caps: &regex::Captures) -> RuntimeValue {
    let m = caps.get(0).expect("capture group 0 is always present for a match");
    let mut captures = DictMap::default();
    for (index, group) in caps.iter().enumerate().skip(1) {
        if let Some(group) = group {
            captures.insert(
                Ident::new(&index.to_string()),
                RuntimeValue::String(Shared::new(group.as_str().to_string())),
            );
        }
    }
    for name in re.capture_names().flatten() {
        if let Some(group) = caps.name(name) {
            captures.insert(
                Ident::new(name),
                RuntimeValue::String(Shared::new(group.as_str().to_string())),
            );
        }
    }

    let mut fields = DictMap::default();
    fields.insert(
        Ident::new("match"),
        RuntimeValue::String(Shared::new(m.as_str().to_string())),
    );
    fields.insert(Ident::new("captures"), RuntimeValue::Dict(Shared::new(captures)));
    fields.insert(Ident::new("start"), RuntimeValue::Number((m.start() as i64).into()));
    fields.insert(Ident::new("end"), RuntimeValue::Number((m.end() as i64).into()));
    RuntimeValue::Dict(Shared::new(fields))
}

/// Splits `input` on every match of `re`. Yields `matches.len() + 1` segments, so the
/// original text is `segments[0] + matches[0] + segments[1] + ... + segments[n]`.
fn regex_replace_matches_inner(re: &Regex, input: &str) -> Result<RuntimeValue, Error> {
    let mut segments = Vec::new();
    let mut matches = Vec::new();
    let mut last_end = 0usize;

    for caps in re.captures_iter(input) {
        if matches.len() >= MAX_REGEX_REPLACE_MATCHES {
            return Err(Error::Runtime(format!(
                "regex_replace: pattern matched more than {MAX_REGEX_REPLACE_MATCHES} times"
            )));
        }
        let m = caps.get(0).expect("capture group 0 is always present for a match");
        segments.push(RuntimeValue::String(Shared::new(
            input[last_end..m.start()].to_string(),
        )));
        matches.push(match_info(re, &caps));
        last_end = m.end();
    }
    segments.push(RuntimeValue::String(Shared::new(input[last_end..].to_string())));

    let mut result = DictMap::default();
    result.insert(Ident::new("segments"), RuntimeValue::Array(Shared::new(segments)));
    result.insert(Ident::new("matches"), RuntimeValue::Array(Shared::new(matches)));
    Ok(RuntimeValue::Dict(Shared::new(result)))
}

pub(super) fn regex_replace_matches(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return regex_replace_matches_inner(&re, input);
    }
    let re = compile_regex(pattern)?;
    cache_regex(pattern, re.clone());
    regex_replace_matches_inner(&re, input)
}

/// Escapes `text` so it can be inserted literally into a regex *pattern*.
///
/// This is unrelated to escaping a *replacement* string (e.g. for `gsub`),
/// where the special character is `$` rather than the regex metacharacters
/// escaped here.
pub(super) fn regex_escape(text: &str) -> RuntimeValue {
    RuntimeValue::String(Shared::new(regex::escape(text)))
}

#[inline(always)]
pub(super) fn split_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return Ok(RuntimeValue::Array(Shared::new(
            re.split(input).map(|s| s.to_owned().into()).collect::<Vec<_>>(),
        )));
    }
    let re = compile_regex(pattern)?;
    cache_regex(pattern, re.clone());
    Ok(RuntimeValue::Array(Shared::new(
        re.split(input).map(|s| s.to_owned().into()).collect::<Vec<_>>(),
    )))
}

fn split_records_re_inner(re: &Regex, input: &str) -> Result<RuntimeValue, Error> {
    let mut records = Vec::new();
    let mut start = 0;
    let mut index = 0usize;

    for m in re.find_iter(input) {
        if m.start() == m.end() {
            return Err(Error::Runtime(
                "split_records: separator pattern must not match an empty string".to_string(),
            ));
        }

        records.push(split_record(input, index, start, m.start(), Some(m.as_str())));
        start = m.end();
        index += 1;
    }
    records.push(split_record(input, index, start, input.len(), None));

    Ok(RuntimeValue::Array(Shared::new(records)))
}

fn split_record(input: &str, index: usize, start: usize, end: usize, terminator: Option<&str>) -> RuntimeValue {
    let mut result = DictMap::default();
    result.insert(Ident::new("text"), input[start..end].to_owned().into());
    result.insert(Ident::new("index"), index.into());
    result.insert(Ident::new("start_byte"), start.into());
    result.insert(Ident::new("end_byte"), end.into());
    result.insert(
        Ident::new("terminator"),
        terminator
            .map(|t| RuntimeValue::String(Shared::new(t.to_owned())))
            .unwrap_or(RuntimeValue::None),
    );
    RuntimeValue::Dict(Shared::new(result))
}

/// Splits `input` on `pattern` like [`split_re`], but keeps each piece's byte
/// range and the separator that followed it instead of discarding them.
pub(super) fn split_records_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    if let Some(re) = REGEX_CACHE.read().unwrap().get(pattern).cloned() {
        return split_records_re_inner(&re, input);
    }
    let re = compile_regex(pattern)?;
    cache_regex(pattern, re.clone());
    split_records_re_inner(&re, input)
}

static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:https?://[^\s<>"']+)|(?:mailto:[^\s<>"']+)"#).expect("static URL regex must compile")
});

/// Caps matches collected per `extract_urls` call.
const MAX_EXTRACTED_URLS: usize = 10_000;

/// Strips trailing prose punctuation (`.,;:!?'"`) and, for closing brackets
/// (`)]}>`), only the ones left unbalanced by an opening counterpart earlier
/// in `s`, so a wiki-style URL ending in `(...)` keeps its matched pair,
/// while a URL merely wrapped in prose parens loses the stray closer.
fn trim_trailing_url_punctuation(s: &str) -> &str {
    let mut end = s.len();
    while let Some(c) = s[..end].chars().next_back() {
        match c {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' => end -= c.len_utf8(),
            ')' | ']' | '}' | '>' => {
                let open = match c {
                    ')' => '(',
                    ']' => '[',
                    '}' => '{',
                    _ => '<',
                };
                let candidate = &s[..end - c.len_utf8()];
                if candidate.matches(c).count() >= candidate.matches(open).count() {
                    end -= c.len_utf8();
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    &s[..end]
}

/// Extracts `http(s)://` and `mailto:` URLs from plain text (not Markdown links or
/// HTML) as `{url, start_byte, end_byte, kind}` records, trimming trailing prose
/// punctuation off each match. Does not detect bare domains without a scheme.
pub(super) fn extract_urls(input: &str) -> Result<RuntimeValue, Error> {
    let mut results = Vec::new();

    for m in URL_RE.find_iter(input) {
        if results.len() >= MAX_EXTRACTED_URLS {
            return Err(Error::Runtime(format!(
                "extract_urls: input contains more than {MAX_EXTRACTED_URLS} URLs"
            )));
        }

        let url = trim_trailing_url_punctuation(m.as_str());
        let kind = if url.starts_with("mailto:") { "mailto" } else { "http" };
        let start = m.start();
        let end = start + url.len();

        let mut result = DictMap::default();
        result.insert(Ident::new("url"), url.to_owned().into());
        result.insert(Ident::new("start_byte"), start.into());
        result.insert(Ident::new("end_byte"), end.into());
        result.insert(Ident::new("kind"), kind.to_owned().into());
        results.push(RuntimeValue::Dict(Shared::new(result)));
    }

    Ok(RuntimeValue::Array(Shared::new(results)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn strings(v: Vec<&str>) -> RuntimeValue {
        RuntimeValue::Array(Shared::new(
            v.into_iter()
                .map(|s| RuntimeValue::String(Shared::new(s.to_string())))
                .collect(),
        ))
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

    #[test]
    fn regex_cache_rejects_oversized_patterns_and_evicts_old_entries() {
        assert!(match_re("text", &"a".repeat(MAX_REGEX_PATTERN_BYTES + 1)).is_err());

        for index in 0..=MAX_REGEX_CACHE_ENTRIES {
            let pattern = format!("regex_cache_bound_{index}");
            assert!(is_match_re("text", &pattern).is_ok());
        }

        assert!(REGEX_CACHE.read().unwrap().len() <= MAX_REGEX_CACHE_ENTRIES);
    }

    #[rstest]
    #[case("hello", r"hel+o", true)]
    #[case("world", r"^\d+$", false)]
    fn test_is_match_re(#[case] input: &str, #[case] pattern: &str, #[case] expected: bool) {
        let result = is_match_re(input, pattern).unwrap();
        assert_eq!(result, RuntimeValue::Boolean(expected));
        // second call hits cache, same result expected
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
                assert_eq!(
                    map[&Ident::new("year")],
                    RuntimeValue::String(Shared::new("2024".to_string()))
                );
                assert_eq!(
                    map[&Ident::new("month")],
                    RuntimeValue::String(Shared::new("06".to_string()))
                );
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
        assert_eq!(result, RuntimeValue::String(Shared::new(expected.to_string())));
        // second call hits cache, same result expected
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
        // second call hits cache, same result expected
        let result2 = split_re(input, pattern).unwrap();
        assert_eq!(result, result2);
    }

    #[test]
    fn test_split_re_invalid_pattern() {
        assert!(split_re("text", "[invalid").is_err());
    }

    fn record(text: &str, index: usize, start_byte: usize, end_byte: usize, terminator: Option<&str>) -> RuntimeValue {
        let mut result = DictMap::default();
        result.insert(Ident::new("text"), text.to_string().into());
        result.insert(Ident::new("index"), index.into());
        result.insert(Ident::new("start_byte"), start_byte.into());
        result.insert(Ident::new("end_byte"), end_byte.into());
        result.insert(
            Ident::new("terminator"),
            terminator
                .map(|t| RuntimeValue::String(Shared::new(t.to_string())))
                .unwrap_or(RuntimeValue::None),
        );
        RuntimeValue::Dict(Shared::new(result))
    }

    #[test]
    fn test_split_records_re_basic() {
        let result = split_records_re("a,b,c", ",").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![
                record("a", 0, 0, 1, Some(",")),
                record("b", 1, 2, 3, Some(",")),
                record("c", 2, 4, 5, None),
            ]))
        );
        // second call hits cache, same result expected
        let result2 = split_records_re("a,b,c", ",").unwrap();
        assert_eq!(result, result2);
    }

    #[test]
    fn test_split_records_re_no_trailing_separator() {
        let result = split_records_re("a,b", ",").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![
                record("a", 0, 0, 1, Some(",")),
                record("b", 1, 2, 3, None),
            ]))
        );
    }

    #[test]
    fn test_split_records_re_no_match() {
        let result = split_records_re("abc", ",").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![record("abc", 0, 0, 3, None)]))
        );
    }

    #[test]
    fn test_split_records_re_regex_separator() {
        let result = split_records_re("a1b22c333d", "[0-9]+").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![
                record("a", 0, 0, 1, Some("1")),
                record("b", 1, 2, 3, Some("22")),
                record("c", 2, 5, 6, Some("333")),
                record("d", 3, 9, 10, None),
            ]))
        );
    }

    #[test]
    fn test_split_records_re_empty_match_is_error() {
        assert!(split_records_re("abc", "").is_err());
    }

    #[test]
    fn test_split_records_re_invalid_pattern() {
        assert!(split_records_re("text", "[invalid").is_err());
    }

    fn url_record(url: &str, start: usize, end: usize, kind: &str) -> RuntimeValue {
        let mut result = DictMap::default();
        result.insert(Ident::new("url"), url.to_string().into());
        result.insert(Ident::new("start_byte"), start.into());
        result.insert(Ident::new("end_byte"), end.into());
        result.insert(Ident::new("kind"), kind.to_string().into());
        RuntimeValue::Dict(Shared::new(result))
    }

    #[test]
    fn test_extract_urls_http_and_mailto() {
        let result = extract_urls("see https://example.com and mailto:a@b.com").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![
                url_record("https://example.com", 4, 23, "http"),
                url_record("mailto:a@b.com", 28, 42, "mailto"),
            ]))
        );
    }

    #[test]
    fn test_extract_urls_trims_trailing_prose_punctuation() {
        let result = extract_urls("Check https://example.com/page.").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![url_record("https://example.com/page", 6, 30, "http")]))
        );
    }

    #[test]
    fn test_extract_urls_trims_unbalanced_wrapping_paren() {
        let result = extract_urls("(see https://example.com)").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![url_record("https://example.com", 5, 24, "http")]))
        );
    }

    #[test]
    fn test_extract_urls_keeps_balanced_wiki_style_paren() {
        let result = extract_urls("https://en.wikipedia.org/wiki/Rust_(programming_language)").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![url_record(
                "https://en.wikipedia.org/wiki/Rust_(programming_language)",
                0,
                57,
                "http"
            )]))
        );
    }

    #[test]
    fn test_extract_urls_no_match() {
        let result = extract_urls("no urls here").unwrap();
        assert_eq!(result, RuntimeValue::empty_array());
    }

    #[test]
    fn test_extract_urls_too_many_matches_is_error() {
        let input = "https://a.co ".repeat(MAX_EXTRACTED_URLS + 1);
        assert!(extract_urls(&input).is_err());
    }

    #[test]
    fn test_scan_re_no_groups() {
        let result = scan_re("a1b2c3", r"\d").unwrap();
        assert_eq!(result, strings(vec!["1", "2", "3"]));
        // second call hits cache, same result expected
        let result2 = scan_re("a1b2c3", r"\d").unwrap();
        assert_eq!(result, result2);
    }

    #[test]
    fn test_scan_re_with_groups() {
        let result = scan_re("2024-06 2025-07", r"(\d{4})-(\d{2})").unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Array(Shared::new(vec![
                    RuntimeValue::String(Shared::new("2024".to_string())),
                    RuntimeValue::String(Shared::new("06".to_string())),
                ])),
                RuntimeValue::Array(Shared::new(vec![
                    RuntimeValue::String(Shared::new("2025".to_string())),
                    RuntimeValue::String(Shared::new("07".to_string())),
                ])),
            ]))
        );
    }

    #[test]
    fn test_scan_re_no_match() {
        let result = scan_re("no digits here", r"\d+").unwrap();
        assert_eq!(result, RuntimeValue::Array(Shared::new(vec![])));
    }

    #[test]
    fn test_scan_re_invalid_pattern() {
        assert!(scan_re("text", "[invalid").is_err());
    }

    fn dict_get(value: &RuntimeValue, key: &str) -> RuntimeValue {
        match value {
            RuntimeValue::Dict(map) => map[&Ident::new(key)].clone(),
            other => panic!("expected Dict, got {:?}", other),
        }
    }

    fn as_array(value: &RuntimeValue) -> Vec<RuntimeValue> {
        match value {
            RuntimeValue::Array(arr) => (**arr).clone(),
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[test]
    fn test_regex_replace_matches_captures() {
        let result = regex_replace_matches("hello1 world2", r"(?P<word>[a-z]+)(\d)").unwrap();
        let segments = as_array(&dict_get(&result, "segments"));
        let matches = as_array(&dict_get(&result, "matches"));
        assert_eq!(segments.len(), matches.len() + 1);
        assert_eq!(matches.len(), 2);

        let first = &matches[0];
        assert_eq!(dict_get(first, "match"), "hello1".into());
        let captures = dict_get(first, "captures");
        assert_eq!(dict_get(&captures, "1"), "hello".into());
        assert_eq!(dict_get(&captures, "word"), "hello".into());
        assert_eq!(dict_get(&captures, "2"), "1".into());
    }

    #[test]
    fn test_regex_replace_matches_byte_offsets_are_not_char_offsets() {
        // "é" is 2 UTF-8 bytes, so the digit's byte offset differs from its char index (5).
        let result = regex_replace_matches("héllo1", r"\d").unwrap();
        let matches = as_array(&dict_get(&result, "matches"));
        assert_eq!(dict_get(&matches[0], "start"), RuntimeValue::Number(6.into()));
        assert_eq!(dict_get(&matches[0], "end"), RuntimeValue::Number(7.into()));
    }

    #[test]
    fn test_regex_replace_matches_no_match_is_single_segment() {
        let result = regex_replace_matches("no digits here", r"\d+").unwrap();
        assert_eq!(as_array(&dict_get(&result, "matches")), vec![]);
        assert_eq!(as_array(&dict_get(&result, "segments")), vec!["no digits here".into()]);
    }

    #[test]
    fn test_regex_replace_matches_invalid_pattern() {
        assert!(regex_replace_matches("text", "[invalid").is_err());
    }

    #[test]
    fn test_regex_replace_matches_caps_match_count() {
        let input = "a".repeat(MAX_REGEX_REPLACE_MATCHES + 1);
        assert!(regex_replace_matches(&input, "a").is_err());
    }

    #[rstest]
    #[case("", "")]
    #[case("hello", "hello")]
    #[case("a-b", r"a\-b")]
    #[case("[abc]", r"\[abc\]")]
    #[case(r"a\b", r"a\\b")]
    #[case("a.b*c?", r"a\.b\*c\?")]
    #[case("こんにちは", "こんにちは")]
    #[case("café", "café")]
    fn test_regex_escape(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(
            regex_escape(input),
            RuntimeValue::String(Shared::new(expected.to_string()))
        );
    }

    #[test]
    fn test_regex_escape_roundtrips_as_literal_match() {
        let literal = "a.b*c?[d]-e\\f";
        let pattern = regex_escape(literal);
        let pattern = match pattern {
            RuntimeValue::String(s) => (*s).clone(),
            other => panic!("expected String, got {:?}", other),
        };
        assert_eq!(is_match_re(literal, &pattern).unwrap(), RuntimeValue::Boolean(true));
        assert_eq!(
            is_match_re("axbxcxdxe", &pattern).unwrap(),
            RuntimeValue::Boolean(false)
        );
    }
}
