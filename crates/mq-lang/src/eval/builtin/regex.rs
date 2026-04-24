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
