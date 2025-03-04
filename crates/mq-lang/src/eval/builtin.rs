use crate::Token;
use crate::arena::Arena;
use crate::ast::node as ast;
use crate::number::Number;
use base64::prelude::*;
use compact_str::CompactString;
use itertools::Itertools;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use regex::Regex;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::process::exit;
use std::rc::Rc;
use std::{
    sync::{LazyLock, Mutex},
    vec,
};
use thiserror::Error;

use super::error::EvalError;
use super::runtime_value::RuntimeValue;
use mq_md;

static REGEX_CACHE: LazyLock<Mutex<FxHashMap<String, Regex>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

type FunctionName = String;
type ArgType = Vec<RuntimeValue>;

#[derive(Clone, Debug)]
pub struct BuiltinFunction {
    pub num_params: ParamNum,
    pub func: fn(&ast::Ident, &Vec<RuntimeValue>) -> Result<RuntimeValue, Error>,
}

#[derive(Clone, Debug)]
pub enum ParamNum {
    None,
    Fixed(u8),
    Range(u8, u8),
}

impl ParamNum {
    pub fn to_num(&self) -> u8 {
        match self {
            ParamNum::None => 0,
            ParamNum::Fixed(n) => *n,
            ParamNum::Range(min, _) => *min,
        }
    }

    #[inline(always)]
    pub fn is_valid(&self, num_args: u8) -> bool {
        match self {
            ParamNum::None => num_args == 0,
            ParamNum::Fixed(n) => num_args == *n,
            ParamNum::Range(min, max) => num_args >= *min && num_args <= *max,
        }
    }

    pub fn is_missing_one_params(&self, num_args: u8) -> bool {
        match self {
            ParamNum::Fixed(n) => num_args == n.checked_sub(1).unwrap_or_default(),
            _ => false,
        }
    }
}

impl BuiltinFunction {
    pub fn new(
        num_params: ParamNum,
        func: fn(&ast::Ident, &Vec<RuntimeValue>) -> Result<RuntimeValue, Error>,
    ) -> Self {
        BuiltinFunction { num_params, func }
    }
}

pub static BUILTIN_FUNCTIONS: LazyLock<FxHashMap<CompactString, BuiltinFunction>> =
    LazyLock::new(|| {
        let mut map = FxHashMap::default();

        map.insert(
            CompactString::new("halt"),
            BuiltinFunction::new(ParamNum::None, |ident, args| match args.as_slice() {
                [RuntimeValue::Number(exit_code)] => exit(exit_code.value() as i32),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("debug"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [runtime_value] => {
                    eprintln!("DEBUG: {}", runtime_value);
                    Ok(args.first().unwrap().clone())
                }
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("type"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| {
                Ok(args.first().unwrap().name().to_string().into())
            }),
        );
        map.insert(
            CompactString::new("array"),
            BuiltinFunction::new(ParamNum::Range(0, u8::MAX), |_, args| {
                Ok(RuntimeValue::Array(args.iter().cloned().collect_vec()))
            }),
        );
        map.insert(
            CompactString::new("from_date"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(date_str)] => from_date(date_str),
                [RuntimeValue::Markdown(node_value)] => from_date(node_value.value().as_str()),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_date"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(ms), RuntimeValue::String(format)] => {
                    to_date(*ms, Some(format.as_str()))
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),

                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("now"),
            BuiltinFunction::new(ParamNum::None, |_, _| {
                Ok(RuntimeValue::Number(
                    (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| Error::Runtime(format!("{}", e)))?
                        .as_millis() as i64)
                        .into(),
                ))
            }),
        );
        map.insert(
            CompactString::new("base64"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s)] => base64(s),
                [RuntimeValue::Markdown(node_value)] => base64(node_value.value().as_str())
                    .and_then(|b| match b {
                        RuntimeValue::String(s) => Ok(node_value.with_value(&s).into()),
                        a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                    }),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("base64d"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s)] => base64d(s),
                [RuntimeValue::Markdown(node_value)] => base64d(node_value.value().as_str())
                    .and_then(|o| match o {
                        RuntimeValue::String(s) => Ok(node_value.with_value(&s).into()),
                        a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                    }),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("min"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
                    Ok(std::cmp::min(*n1, *n2).into())
                }
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                    Ok(std::cmp::min(s1.to_string(), s2.to_string()).into())
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("max"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
                    Ok(std::cmp::max(*n1, *n2).into())
                }
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                    Ok(std::cmp::max(s1.to_string(), s2.to_string()).into())
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_html"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(mq_md::to_html(s).into()),
                [RuntimeValue::Markdown(node_value)] => {
                    Ok(mq_md::to_html(node_value.to_string().as_str()).into())
                }
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_csv"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => Ok(array.iter().join(",").into()),
                [a] => Ok(a.to_string().into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_tsv"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => Ok(array.iter().join("\t").into()),
                [a] => Ok(a.to_string().into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_string"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::Markdown(node_value)] => Ok(node_value.to_string().into()),
                [RuntimeValue::Array(array)] => {
                    let result_value: Result<Vec<RuntimeValue>, Error> = array
                        .clone()
                        .into_iter()
                        .map(|o| match o {
                            RuntimeValue::Markdown(node_value) => Ok(node_value.to_string().into()),
                            _ => Ok(o.to_string().into()),
                        })
                        .collect();

                    result_value.map(RuntimeValue::Array)
                }
                [o] => Ok(o.to_string().into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_number"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Markdown(node_value)] => node_value
                    .to_string()
                    .parse::<f64>()
                    .map(|n| RuntimeValue::Number(n.into()))
                    .map_err(|e| Error::Runtime(format!("{}", e))),
                [RuntimeValue::String(s)] => s
                    .parse::<f64>()
                    .map(|n| RuntimeValue::Number(n.into()))
                    .map_err(|e| Error::Runtime(format!("{}", e))),
                [RuntimeValue::Array(array)] => {
                    let result_value: Result<Vec<RuntimeValue>, Error> = array
                        .clone()
                        .into_iter()
                        .map(|o| match o {
                            RuntimeValue::Markdown(node_value) => node_value
                                .to_string()
                                .parse::<f64>()
                                .map(|n| RuntimeValue::Number(n.into()))
                                .map_err(|e| Error::Runtime(format!("{}", e))),
                            RuntimeValue::String(s) => s
                                .parse::<f64>()
                                .map(|n| RuntimeValue::Number(n.into()))
                                .map_err(|e| Error::Runtime(format!("{}", e))),
                            RuntimeValue::Bool(b) => {
                                Ok(RuntimeValue::Number(if b { 1 } else { 0 }.into()))
                            }
                            a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                        })
                        .collect();

                    result_value.map(RuntimeValue::Array)
                }
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("url_encode"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s)] => url_encode(s),
                [RuntimeValue::Markdown(node_value)] => url_encode(node_value.value().as_str())
                    .and_then(|o| match o {
                        RuntimeValue::String(s) => Ok(node_value.with_value(&s).into()),
                        a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                    }),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_text"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::None] => Ok("".to_owned().into()),
                [RuntimeValue::Markdown(node_value)] => Ok(node_value.value().into()),
                [RuntimeValue::Array(array)] => Ok(array
                    .iter()
                    .map(|a| {
                        if a.is_none() {
                            "".to_string()
                        } else {
                            a.to_string()
                        }
                    })
                    .join(",")
                    .into()),
                [value] => Ok(value.to_string().into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("ends_with"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Markdown(node_value), RuntimeValue::String(s)] => {
                    Ok(node_value.value().ends_with(s).into())
                }
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.ends_with(s2).into()),
                [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
                    .last()
                    .map_or(Ok(RuntimeValue::FALSE), |o| {
                        eval_builtin(o, ident, &vec![RuntimeValue::String(s.clone())])
                    })
                    .unwrap_or(RuntimeValue::FALSE)),
                [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("starts_with"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Markdown(node_value), RuntimeValue::String(s)] => {
                    Ok(node_value.value().starts_with(s).into())
                }
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                    Ok(s1.starts_with(s2).into())
                }
                [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
                    .first()
                    .map_or(Ok(RuntimeValue::FALSE), |o| {
                        eval_builtin(o, ident, &vec![RuntimeValue::String(s.clone())])
                    })
                    .unwrap_or(RuntimeValue::FALSE)),
                [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("match"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s), RuntimeValue::String(pattern)] => match_re(s, pattern),
                [
                    RuntimeValue::Markdown(node_value),
                    RuntimeValue::String(pattern),
                ] => match_re(&node_value.value(), pattern),
                [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("downcase"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::Markdown(node_value)] => Ok(node_value
                    .with_value(node_value.value().to_lowercase().as_str())
                    .into()),
                [RuntimeValue::String(s)] => Ok(s.to_lowercase().into()),
                [_] => Ok(RuntimeValue::NONE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("gsub"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, args| match args.as_slice() {
                [
                    RuntimeValue::String(s1),
                    RuntimeValue::String(s2),
                    RuntimeValue::String(s3),
                ] => Ok(replace_re(s1, s2, s3)?),
                [
                    RuntimeValue::Markdown(node_value),
                    RuntimeValue::String(s1),
                    RuntimeValue::String(s2),
                ] => Ok(node_value
                    .with_value(
                        &replace_re(node_value.value().as_str(), s1.as_str(), s2.as_str())?
                            .to_string(),
                    )
                    .into()),
                [
                    RuntimeValue::None,
                    RuntimeValue::String(_),
                    RuntimeValue::String(_),
                ] => Ok(RuntimeValue::None),
                [a, b, c] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone(), c.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("replace"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, args| match args.as_slice() {
                [
                    RuntimeValue::String(s1),
                    RuntimeValue::String(s2),
                    RuntimeValue::String(s3),
                ] => Ok(s1.replace(s2, s3).into()),
                [
                    RuntimeValue::Markdown(node_value),
                    RuntimeValue::String(s1),
                    RuntimeValue::String(s2),
                ] => Ok(node_value
                    .with_value(
                        node_value
                            .value()
                            .replace(s1.as_str(), s2.as_str())
                            .as_str(),
                    )
                    .into()),
                [
                    RuntimeValue::None,
                    RuntimeValue::String(_),
                    RuntimeValue::String(_),
                ] => Ok(RuntimeValue::None),
                [a, b, c] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone(), c.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("repeat"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s), RuntimeValue::Number(n)] if !n.is_zero() => {
                    Ok(s.repeat(n.value() as usize).into())
                }
                [RuntimeValue::Markdown(node_value), RuntimeValue::Number(n)] if !n.is_zero() => {
                    Ok(node_value
                        .with_value(node_value.value().repeat(n.value() as usize).as_str())
                        .into())
                }
                [RuntimeValue::None, _] => Ok(RuntimeValue::None),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("explode"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(
                    s.chars()
                        .map(|c| RuntimeValue::Number((c as u32).into()))
                        .collect_vec(),
                )),
                [RuntimeValue::Markdown(node_value)] => Ok(RuntimeValue::Array(
                    node_value
                        .value()
                        .chars()
                        .map(|c| RuntimeValue::Number((c as u32).into()))
                        .collect_vec(),
                )),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("implode"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => {
                    let result: String = array
                        .iter()
                        .map(|o| match o {
                            RuntimeValue::Number(n) => std::char::from_u32(n.value() as u32)
                                .unwrap_or_default()
                                .to_string(),
                            _ => "".to_string(),
                        })
                        .collect();
                    Ok(result.into())
                }
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("trim"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(s.trim().to_string().into()),
                [RuntimeValue::Markdown(node_value)] => {
                    Ok(node_value.with_value(node_value.to_string().trim()).into())
                }
                [RuntimeValue::None] => Ok(RuntimeValue::None),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("upcase"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Markdown(node_value)] => Ok(node_value
                    .with_value(node_value.value().to_uppercase().as_str())
                    .into()),
                [RuntimeValue::String(s)] => Ok(s.to_uppercase().into()),
                [RuntimeValue::None] => Ok(RuntimeValue::None),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("slice"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, args| match args.as_slice() {
                [
                    RuntimeValue::String(s),
                    RuntimeValue::Number(start),
                    RuntimeValue::Number(end),
                ] => {
                    let start = start.value() as usize;
                    let end = end.value() as usize;

                    let sub: String = s
                        .chars()
                        .enumerate()
                        .filter(|&(i, _)| i >= start && i < end)
                        .fold("".to_string(), |s, (_, c)| format!("{}{}", s, c));

                    Ok(sub.into())
                }
                [
                    RuntimeValue::Markdown(node_value),
                    RuntimeValue::Number(start),
                    RuntimeValue::Number(end),
                ] => {
                    let start = start.value() as usize;
                    let end = end.value() as usize;

                    let sub: String = node_value
                        .value()
                        .chars()
                        .enumerate()
                        .filter(|&(i, _)| i >= start && i < end)
                        .fold("".to_string(), |s, (_, c)| format!("{}{}", s, c));

                    Ok(node_value.with_value(&sub).into())
                }
                [
                    RuntimeValue::None,
                    RuntimeValue::Number(_),
                    RuntimeValue::Number(_),
                ] => Ok(RuntimeValue::NONE),
                [a, b, c] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone(), c.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("pow"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(base), RuntimeValue::Number(exp)] => Ok(
                    RuntimeValue::Number((base.value() as i64).pow(exp.value() as u32).into()),
                ),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("index"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(
                    (s1.find(s2).map(|v| v as isize).unwrap_or_else(|| -1) as i64).into(),
                )),
                [RuntimeValue::Markdown(node_value), RuntimeValue::String(s)] => {
                    Ok(RuntimeValue::Number(
                        (node_value
                            .value()
                            .find(s)
                            .map(|v| v as isize)
                            .unwrap_or_else(|| -1) as i64)
                            .into(),
                    ))
                }
                [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
                    .iter()
                    .position(|o| match o {
                        RuntimeValue::String(s1) => s1 == s,
                        _ => false,
                    })
                    .map(|i| RuntimeValue::Number((i as i64).into()))
                    .unwrap_or(RuntimeValue::Number((-1_i64).into()))),
                [RuntimeValue::None, _] => Ok(RuntimeValue::Number((-1_i64).into())),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("len"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.chars().count().into())),
                [RuntimeValue::Markdown(node_value)] => Ok(RuntimeValue::Number(
                    node_value.value().chars().count().into(),
                )),
                [RuntimeValue::Array(array)] => Ok(RuntimeValue::Number(array.len().into())),
                [RuntimeValue::None] => Ok(RuntimeValue::Number(0.into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("utf8bytelen"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.len().into())),
                [RuntimeValue::Markdown(node_value)] => {
                    Ok(RuntimeValue::Number(node_value.value().len().into()))
                }
                [RuntimeValue::Array(array)] => Ok(RuntimeValue::Number(array.len().into())),
                [RuntimeValue::None] => Ok(RuntimeValue::Number(0.into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );

        map.insert(
            CompactString::new("rindex"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(
                    s1.rfind(s2)
                        .map(|v| v as isize)
                        .unwrap_or_else(|| -1)
                        .into(),
                )),
                [RuntimeValue::Markdown(node_value), RuntimeValue::String(s)] => {
                    Ok(RuntimeValue::Number(
                        node_value
                            .value()
                            .rfind(s)
                            .map(|v| v as isize)
                            .unwrap_or_else(|| -1)
                            .into(),
                    ))
                }
                [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
                    .iter()
                    .rposition(|o| match o {
                        RuntimeValue::String(s1) => s1 == s,
                        _ => false,
                    })
                    .map(|i| RuntimeValue::Number(i.into()))
                    .unwrap_or(RuntimeValue::Number((-1_i64).into()))),
                [RuntimeValue::None, RuntimeValue::String(_)] => {
                    Ok(RuntimeValue::Number((-1_i64).into()))
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("nth"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Array(array), RuntimeValue::Number(n)] => {
                    match array.get(n.value() as usize) {
                        Some(o) => Ok(o.clone()),
                        None => Ok(RuntimeValue::None),
                    }
                }
                [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
                    match s.chars().nth(n.value() as usize) {
                        Some(o) => Ok(o.to_string().into()),
                        None => Ok(RuntimeValue::None),
                    }
                }
                [RuntimeValue::None, RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("del"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Array(array), RuntimeValue::Number(n)] => {
                    let mut array = array.clone();
                    array.remove(n.value() as usize);
                    Ok(RuntimeValue::Array(array))
                }
                [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
                    let mut s = s.clone().chars().collect_vec();
                    s.remove(n.value() as usize);
                    Ok(s.into_iter().collect::<String>().into())
                }
                [RuntimeValue::None, RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("join"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Array(array), RuntimeValue::String(s)] => {
                    Ok(array.iter().join(s).into())
                }
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("reverse"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => {
                    let mut vec = array.to_vec();
                    vec.reverse();
                    Ok(RuntimeValue::Array(vec))
                }
                [RuntimeValue::String(s)] => Ok(s.chars().rev().collect::<String>().into()),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("sort"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => {
                    let mut vec = array.to_vec();
                    vec.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    Ok(RuntimeValue::Array(vec))
                }
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("compact"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(
                    array.iter().filter(|v| !v.is_none()).cloned().collect_vec(),
                )),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("range"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(start), RuntimeValue::Number(end)] => {
                    let range: Vec<RuntimeValue> = ((start.value() as u64)..(end.value() as u64))
                        .map(|n| RuntimeValue::Number(n.into()))
                        .collect();
                    Ok(RuntimeValue::Array(range))
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("split"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(split_re(s1, s2)?),
                [RuntimeValue::Markdown(node_value), RuntimeValue::String(s)] => {
                    Ok(split_re(node_value.value().as_str(), s)?)
                }
                [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::EMPTY_ARRAY),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("uniq"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => {
                    let mut vec = array.to_vec();
                    vec.dedup();
                    Ok(RuntimeValue::Array(vec))
                }
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("ceil"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ceil().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("floor"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().floor().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("round"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().round().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("trunc"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().trunc().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("abs"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().abs().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("eq"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [a, b] => Ok((a == b).into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("ne"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [a, b] => Ok((a != b).into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("gt"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 > s2).into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 > n2).into()),
                [RuntimeValue::Bool(b1), RuntimeValue::Bool(b2)] => Ok((b1 > b2).into()),
                [RuntimeValue::Markdown(n1), RuntimeValue::Markdown(n2)] => Ok((n1 == n2).into()),
                [_, _] => Ok(RuntimeValue::FALSE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("gte"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 >= s2).into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 >= n2).into()),
                [RuntimeValue::Bool(b1), RuntimeValue::Bool(b2)] => Ok((b1 >= b2).into()),
                [RuntimeValue::Markdown(n1), RuntimeValue::Markdown(n2)] => Ok((n1 == n2).into()),
                [_, _] => Ok(RuntimeValue::FALSE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("lt"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 < s2).into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 < n2).into()),
                [RuntimeValue::Bool(b1), RuntimeValue::Bool(b2)] => Ok((b1 < b2).into()),
                [RuntimeValue::Markdown(n1), RuntimeValue::Markdown(n2)] => Ok((n1 == n2).into()),
                [_, _] => Ok(RuntimeValue::FALSE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("lte"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 <= s2).into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 <= n2).into()),
                [RuntimeValue::Bool(b1), RuntimeValue::Bool(b2)] => Ok((b1 <= b2).into()),
                [RuntimeValue::Markdown(n1), RuntimeValue::Markdown(n2)] => Ok((n1 == n2).into()),
                [_, _] => Ok(RuntimeValue::FALSE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("add"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                    Ok(format!("{}{}", s1, s2).into())
                }
                [RuntimeValue::Markdown(node_value), RuntimeValue::String(s)] => Ok(node_value
                    .with_value(format!("{}{}", node_value.value(), s).as_str())
                    .into()),
                [RuntimeValue::String(s), RuntimeValue::Markdown(node_value)] => Ok(node_value
                    .with_value(format!("{}{}", s, node_value.value()).as_str())
                    .into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 + *n2).into()),
                [RuntimeValue::Array(a1), RuntimeValue::Array(a2)] => {
                    let a1: Vec<RuntimeValue> = a1.to_vec();
                    let a2: Vec<RuntimeValue> = a2.to_vec();
                    Ok(RuntimeValue::Array(itertools::concat(vec![a1, a2])))
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("sub"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 - *n2).into()),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("div"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
                    if n2.is_zero() {
                        Err(Error::ZeroDivision)
                    } else {
                        Ok((*n1 / *n2).into())
                    }
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("mul"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 * *n2).into()),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("mod"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 % *n2).into()),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("and"),
            BuiltinFunction::new(ParamNum::Range(2, 255), |_, args| {
                Ok(args.iter().all(|arg| arg.is_true()).into())
            }),
        );
        map.insert(
            CompactString::new("or"),
            BuiltinFunction::new(ParamNum::Range(2, 255), |_, args| {
                Ok(args.iter().any(|arg| arg.is_true()).into())
            }),
        );
        map.insert(
            CompactString::new("not"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [a] => Ok((!a.is_true()).into()),
                _ => unreachable!(),
            }),
        );

        // markdown
        map.insert(
            CompactString::new("md_code"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [a, RuntimeValue::String(lang)] => Ok(mq_md::Node::Code(mq_md::Code {
                    value: a.to_string(),
                    lang: Some(lang.to_string()),
                    position: None,
                })
                .into()),
                [a, RuntimeValue::None] => Ok(mq_md::Node::Code(mq_md::Code {
                    value: a.to_string(),
                    lang: None,
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_code_inline"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [a] => Ok(mq_md::Node::CodeInline(mq_md::CodeInline {
                    value: a.to_string(),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_h"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [RuntimeValue::Markdown(node), RuntimeValue::Number(depth)] => {
                    Ok(mq_md::Node::Heading(mq_md::Heading {
                        depth: (*depth).value() as u8,
                        value: node.node_value(),
                        position: None,
                    })
                    .into())
                }
                [a, RuntimeValue::Number(depth)] => Ok(mq_md::Node::Heading(mq_md::Heading {
                    depth: (*depth).value() as u8,
                    value: Box::new(a.to_string().into()),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_hr"),
            BuiltinFunction::new(ParamNum::Fixed(0), |_, _| {
                Ok(mq_md::Node::HorizontalRule { position: None }.into())
            }),
        );
        map.insert(
            CompactString::new("md_link"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, args| match args.as_slice() {
                [RuntimeValue::String(url), RuntimeValue::String(title)] => {
                    Ok(mq_md::Node::Link(mq_md::Link {
                        url: url.to_string(),
                        title: Some(title.to_string()),
                        position: None,
                    })
                    .into())
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("md_image"),
            BuiltinFunction::new(ParamNum::Fixed(3), |_, args| match args.as_slice() {
                [
                    RuntimeValue::String(url),
                    RuntimeValue::String(alt),
                    RuntimeValue::String(title),
                ] => Ok(mq_md::Node::Image(mq_md::Image {
                    alt: alt.to_string(),
                    url: url.to_string(),
                    title: Some(title.to_string()),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_math"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [a] => Ok(mq_md::Node::Math(mq_md::Math {
                    value: a.to_string(),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_math_inline"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [a] => Ok(mq_md::Node::MathInline(mq_md::MathInline {
                    value: a.to_string(),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_name"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::Markdown(m)] => Ok(m.name().to_string().into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_strong"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::Markdown(node)] => Ok(mq_md::Node::Strong(mq_md::Value {
                    value: node.node_value(),
                    position: None,
                })
                .into()),
                [a] => Ok(mq_md::Node::Strong(mq_md::Value {
                    value: Box::new(a.to_string().into()),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_em"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::Markdown(node)] => Ok(mq_md::Node::Emphasis(mq_md::Value {
                    value: node.node_value(),
                    position: None,
                })
                .into()),
                [a] => Ok(mq_md::Node::Emphasis(mq_md::Value {
                    value: Box::new(a.to_string().into()),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_text"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [a] => Ok(mq_md::Node::Text(mq_md::Text {
                    value: a.to_string(),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_list"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [RuntimeValue::Markdown(node), RuntimeValue::Number(level)] => {
                    Ok(mq_md::Node::List(mq_md::List {
                        value: node.node_value(),
                        index: 0,
                        level: level.value() as u8,
                        checked: None,
                        position: None,
                    })
                    .into())
                }
                [a, RuntimeValue::Number(level)] => Ok(mq_md::Node::List(mq_md::List {
                    value: Box::new(a.to_string().into()),
                    index: 0,
                    level: level.value() as u8,
                    checked: None,
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("md_list_level"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, args| match args.as_slice() {
                [RuntimeValue::Markdown(mq_md::Node::List(mq_md::List { level, .. }))] => {
                    Ok(RuntimeValue::Number((*level).into()))
                }
                [a] => Ok(a.clone()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("md_check"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, args| match args.as_slice() {
                [
                    RuntimeValue::Markdown(mq_md::Node::List(list)),
                    RuntimeValue::Bool(checked),
                ] => Ok(mq_md::Node::List(mq_md::List {
                    checked: Some(*checked),
                    ..list.clone()
                })
                .into()),
                [a, ..] => Ok(a.clone()),
                _ => Ok(RuntimeValue::None),
            }),
        );

        map
    });

#[derive(Clone, Debug)]
pub struct BuiltinSelectorDoc {
    pub description: &'static str,
    pub params: &'static [&'static str],
}

pub static BUILTIN_SELECTOR_DOC: LazyLock<FxHashMap<CompactString, BuiltinSelectorDoc>> =
    LazyLock::new(|| {
        let mut map = FxHashMap::default();

        map.insert(
            CompactString::new(".h"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the specified depth.",
                params: &["depth"],
            },
        );

        map.insert(
            CompactString::new(".h1"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 1 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".h2"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 2 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".h3"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 3 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".h4"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 4 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".h5"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 5 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".#"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 1 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".##"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 2 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".###"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 3 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".####"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 4 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".#####"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 5 depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".code"),
            BuiltinSelectorDoc {
                description: "Selects a code block node with the specified language.",
                params: &["lang"],
            },
        );

        map.insert(
            CompactString::new(".code_inline"),
            BuiltinSelectorDoc {
                description: "Selects an inline code node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".inline_math"),
            BuiltinSelectorDoc {
                description: "Selects an inline math node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".strong"),
            BuiltinSelectorDoc {
                description: "Selects a strong (bold) node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".emphasis"),
            BuiltinSelectorDoc {
                description: "Selects an emphasis (italic) node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".delete"),
            BuiltinSelectorDoc {
                description: "Selects a delete (strikethrough) node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".link"),
            BuiltinSelectorDoc {
                description: "Selects a link node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".link_ref"),
            BuiltinSelectorDoc {
                description: "Selects a link reference node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".image"),
            BuiltinSelectorDoc {
                description: "Selects an image node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".heading"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the specified depth.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".horizontal_rule"),
            BuiltinSelectorDoc {
                description: "Selects a horizontal rule node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".blockquote"),
            BuiltinSelectorDoc {
                description: "Selects a blockquote node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".[][]"),
            BuiltinSelectorDoc {
                description: "Selects a table cell node with the specified row and column.",
                params: &["row", "column"],
            },
        );

        map.insert(
            CompactString::new(".html"),
            BuiltinSelectorDoc {
                description: "Selects an HTML node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".<>"),
            BuiltinSelectorDoc {
                description: "Selects an HTML node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".footnote"),
            BuiltinSelectorDoc {
                description: "Selects a footnote node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".mdx_jsx_flow_element"),
            BuiltinSelectorDoc {
                description: "Selects an MDX JSX flow element node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".list"),
            BuiltinSelectorDoc {
                description: "Selects a list node with the specified index and checked state.",
                params: &["indent", "checked"],
            },
        );

        map.insert(
            CompactString::new(".mdx_js_esm"),
            BuiltinSelectorDoc {
                description: "Selects an MDX JS ESM node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".toml"),
            BuiltinSelectorDoc {
                description: "Selects a TOML node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".yaml"),
            BuiltinSelectorDoc {
                description: "Selects a YAML node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".break"),
            BuiltinSelectorDoc {
                description: "Selects a break node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".mdx_text_expression"),
            BuiltinSelectorDoc {
                description: "Selects an MDX text expression node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".footnote_ref"),
            BuiltinSelectorDoc {
                description: "Selects a footnote reference node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".image_ref"),
            BuiltinSelectorDoc {
                description: "Selects an image reference node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".mdx_jsx_text_element"),
            BuiltinSelectorDoc {
                description: "Selects an MDX JSX text element node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".math"),
            BuiltinSelectorDoc {
                description: "Selects a math node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".math_inline"),
            BuiltinSelectorDoc {
                description: "Selects a math inline node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".mdx_flow_expression"),
            BuiltinSelectorDoc {
                description: "Selects an MDX flow expression node.",
                params: &[],
            },
        );

        map.insert(
            CompactString::new(".definition"),
            BuiltinSelectorDoc {
                description: "Selects a definition node.",
                params: &[],
            },
        );

        map
    });

#[derive(Clone, Debug)]
pub struct BuiltinFunctionDoc {
    pub description: &'static str,
    pub params: &'static [&'static str],
}

pub static BUILTIN_FUNCTION_DOC: LazyLock<FxHashMap<CompactString, BuiltinFunctionDoc>> =
    LazyLock::new(|| {
        let mut map = FxHashMap::default();

        map.insert(
            CompactString::new("halt"),
            BuiltinFunctionDoc {
                description: "Terminates the program with the given exit code.",
                params: &["exit_code"],
            },
        );
        map.insert(
            CompactString::new("debug"),
            BuiltinFunctionDoc {
                description: "Prints the debug information of the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("type"),
            BuiltinFunctionDoc {
                description: "Returns the type of the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("array"),
            BuiltinFunctionDoc {
                description: "Creates an array from the given values.",
                params: &["values"],
            },
        );
        map.insert(
            CompactString::new("from_date"),
            BuiltinFunctionDoc {
                description: "Converts a date string to a timestamp.",
                params: &["date_str"],
            },
        );
        map.insert(
            CompactString::new("to_date"),
            BuiltinFunctionDoc {
                description: "Converts a timestamp to a date string with the given format.",
                params: &["timestamp", "format"],
            },
        );
        map.insert(
            CompactString::new("now"),
            BuiltinFunctionDoc {
                description: "Returns the current timestamp.",
                params: &[],
            },
        );
        map.insert(
            CompactString::new("base64"),
            BuiltinFunctionDoc {
                description: "Encodes the given string to base64.",
                params: &["input"],
            },
        );
        map.insert(
            CompactString::new("base64d"),
            BuiltinFunctionDoc {
                description: "Decodes the given base64 string.",
                params: &["input"],
            },
        );
        map.insert(
            CompactString::new("min"),
            BuiltinFunctionDoc {
                description: "Returns the minimum of two values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("max"),
            BuiltinFunctionDoc {
                description: "Returns the maximum of two values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("to_html"),
            BuiltinFunctionDoc {
                description: "Converts the given markdown string to HTML.",
                params: &["markdown"],
            },
        );
        map.insert(
            CompactString::new("to_csv"),
            BuiltinFunctionDoc {
                description: "Converts the given value to a CSV.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_tsv"),
            BuiltinFunctionDoc {
                description: "Converts the given value to a TSV.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_string"),
            BuiltinFunctionDoc {
                description: "Converts the given value to a string.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_number"),
            BuiltinFunctionDoc {
                description: "Converts the given value to a number.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("url_encode"),
            BuiltinFunctionDoc {
                description: "URL-encodes the given string.",
                params: &["input"],
            },
        );
        map.insert(
            CompactString::new("to_text"),
            BuiltinFunctionDoc {
                description: "Converts the given markdown node to plain text.",
                params: &["markdown"],
            },
        );
        map.insert(
            CompactString::new("ends_with"),
            BuiltinFunctionDoc {
                description: "Checks if the given string ends with the specified substring.",
                params: &["string", "substring"],
            },
        );
        map.insert(
            CompactString::new("starts_with"),
            BuiltinFunctionDoc {
                description: "Checks if the given string starts with the specified substring.",
                params: &["string", "substring"],
            },
        );
        map.insert(
            CompactString::new("match"),
            BuiltinFunctionDoc {
                description: "Finds all matches of the given pattern in the string.",
                params: &["string", "pattern"],
            },
        );
        map.insert(
            CompactString::new("downcase"),
            BuiltinFunctionDoc {
                description: "Converts the given string to lowercase.",
                params: &["input"],
            },
        );
        map.insert(
            CompactString::new("gsub"),
            BuiltinFunctionDoc {
                description: "Replaces all occurrences matching a regular expression pattern with the replacement string.",
                params: &["pattern", "from", "to"],
            },
        );
        map.insert(
            CompactString::new("replace"),
            BuiltinFunctionDoc {
                description: "Replaces all occurrences of a substring with another substring.",
                params: &["string", "from", "to"],
            },
        );
        map.insert(
            CompactString::new("repeat"),
            BuiltinFunctionDoc {
                description: "Repeats the given string a specified number of times.",
                params: &["string", "count"],
            },
        );
        map.insert(
            CompactString::new("explode"),
            BuiltinFunctionDoc {
                description: "Splits the given string into an array of characters.",
                params: &["string"],
            },
        );
        map.insert(
            CompactString::new("implode"),
            BuiltinFunctionDoc {
                description: "Joins an array of characters into a string.",
                params: &["array"],
            },
        );
        map.insert(
            CompactString::new("trim"),
            BuiltinFunctionDoc {
                description: "Trims whitespace from both ends of the given string.",
                params: &["input"],
            },
        );
        map.insert(
            CompactString::new("upcase"),
            BuiltinFunctionDoc {
                description: "Converts the given string to uppercase.",
                params: &["input"],
            },
        );
        map.insert(
            CompactString::new("slice"),
            BuiltinFunctionDoc {
                description: "Extracts a substring from the given string.",
                params: &["string", "start", "end"],
            },
        );
        map.insert(
            CompactString::new("pow"),
            BuiltinFunctionDoc {
                description: "Raises the base to the power of the exponent.",
                params: &["base", "exponent"],
            },
        );
        map.insert(
            CompactString::new("index"),
            BuiltinFunctionDoc {
                description: "Finds the first occurrence of a substring in the given string.",
                params: &["string", "substring"],
            },
        );
        map.insert(
            CompactString::new("len"),
            BuiltinFunctionDoc {
                description: "Returns the length of the given string or array.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("rindex"),
            BuiltinFunctionDoc {
                description: "Finds the last occurrence of a substring in the given string.",
                params: &["string", "substring"],
            },
        );
        map.insert(
            CompactString::new("nth"),
            BuiltinFunctionDoc {
                description: "Gets the element at the specified index in the array or string.",
                params: &["array_or_string", "index"],
            },
        );
        map.insert(
            CompactString::new("join"),
            BuiltinFunctionDoc {
                description:
                    "Joins the elements of an array into a string with the given separator.",
                params: &["array", "separator"],
            },
        );
        map.insert(
            CompactString::new("reverse"),
            BuiltinFunctionDoc {
                description: "Reverses the given string or array.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("sort"),
            BuiltinFunctionDoc {
                description: "Sorts the elements of the given array.",
                params: &["array"],
            },
        );
        map.insert(
            CompactString::new("compact"),
            BuiltinFunctionDoc {
                description: "Removes None values from the given array.",
                params: &["array"],
            },
        );
        map.insert(
            CompactString::new("range"),
            BuiltinFunctionDoc {
                description: "Creates an array of numbers within the specified range.",
                params: &["start", "end"],
            },
        );
        map.insert(
            CompactString::new("split"),
            BuiltinFunctionDoc {
                description: "Splits the given string by the specified separator.",
                params: &["string", "separator"],
            },
        );
        map.insert(
            CompactString::new("uniq"),
            BuiltinFunctionDoc {
                description: "Removes duplicate elements from the given array.",
                params: &["array"],
            },
        );
        map.insert(
            CompactString::new("eq"),
            BuiltinFunctionDoc {
                description: "Checks if two values are equal.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("ne"),
            BuiltinFunctionDoc {
                description: "Checks if two values are not equal.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("gt"),
            BuiltinFunctionDoc {
                description: "Checks if the first value is greater than the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("gte"),
            BuiltinFunctionDoc {
                description:
                    "Checks if the first value is greater than or equal to the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("lt"),
            BuiltinFunctionDoc {
                description: "Checks if the first value is less than the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("lte"),
            BuiltinFunctionDoc {
                description: "Checks if the first value is less than or equal to the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("add"),
            BuiltinFunctionDoc {
                description: "Adds two values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("sub"),
            BuiltinFunctionDoc {
                description: "Subtracts the second value from the first value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("div"),
            BuiltinFunctionDoc {
                description: "Divides the first value by the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("mul"),
            BuiltinFunctionDoc {
                description: "Multiplies two values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("mod"),
            BuiltinFunctionDoc {
                description: "Calculates the remainder of the division of the first value by the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("and"),
            BuiltinFunctionDoc {
                description: "Performs a logical AND operation on two boolean values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("or"),
            BuiltinFunctionDoc {
                description: "Performs a logical OR operation on two boolean values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new("not"),
            BuiltinFunctionDoc {
                description: "Performs a logical NOT operation on a boolean value.",
                params: &["value"],
            },
        );

        map.insert(
            CompactString::new("round"),
            BuiltinFunctionDoc {
                description: "Rounds the given number to the nearest integer.",
                params: &["number"],
            },
        );
        map.insert(
            CompactString::new("trunc"),
            BuiltinFunctionDoc {
                description:
                    "Truncates the given number to an integer by removing the fractional part.",
                params: &["number"],
            },
        );
        map.insert(
            CompactString::new("ceil"),
            BuiltinFunctionDoc {
                description: "Rounds the given number up to the nearest integer.",
                params: &["number"],
            },
        );
        map.insert(
            CompactString::new("floor"),
            BuiltinFunctionDoc {
                description: "Rounds the given number down to the nearest integer.",
                params: &["number"],
            },
        );
        map.insert(
            CompactString::new("del"),
            BuiltinFunctionDoc {
                description: "Deletes the element at the specified index in the array or string.",
                params: &["array_or_string", "index"],
            },
        );
        map.insert(
            CompactString::new("abs"),
            BuiltinFunctionDoc {
                description: "Returns the absolute value of the given number.",
                params: &["number"],
            },
        );
        map.insert(
            CompactString::new("md_name"),
            BuiltinFunctionDoc {
                description: "Returns the name of the given markdown node.",
                params: &["markdown"],
            },
        );
        map.insert(
            CompactString::new("md_text"),
            BuiltinFunctionDoc {
                description: "Creates a markdown text node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("md_image"),
            BuiltinFunctionDoc {
                description:
                    "Creates a markdown image node with the given URL, alt text, and title.",
                params: &["url", "alt", "title"],
            },
        );
        map.insert(
            CompactString::new("md_code"),
            BuiltinFunctionDoc {
                description: "Creates a markdown code block with the given value and language.",
                params: &["value", "language"],
            },
        );
        map.insert(
            CompactString::new("md_code_inline"),
            BuiltinFunctionDoc {
                description: "Creates an inline markdown code node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("md_h"),
            BuiltinFunctionDoc {
                description: "Creates a markdown heading node with the given value and depth.",
                params: &["value", "depth"],
            },
        );
        map.insert(
            CompactString::new("md_math"),
            BuiltinFunctionDoc {
                description: "Creates a markdown math block with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("md_math_inline"),
            BuiltinFunctionDoc {
                description: "Creates an inline markdown math node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("md_strong"),
            BuiltinFunctionDoc {
                description: "Creates a markdown strong (bold) node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("md_em"),
            BuiltinFunctionDoc {
                description: "Creates a markdown emphasis (italic) node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("md_hr"),
            BuiltinFunctionDoc {
                description: "Creates amarkdown horizontal rule node.",
                params: &[],
            },
        );
        map.insert(
            CompactString::new("md_list"),
            BuiltinFunctionDoc {
                description: "Creates a markdown list node with the given value and indent level.",
                params: &["value", "indent"],
            },
        );
        map.insert(
            CompactString::new("md_list_level"),
            BuiltinFunctionDoc {
                description: "Returns the indent level of a markdown list node.",
                params: &["list"],
            },
        );
        map.insert(
            CompactString::new("md_check"),
            BuiltinFunctionDoc {
                description: "Creates a markdown list node with the given checked state.",
                params: &["list", "checked"],
            },
        );

        map
    });

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("Invalid base64 string")]
    InvalidBase64String(#[from] base64::DecodeError),
    #[error("\"{0}\" is not defined")]
    NotDefined(FunctionName),
    #[error("Unable to format date time, {0}")]
    InvalidDateTimeFormat(String),
    #[error("Invalid types for \"{0}\", got {1:?}")]
    InvalidTypes(FunctionName, ArgType),
    #[error("Invalid number of arguments in \"{0}\", expected {1}, got {2}")]
    InvalidNumberOfArguments(FunctionName, u8, u8),
    #[error("Invalid regular expression \"{0}\"")]
    InvalidRegularExpression(String),
    #[error("Runtime error: {0}")]
    Runtime(String),
    #[error("Divided by 0")]
    ZeroDivision,
}

impl Error {
    pub fn to_eval_error(
        &self,
        node: ast::Node,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> EvalError {
        match self {
            Error::InvalidBase64String(e) => EvalError::InvalidBase64String(
                (*token_arena.borrow()[node.token_id]).clone(),
                e.to_string(),
            ),
            Error::NotDefined(name) => {
                EvalError::NotDefined((*token_arena.borrow()[node.token_id]).clone(), name.clone())
            }
            Error::InvalidDateTimeFormat(msg) => EvalError::DateTimeFormatError(
                (*token_arena.borrow()[node.token_id]).clone(),
                msg.clone(),
            ),
            Error::InvalidTypes(name, args) => EvalError::InvalidTypes {
                token: (*token_arena.borrow()[node.token_id]).clone(),
                name: name.clone(),
                args: args.iter().map(|o| o.to_string().into()).collect_vec(),
            },
            Error::InvalidNumberOfArguments(name, expected, got) => {
                EvalError::InvalidNumberOfArguments(
                    (*token_arena.borrow()[node.token_id]).clone(),
                    name.clone(),
                    *expected,
                    *got,
                )
            }
            Error::InvalidRegularExpression(regex) => EvalError::InvalidRegularExpression(
                (*token_arena.borrow()[node.token_id]).clone(),
                regex.clone(),
            ),
            Error::Runtime(msg) => {
                EvalError::RuntimeError((*token_arena.borrow()[node.token_id]).clone(), msg.clone())
            }
            Error::ZeroDivision => {
                EvalError::ZeroDivision((*token_arena.borrow()[node.token_id]).clone())
            }
        }
    }
}

#[inline(always)]
pub fn eval_builtin(
    result_value: &RuntimeValue,
    ident: &ast::Ident,
    args: &Vec<RuntimeValue>,
) -> Result<RuntimeValue, Error> {
    BUILTIN_FUNCTIONS.get(&ident.name).map_or_else(
        || Err(Error::NotDefined(ident.to_string())),
        |f| {
            let args = if f.num_params.is_valid(args.len() as u8) {
                args
            } else if f.num_params.is_missing_one_params(args.len() as u8) {
                &vec![result_value.clone()]
                    .into_iter()
                    .chain(args.clone())
                    .collect()
            } else {
                return Err(Error::InvalidNumberOfArguments(
                    ident.to_string(),
                    f.num_params.to_num(),
                    args.len() as u8,
                ));
            };

            (f.func)(ident, args)
        },
    )
}

#[inline(always)]
pub fn eval_selector(node: mq_md::Node, selector: &ast::Selector) -> Vec<RuntimeValue> {
    match selector {
        ast::Selector::Code(lang) if node.is_code(lang.clone()) => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::InlineCode if node.is_inline_code() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::InlineMath if node.is_inline_math() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Strong if node.is_strong() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Emphasis if node.is_emphasis() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Delete if node.is_delete() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Link if node.is_link() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::LinkRef if node.is_link_ref() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Image if node.is_image() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Heading(depth) if node.is_heading(*depth) => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::HorizontalRule if node.is_horizontal_rule() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Blockquote if node.is_blockquote() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Table(row, column) => match (row, column, node.clone()) {
            (
                Some(row1),
                Some(column1),
                mq_md::Node::TableCell(mq_md::TableCell {
                    column: column2,
                    row: row2,
                    last_cell_in_row: _,
                    last_cell_of_in_table: _,
                    ..
                }),
            ) => {
                if *row1 == row2 && *column1 == column2 {
                    vec![RuntimeValue::Markdown(node.clone())]
                } else {
                    Vec::new()
                }
            }
            (Some(row1), None, mq_md::Node::TableCell(mq_md::TableCell { row: row2, .. })) => {
                if *row1 == row2 {
                    vec![RuntimeValue::Markdown(node)]
                } else {
                    Vec::new()
                }
            }
            (
                None,
                Some(column1),
                mq_md::Node::TableCell(mq_md::TableCell {
                    column: column2, ..
                }),
            ) => {
                if *column1 == column2 {
                    vec![RuntimeValue::Markdown(node)]
                } else {
                    Vec::new()
                }
            }
            (None, None, mq_md::Node::TableCell(_)) => {
                vec![RuntimeValue::Markdown(node)]
            }
            _ => Vec::new(),
        },
        ast::Selector::Html if node.is_html() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Footnote if node.is_footnote() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::MdxJsxFlowElement if node.is_mdx_jsx_flow_element() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::List(index, checked) => match (index, node.clone()) {
            (
                Some(index),
                mq_md::Node::List(mq_md::List {
                    index: list_index,
                    checked: list_checked,
                    ..
                }),
            ) => {
                if *index == list_index && *checked == list_checked {
                    vec![RuntimeValue::Markdown(node)]
                } else {
                    Vec::new()
                }
            }
            (_, mq_md::Node::List(mq_md::List { .. })) => {
                vec![RuntimeValue::Markdown(node)]
            }
            _ => Vec::new(),
        },
        ast::Selector::MdxJsEsm if node.is_msx_js_esm() => {
            vec![RuntimeValue::Markdown(node.clone())]
        }
        ast::Selector::Toml if node.is_toml() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Yaml if node.is_yaml() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Break if node.is_break() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::MdxTextExpression if node.is_mdx_text_expression() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::FootnoteRef if node.is_footnote_ref() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::ImageRef if node.is_image_ref() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::MdxJsxTextElement if node.is_mdx_jsx_text_element() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Math if node.is_math() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::MdxFlowExpression if node.is_mdx_flow_expression() => {
            vec![RuntimeValue::Markdown(node)]
        }
        ast::Selector::Definition if node.is_definition() => {
            vec![RuntimeValue::Markdown(node)]
        }
        _ => Vec::new(),
    }
}

#[inline(always)]
fn from_date(date_str: &str) -> Result<RuntimeValue, Error> {
    match chrono::DateTime::parse_from_rfc3339(date_str) {
        Ok(datetime) => Ok(RuntimeValue::Number(datetime.timestamp_millis().into())),
        Err(e) => Err(Error::Runtime(format!("{}", e))),
    }
}

#[inline(always)]
fn to_date(ms: Number, format: Option<&str>) -> Result<RuntimeValue, Error> {
    chrono::DateTime::from_timestamp((ms.value() as i64) / 1000, 0)
        .map(|dt| {
            format
                .map(|f| dt.format(f).to_string())
                .unwrap_or(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
        .map(RuntimeValue::String)
        .ok_or_else(|| Error::InvalidDateTimeFormat(format.unwrap_or("").to_string()))
}

#[inline(always)]
fn base64(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(BASE64_STANDARD.encode(input)))
}

#[inline(always)]
fn base64d(input: &str) -> Result<RuntimeValue, Error> {
    BASE64_STANDARD
        .decode(input)
        .map_err(Error::InvalidBase64String)
        .map(|v| RuntimeValue::String(String::from_utf8_lossy(&v).to_string()))
}

#[inline(always)]
fn url_encode(input: &str) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(
        utf8_percent_encode(input, NON_ALPHANUMERIC).to_string(),
    ))
}

#[inline(always)]
fn match_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    let mut cache = REGEX_CACHE.lock().unwrap();
    if let Some(re) = cache.get(pattern) {
        let matches: Vec<RuntimeValue> = re
            .find_iter(input)
            .map(|m| RuntimeValue::String(m.as_str().to_string()))
            .collect();
        Ok(RuntimeValue::Array(matches))
    } else if let Ok(re) = Regex::new(pattern) {
        cache.insert(pattern.to_string(), re.clone());
        let matches: Vec<RuntimeValue> = re
            .find_iter(input)
            .map(|m| RuntimeValue::String(m.as_str().to_string()))
            .collect();
        Ok(RuntimeValue::Array(matches))
    } else {
        Err(Error::InvalidRegularExpression(pattern.to_string()))
    }
}

#[inline(always)]
fn replace_re(input: &str, pattern: &str, replacement: &str) -> Result<RuntimeValue, Error> {
    let mut cache = REGEX_CACHE.lock().unwrap();
    if let Some(re) = cache.get(pattern) {
        Ok(re.replace_all(input, replacement).to_string().into())
    } else if let Ok(re) = Regex::new(pattern) {
        cache.insert(pattern.to_string(), re.clone());
        Ok(re.replace_all(input, replacement).to_string().into())
    } else {
        Err(Error::InvalidRegularExpression(pattern.to_string()))
    }
}

#[inline(always)]
fn split_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    let mut cache = REGEX_CACHE.lock().unwrap();
    if let Some(re) = cache.get(pattern) {
        Ok(RuntimeValue::Array(
            re.split(input).map(|s| s.to_owned().into()).collect_vec(),
        ))
    } else if let Ok(re) = Regex::new(pattern) {
        cache.insert(pattern.to_string(), re.clone());
        Ok(RuntimeValue::Array(
            re.split(input).map(|s| s.to_owned().into()).collect_vec(),
        ))
    } else {
        Err(Error::InvalidRegularExpression(pattern.to_string()))
    }
}
