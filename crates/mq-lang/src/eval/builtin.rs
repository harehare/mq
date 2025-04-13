use crate::Token;
use crate::arena::Arena;
use crate::ast::node as ast;
use crate::number::Number;
use base64::prelude::*;
use compact_str::CompactString;
use itertools::Itertools;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use regex::{Regex, RegexBuilder};
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, smallvec};
use std::cell::RefCell;
use std::process::exit;
use std::rc::Rc;
use std::{
    sync::{LazyLock, Mutex},
    vec,
};
use thiserror::Error;

use super::error::EvalError;
use super::runtime_value::{self, RuntimeValue};
use mq_markdown;

static REGEX_CACHE: LazyLock<Mutex<FxHashMap<String, Regex>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

type FunctionName = String;
type ErrorArgs = Vec<RuntimeValue>;
pub type Args = SmallVec<[RuntimeValue; 4]>;

#[derive(Clone, Debug)]
pub struct BuiltinFunction {
    pub num_params: ParamNum,
    pub func: fn(&ast::Ident, &RuntimeValue, &Args) -> Result<RuntimeValue, Error>,
}

#[derive(Clone, Debug)]
pub enum ParamNum {
    None,
    Fixed(u8),
    Range(u8, u8),
}

impl ParamNum {
    #[inline(always)]
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

    #[inline(always)]
    pub fn is_missing_one_params(&self, num_args: u8) -> bool {
        match self {
            ParamNum::Fixed(n) => num_args == n.checked_sub(1).unwrap_or_default(),
            ParamNum::Range(n, _) => num_args == n.checked_sub(1).unwrap_or_default(),
            _ => false,
        }
    }
}

impl BuiltinFunction {
    pub fn new(
        num_params: ParamNum,
        func: fn(&ast::Ident, &RuntimeValue, &Args) -> Result<RuntimeValue, Error>,
    ) -> Self {
        BuiltinFunction { num_params, func }
    }
}

pub static BUILTIN_FUNCTIONS: LazyLock<FxHashMap<CompactString, BuiltinFunction>> =
    LazyLock::new(|| {
        let mut map = FxHashMap::default();

        map.insert(
            CompactString::new("halt"),
            BuiltinFunction::new(ParamNum::None, |ident, _, args| match args.as_slice() {
                [RuntimeValue::Number(exit_code)] => exit(exit_code.value() as i32),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("error"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(message)] => Err(Error::UserDefined(message.clone())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("debug"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, current_value, args| {
                match args.as_slice() {
                    [a] => {
                        eprintln!("DEBUG: {}", a);
                        Ok(current_value.clone())
                    }
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("type"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| {
                Ok(args.first().unwrap().name().to_string().into())
            }),
        );
        map.insert(
            CompactString::new("array"),
            BuiltinFunction::new(ParamNum::Range(0, u8::MAX), |_, _, args| {
                Ok(RuntimeValue::Array(args.to_vec()))
            }),
        );
        map.insert(
            CompactString::new("from_date"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(date_str)] => from_date(date_str),
                [RuntimeValue::Markdown(node_value, _)] => from_date(node_value.value().as_str()),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_date"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::None, |_, _, _| {
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => base64(s),
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| {
                        base64(md.value().as_str()).and_then(|b| match b {
                            RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                            a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                        })
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("base64d"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => base64d(s),
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| {
                        base64d(md.value().as_str()).and_then(|o| match o {
                            RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                            a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                        })
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("min"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(mq_markdown::to_html(s).into()),
                [RuntimeValue::Markdown(node_value, _)] => {
                    Ok(mq_markdown::to_html(node_value.to_string().as_str()).into())
                }
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_csv"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => Ok(array.iter().join(",").into()),
                [a] => Ok(a.to_string().into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_tsv"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => Ok(array.iter().join("\t").into()),
                [a] => Ok(a.to_string().into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_string"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(node_value, _)] => Ok(node_value.to_string().into()),
                [RuntimeValue::Array(array)] => {
                    let result_value: Result<Vec<RuntimeValue>, Error> = array
                        .clone()
                        .into_iter()
                        .map(|o| match o {
                            RuntimeValue::Markdown(node_value, _) => {
                                Ok(node_value.to_string().into())
                            }
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| {
                        md.to_string()
                            .parse::<f64>()
                            .map(|n| RuntimeValue::Number(n.into()))
                            .map_err(|e| Error::Runtime(format!("{}", e)))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [RuntimeValue::String(s)] => s
                    .parse::<f64>()
                    .map(|n| RuntimeValue::Number(n.into()))
                    .map_err(|e| Error::Runtime(format!("{}", e))),
                [RuntimeValue::Array(array)] => {
                    let result_value: Result<Vec<RuntimeValue>, Error> = array
                        .clone()
                        .into_iter()
                        .map(|o| match o {
                            node @ RuntimeValue::Markdown(_, _) => node
                                .markdown_node()
                                .map(|md| {
                                    md.to_string()
                                        .parse::<f64>()
                                        .map(|n| RuntimeValue::Number(n.into()))
                                        .map_err(|e| Error::Runtime(format!("{}", e)))
                                })
                                .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                            RuntimeValue::String(s) => s
                                .parse::<f64>()
                                .map(|n| RuntimeValue::Number(n.into()))
                                .map_err(|e| Error::Runtime(format!("{}", e))),
                            RuntimeValue::Bool(b) => {
                                Ok(RuntimeValue::Number(if b { 1 } else { 0 }.into()))
                            }
                            n @ RuntimeValue::Number(_) => Ok(n),
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => url_encode(s),
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| {
                        url_encode(md.value().as_str()).and_then(|o| match o {
                            RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                            a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                        })
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_text"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::None] => Ok("".to_owned().into()),
                [RuntimeValue::Markdown(node_value, _)] => Ok(node_value.value().into()),
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                    .markdown_node()
                    .map(|md| Ok(md.value().ends_with(s).into()))
                    .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.ends_with(s2).into()),
                [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
                    .last()
                    .map_or(Ok(RuntimeValue::FALSE), |o| {
                        eval_builtin(o, ident, &smallvec![RuntimeValue::String(s.clone())])
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                    .markdown_node()
                    .map(|md| Ok(md.value().starts_with(s).into()))
                    .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                    Ok(s1.starts_with(s2).into())
                }
                [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
                    .first()
                    .map_or(Ok(RuntimeValue::FALSE), |o| {
                        eval_builtin(o, ident, &smallvec![RuntimeValue::String(s.clone())])
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s), RuntimeValue::String(pattern)] => match_re(s, pattern),
                [
                    node @ RuntimeValue::Markdown(_, _),
                    RuntimeValue::String(pattern),
                ] => node
                    .markdown_node()
                    .map(|md| match_re(&md.value(), pattern))
                    .unwrap_or_else(|| Ok(RuntimeValue::EMPTY_ARRAY)),
                [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::EMPTY_ARRAY),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("downcase"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| Ok(node.update_markdown_value(md.value().to_lowercase().as_str())))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [RuntimeValue::String(s)] => Ok(s.to_lowercase().into()),
                [_] => Ok(RuntimeValue::NONE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("gsub"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, args| match args.as_slice() {
                [
                    RuntimeValue::String(s1),
                    RuntimeValue::String(s2),
                    RuntimeValue::String(s3),
                ] => Ok(replace_re(s1, s2, s3)?),
                [
                    node @ RuntimeValue::Markdown(_, _),
                    RuntimeValue::String(s1),
                    RuntimeValue::String(s2),
                ] => node
                    .markdown_node()
                    .map(|md| {
                        Ok(node.update_markdown_value(
                            &replace_re(md.value().as_str(), s1.as_str(), s2.as_str())?.to_string(),
                        ))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [
                    RuntimeValue::None,
                    RuntimeValue::String(_),
                    RuntimeValue::String(_),
                ] => Ok(RuntimeValue::NONE),
                [a, b, c] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone(), c.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("replace"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, args| match args.as_slice() {
                [
                    RuntimeValue::String(s1),
                    RuntimeValue::String(s2),
                    RuntimeValue::String(s3),
                ] => Ok(s1.replace(s2, s3).into()),
                [
                    node @ RuntimeValue::Markdown(_, _),
                    RuntimeValue::String(s1),
                    RuntimeValue::String(s2),
                ] => node
                    .markdown_node()
                    .map(|md| {
                        Ok(node.update_markdown_value(
                            md.value().replace(s1.as_str(), s2.as_str()).as_str(),
                        ))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [
                    RuntimeValue::None,
                    RuntimeValue::String(_),
                    RuntimeValue::String(_),
                ] => Ok(RuntimeValue::NONE),
                [a, b, c] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone(), c.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("repeat"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
                    Ok(s.repeat(n.value() as usize).into())
                }
                [node @ RuntimeValue::Markdown(_, _), RuntimeValue::Number(n)] => node
                    .markdown_node()
                    .map(|md| {
                        Ok(node
                            .update_markdown_value(md.value().repeat(n.value() as usize).as_str()))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [RuntimeValue::Array(array), RuntimeValue::Number(n)] => {
                    let n = n.value() as usize;
                    if n == 0 {
                        return Ok(RuntimeValue::EMPTY_ARRAY);
                    }

                    let mut repeated_array = Vec::with_capacity(array.len() * n);
                    for _ in 0..n {
                        repeated_array.extend_from_slice(array);
                    }
                    Ok(RuntimeValue::Array(repeated_array))
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(
                    s.chars()
                        .map(|c| RuntimeValue::Number((c as u32).into()))
                        .collect::<Vec<_>>(),
                )),
                [node @ RuntimeValue::Markdown(_, _)] => Ok(RuntimeValue::Array(
                    node.markdown_node()
                        .map(|md| {
                            md.value()
                                .chars()
                                .map(|c| RuntimeValue::Number((c as u32).into()))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                )),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("implode"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(s.trim().to_string().into()),
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| Ok(node.update_markdown_value(md.to_string().trim())))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [RuntimeValue::None] => Ok(RuntimeValue::None),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("upcase"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| Ok(node.update_markdown_value(md.value().to_uppercase().as_str())))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [RuntimeValue::String(s)] => Ok(s.to_uppercase().into()),
                [RuntimeValue::None] => Ok(RuntimeValue::None),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("update"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [
                    node1 @ RuntimeValue::Markdown(_, _),
                    node2 @ RuntimeValue::Markdown(_, _),
                ] => node2
                    .markdown_node()
                    .map(|md| Ok(node1.update_markdown_value(&md.value())))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [
                    RuntimeValue::Markdown(node_value, _),
                    RuntimeValue::String(s),
                ] => Ok(node_value.with_value(s).into()),
                [RuntimeValue::None, _] => Ok(RuntimeValue::NONE),
                [_, a] => Ok(a.clone()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("slice"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, args| match args.as_slice() {
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
                    node @ RuntimeValue::Markdown(_, _),
                    RuntimeValue::Number(start),
                    RuntimeValue::Number(end),
                ] => node
                    .markdown_node()
                    .map(|md| {
                        let start = start.value() as usize;
                        let end = end.value() as usize;
                        let sub: String = md
                            .value()
                            .chars()
                            .enumerate()
                            .filter(|&(i, _)| i >= start && i < end)
                            .fold("".to_string(), |s, (_, c)| format!("{}{}", s, c));

                        Ok(node.update_markdown_value(&sub))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(
                    (s1.find(s2).map(|v| v as isize).unwrap_or_else(|| -1) as i64).into(),
                )),
                [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                    .markdown_node()
                    .map(|md| {
                        Ok(RuntimeValue::Number(
                            (md.value().find(s).map(|v| v as isize).unwrap_or_else(|| -1) as i64)
                                .into(),
                        ))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::Number((-1_i64).into()))),
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.chars().count().into())),
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| Ok(RuntimeValue::Number(md.value().chars().count().into())))
                    .unwrap_or_else(|| Ok(RuntimeValue::Number(0.into()))),
                [RuntimeValue::Array(array)] => Ok(RuntimeValue::Number(array.len().into())),
                [RuntimeValue::None] => Ok(RuntimeValue::Number(0.into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("utf8bytelen"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.len().into())),
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| Ok(RuntimeValue::Number(md.value().len().into())))
                    .unwrap_or_else(|| Ok(RuntimeValue::Number(0.into()))),
                [RuntimeValue::Array(array)] => Ok(RuntimeValue::Number(array.len().into())),
                [RuntimeValue::None] => Ok(RuntimeValue::Number(0.into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );

        map.insert(
            CompactString::new("rindex"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(
                    s1.rfind(s2)
                        .map(|v| v as isize)
                        .unwrap_or_else(|| -1)
                        .into(),
                )),
                [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                    .markdown_node()
                    .map(|md| {
                        Ok(RuntimeValue::Number(
                            md.value()
                                .rfind(s)
                                .map(|v| v as isize)
                                .unwrap_or_else(|| -1)
                                .into(),
                        ))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::Number((-1_i64).into()))),
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
                [RuntimeValue::Markdown(node, _), RuntimeValue::Number(i)] => {
                    Ok(RuntimeValue::Markdown(
                        node.clone(),
                        Some(runtime_value::Selector::Index(i.value() as usize)),
                    ))
                }
                [RuntimeValue::None, RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("del"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Array(array), RuntimeValue::Number(n)] => {
                    let mut array = array.clone();
                    array.remove(n.value() as usize);
                    Ok(RuntimeValue::Array(array))
                }
                [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
                    let mut s = s.clone().chars().collect::<Vec<_>>();
                    s.remove(n.value() as usize);
                    Ok(s.into_iter().collect::<String>().into())
                }
                [RuntimeValue::None, RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("join"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Array(array), RuntimeValue::String(s)] => {
                    Ok(array.iter().join(s).into())
                }
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("reverse"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(
                    array
                        .iter()
                        .filter(|v| !v.is_none())
                        .cloned()
                        .collect::<Vec<_>>(),
                )),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("range"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(split_re(s1, s2)?),
                [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                    .markdown_node()
                    .map(|md| split_re(md.value().as_str(), s))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::EMPTY_ARRAY),
                [a, b] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("uniq"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Array(array)] => {
                    let mut vec = array.to_vec();
                    let mut unique = FxHashMap::default();
                    vec.retain(|item| unique.insert(item.to_string(), item.clone()).is_none());
                    Ok(RuntimeValue::Array(vec))
                }
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("ceil"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ceil().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("floor"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().floor().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("round"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().round().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("trunc"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().trunc().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("abs"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, args| match args.as_slice() {
                [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().abs().into())),
                [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("eq"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [a, b] => Ok((a == b).into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("ne"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [a, b] => Ok((a != b).into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("gt"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 > s2).into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 > n2).into()),
                [RuntimeValue::Bool(b1), RuntimeValue::Bool(b2)] => Ok((b1 > b2).into()),
                [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => {
                    Ok((n1 > n2).into())
                }
                [_, _] => Ok(RuntimeValue::FALSE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("gte"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 >= s2).into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 >= n2).into()),
                [RuntimeValue::Bool(b1), RuntimeValue::Bool(b2)] => Ok((b1 >= b2).into()),
                [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => {
                    Ok((n1 >= n2).into())
                }
                [_, _] => Ok(RuntimeValue::FALSE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("lt"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 < s2).into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 < n2).into()),
                [RuntimeValue::Bool(b1), RuntimeValue::Bool(b2)] => Ok((b1 < b2).into()),
                [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => {
                    Ok((n1 < n2).into())
                }
                [_, _] => Ok(RuntimeValue::FALSE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("lte"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 <= s2).into()),
                [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 <= n2).into()),
                [RuntimeValue::Bool(b1), RuntimeValue::Bool(b2)] => Ok((b1 <= b2).into()),
                [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => {
                    Ok((n1 <= n2).into())
                }
                [_, _] => Ok(RuntimeValue::FALSE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("add"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
                [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                    Ok(format!("{}{}", s1, s2).into())
                }
                [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                    .markdown_node()
                    .map(|md| {
                        Ok(node.update_markdown_value(format!("{}{}", md.value(), s).as_str()))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [RuntimeValue::String(s), node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| {
                        Ok(node.update_markdown_value(format!("{}{}", s, md.value()).as_str()))
                    })
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [
                    node1 @ RuntimeValue::Markdown(_, _),
                    node2 @ RuntimeValue::Markdown(_, _),
                ] => Ok(node2
                    .markdown_node()
                    .and_then(|md2| {
                        node1.markdown_node().map(|md1| {
                            node1.update_markdown_value(
                                format!("{}{}", md1.value(), md2.value()).as_str(),
                            )
                        })
                    })
                    .unwrap_or(RuntimeValue::NONE)),
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, args| match args.as_slice() {
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
            BuiltinFunction::new(ParamNum::Range(2, u8::MAX), |_, _, args| {
                Ok(args.iter().all(|arg| arg.is_true()).into())
            }),
        );
        map.insert(
            CompactString::new("or"),
            BuiltinFunction::new(ParamNum::Range(2, u8::MAX), |_, _, args| {
                Ok(args.iter().any(|arg| arg.is_true()).into())
            }),
        );
        map.insert(
            CompactString::new("not"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [a] => Ok((!a.is_true()).into()),
                _ => unreachable!(),
            }),
        );

        // markdown
        map.insert(
            CompactString::new("to_code"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [a, RuntimeValue::String(lang)] => Ok(mq_markdown::Node::Code(mq_markdown::Code {
                    value: a.to_string(),
                    lang: Some(lang.to_string()),
                    position: None,
                    meta: None,
                    fence: true,
                })
                .into()),
                [a, RuntimeValue::None] if !a.is_none() => {
                    Ok(mq_markdown::Node::Code(mq_markdown::Code {
                        value: a.to_string(),
                        lang: None,
                        position: None,
                        meta: None,
                        fence: true,
                    })
                    .into())
                }
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_code_inline"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [a] if !a.is_none() => Ok(mq_markdown::Node::CodeInline(mq_markdown::CodeInline {
                    value: a.to_string().into(),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_h"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(node, _), RuntimeValue::Number(depth)] => {
                    Ok(mq_markdown::Node::Heading(mq_markdown::Heading {
                        depth: (*depth).value() as u8,
                        values: node.node_values(),
                        position: None,
                    })
                    .into())
                }
                [a, RuntimeValue::Number(depth)] => {
                    Ok(mq_markdown::Node::Heading(mq_markdown::Heading {
                        depth: (*depth).value() as u8,
                        values: vec![a.to_string().into()],
                        position: None,
                    })
                    .into())
                }
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_hr"),
            BuiltinFunction::new(ParamNum::Fixed(0), |_, _, _| {
                Ok(
                    mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule {
                        position: None,
                    })
                    .into(),
                )
            }),
        );
        map.insert(
            CompactString::new("to_link"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, args| match args.as_slice() {
                [
                    RuntimeValue::String(url),
                    RuntimeValue::String(value),
                    RuntimeValue::String(title),
                ] => Ok(mq_markdown::Node::Link(mq_markdown::Link {
                    url: mq_markdown::Url::new(url.to_string()),
                    values: vec![value.to_string().into()],
                    title: if title.is_empty() {
                        None
                    } else {
                        Some(mq_markdown::Title::new(title.into()))
                    },
                    position: None,
                })
                .into()),
                [RuntimeValue::None, _, _] => Ok(RuntimeValue::NONE),
                [a, b, c] => Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![a.clone(), b.clone(), c.clone()],
                )),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_image"),
            BuiltinFunction::new(ParamNum::Fixed(3), |_, _, args| match args.as_slice() {
                [
                    RuntimeValue::String(url),
                    RuntimeValue::String(alt),
                    RuntimeValue::String(title),
                ] => Ok(mq_markdown::Node::Image(mq_markdown::Image {
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
            CompactString::new("to_math"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [a] => Ok(mq_markdown::Node::Math(mq_markdown::Math {
                    value: a.to_string(),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_math_inline"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [a] => Ok(mq_markdown::Node::MathInline(mq_markdown::MathInline {
                    value: a.to_string().into(),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_md_name"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(node, _)] => Ok(node.name().to_string().into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_strong"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(node, _)] => {
                    Ok(mq_markdown::Node::Strong(mq_markdown::Strong {
                        values: node.node_values(),
                        position: None,
                    })
                    .into())
                }
                [a] if !a.is_none() => Ok(mq_markdown::Node::Strong(mq_markdown::Strong {
                    values: vec![a.to_string().into()],
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_em"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(node, _)] => {
                    Ok(mq_markdown::Node::Emphasis(mq_markdown::Emphasis {
                        values: node.node_values(),
                        position: None,
                    })
                    .into())
                }
                [a] if !a.is_none() => Ok(mq_markdown::Node::Emphasis(mq_markdown::Emphasis {
                    values: vec![a.to_string().into()],
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_md_text"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [a] if !a.is_none() => Ok(mq_markdown::Node::Text(mq_markdown::Text {
                    value: a.to_string(),
                    position: None,
                })
                .into()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_md_list"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(node, _), RuntimeValue::Number(level)] => {
                    Ok(mq_markdown::Node::List(mq_markdown::List {
                        values: node.node_values(),
                        index: 0,
                        level: level.value() as u8,
                        checked: None,
                        position: None,
                    })
                    .into())
                }
                [a, RuntimeValue::Number(level)] if !a.is_none() => {
                    Ok(mq_markdown::Node::List(mq_markdown::List {
                        values: vec![a.to_string().into()],
                        index: 0,
                        level: level.value() as u8,
                        checked: None,
                        position: None,
                    })
                    .into())
                }
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("to_md_table_row"),
            BuiltinFunction::new(ParamNum::Range(1, u8::MAX), |_, _, args| {
                let args_num = args.len();
                let mut current_index = 0;
                let values = args
                    .iter()
                    .enumerate()
                    .flat_map(|(i, arg)| match arg {
                        RuntimeValue::Array(array) => {
                            let array_num = array.len();
                            array
                                .iter()
                                .enumerate()
                                .map(move |(j, v)| {
                                    current_index += 1;
                                    mq_markdown::Node::TableCell(mq_markdown::TableCell {
                                        row: 0,
                                        column: current_index - 1,
                                        values: vec![v.to_string().into()],
                                        last_cell_in_row: i == args_num - 1 && j == array_num - 1,
                                        last_cell_of_in_table: false,
                                        position: None,
                                    })
                                })
                                .collect::<Vec<_>>()
                        }
                        v => {
                            current_index += 1;
                            vec![mq_markdown::Node::TableCell(mq_markdown::TableCell {
                                row: 0,
                                column: current_index - 1,
                                values: vec![v.to_string().into()],
                                last_cell_in_row: i == args_num - 1,
                                last_cell_of_in_table: false,
                                position: None,
                            })]
                        }
                    })
                    .collect::<Vec<_>>();

                Ok(RuntimeValue::Markdown(
                    mq_markdown::Node::TableRow(mq_markdown::TableRow {
                        values,
                        position: None,
                    }),
                    None,
                ))
            }),
        );
        map.insert(
            CompactString::new("get_md_list_level"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [
                    RuntimeValue::Markdown(
                        mq_markdown::Node::List(mq_markdown::List { level, .. }),
                        _,
                    ),
                ] => Ok(RuntimeValue::Number((*level).into())),
                [_] => Ok(RuntimeValue::Number(0.into())),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("get_title"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [
                    RuntimeValue::Markdown(
                        mq_markdown::Node::Definition(mq_markdown::Definition { title, .. }),
                        _,
                    )
                    | RuntimeValue::Markdown(
                        mq_markdown::Node::Link(mq_markdown::Link { title, .. }),
                        _,
                    ),
                ] => title
                    .as_ref()
                    .map(|t| Ok(RuntimeValue::String(t.to_value())))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [
                    RuntimeValue::Markdown(
                        mq_markdown::Node::Image(mq_markdown::Image { title, .. }),
                        _,
                    ),
                ] => title
                    .as_ref()
                    .map(|t| Ok(RuntimeValue::String(t.clone())))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                [_] => Ok(RuntimeValue::NONE),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("get_url"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(mq_markdown::Node::Definition(def), _)] => {
                    Ok(def.url.as_str().into())
                }
                [RuntimeValue::Markdown(mq_markdown::Node::Link(link), _)] => {
                    Ok(link.url.as_str().into())
                }
                [RuntimeValue::Markdown(mq_markdown::Node::Image(image), _)] => {
                    Ok(image.url.to_owned().into())
                }
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("set_check"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [
                    RuntimeValue::Markdown(mq_markdown::Node::List(list), _),
                    RuntimeValue::Bool(checked),
                ] => Ok(mq_markdown::Node::List(mq_markdown::List {
                    checked: Some(*checked),
                    ..list.clone()
                })
                .into()),
                [a, ..] => Ok(a.clone()),
                _ => Ok(RuntimeValue::None),
            }),
        );
        map.insert(
            CompactString::new("set_ref"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [
                    RuntimeValue::Markdown(mq_markdown::Node::Definition(def), _),
                    RuntimeValue::String(s),
                ] => Ok(mq_markdown::Node::Definition(mq_markdown::Definition {
                    ident: s.to_owned(),
                    ..def.clone()
                })
                .into()),
                [
                    RuntimeValue::Markdown(mq_markdown::Node::ImageRef(image_ref), _),
                    RuntimeValue::String(s),
                ] => Ok(mq_markdown::Node::ImageRef(mq_markdown::ImageRef {
                    ident: s.to_owned(),
                    ..image_ref.clone()
                })
                .into()),
                [
                    RuntimeValue::Markdown(mq_markdown::Node::LinkRef(link_ref), _),
                    RuntimeValue::String(s),
                ] => Ok(mq_markdown::Node::LinkRef(mq_markdown::LinkRef {
                    ident: s.to_owned(),
                    ..link_ref.clone()
                })
                .into()),
                [
                    RuntimeValue::Markdown(mq_markdown::Node::Footnote(footnote), _),
                    RuntimeValue::String(s),
                ] => Ok(mq_markdown::Node::Footnote(mq_markdown::Footnote {
                    ident: s.to_owned(),
                    ..footnote.clone()
                })
                .into()),
                [
                    RuntimeValue::Markdown(mq_markdown::Node::FootnoteRef(footnote_ref), _),
                    RuntimeValue::String(s),
                ] => Ok(mq_markdown::Node::FootnoteRef(mq_markdown::FootnoteRef {
                    ident: s.to_owned(),
                    ..footnote_ref.clone()
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
            CompactString::new(".text"),
            BuiltinSelectorDoc {
                description: "Selects a text node.",
                params: &[],
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
            CompactString::new("error"),
            BuiltinFunctionDoc {
                description: "Raises a user-defined error with the specified message.",
                params: &["message"],
            },
        );
        map.insert(
            CompactString::new("assert"),
            BuiltinFunctionDoc {
            description: "Asserts that two values are equal, returns the value if true, otherwise raises an error.",
            params: &["value1", "value2"],
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
            CompactString::new("update"),
            BuiltinFunctionDoc {
                description: "Update the value with specified value.",
                params: &["target_value", "source_value"],
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
            CompactString::new("to_md_name"),
            BuiltinFunctionDoc {
                description: "Returns the name of the given markdown node.",
                params: &["markdown"],
            },
        );
        map.insert(
            CompactString::new("to_md_text"),
            BuiltinFunctionDoc {
                description: "Creates a markdown text node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_image"),
            BuiltinFunctionDoc {
                description:
                    "Creates a markdown image node with the given URL, alt text, and title.",
                params: &["url", "alt", "title"],
            },
        );
        map.insert(
            CompactString::new("to_code"),
            BuiltinFunctionDoc {
                description: "Creates a markdown code block with the given value and language.",
                params: &["value", "language"],
            },
        );
        map.insert(
            CompactString::new("to_code_inline"),
            BuiltinFunctionDoc {
                description: "Creates an inline markdown code node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_h"),
            BuiltinFunctionDoc {
                description: "Creates a markdown heading node with the given value and depth.",
                params: &["value", "depth"],
            },
        );
        map.insert(
            CompactString::new("to_math"),
            BuiltinFunctionDoc {
                description: "Creates a markdown math block with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_math_inline"),
            BuiltinFunctionDoc {
                description: "Creates an inline markdown math node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_strong"),
            BuiltinFunctionDoc {
                description: "Creates a markdown strong (bold) node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_em"),
            BuiltinFunctionDoc {
                description: "Creates a markdown emphasis (italic) node with the given value.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_hr"),
            BuiltinFunctionDoc {
                description: "Creates a markdown horizontal rule node.",
                params: &[],
            },
        );
        map.insert(
            CompactString::new("to_link"),
            BuiltinFunctionDoc {
                description: "Creates a markdown link node  with the given  url and title.",
                params: &["url", "value", "title"],
            },
        );
        map.insert(
            CompactString::new("to_md_list"),
            BuiltinFunctionDoc {
                description: "Creates a markdown list node with the given value and indent level.",
                params: &["value", "indent"],
            },
        );
        map.insert(
            CompactString::new("to_md_table_row"),
            BuiltinFunctionDoc {
                description: "Creates a markdown table row node with the given values.",
                params: &["cells"],
            },
        );
        map.insert(
            CompactString::new("get_md_list_level"),
            BuiltinFunctionDoc {
                description: "Returns the indent level of a markdown list node.",
                params: &["list"],
            },
        );
        map.insert(
            CompactString::new("get_title"),
            BuiltinFunctionDoc {
                description: "Returns the title of a markdown node.",
                params: &["node"],
            },
        );
        map.insert(
            CompactString::new("get_url"),
            BuiltinFunctionDoc {
                description: "Returns the url of a markdown node.",
                params: &["node"],
            },
        );
        map.insert(
            CompactString::new("set_check"),
            BuiltinFunctionDoc {
                description: "Creates a markdown list node with the given checked state.",
                params: &["list", "checked"],
            },
        );
        map.insert(
            CompactString::new("set_ref"),
            BuiltinFunctionDoc {
            description: "Sets the reference identifier for markdown nodes that support references (e.g., Definition, LinkRef, ImageRef, Footnote, FootnoteRef).",
            params: &["node", "reference_id"],
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
    InvalidTypes(FunctionName, ErrorArgs),
    #[error("Invalid number of arguments in \"{0}\", expected {1}, got {2}")]
    InvalidNumberOfArguments(FunctionName, u8, u8),
    #[error("Invalid regular expression \"{0}\"")]
    InvalidRegularExpression(String),
    #[error("Runtime error: {0}")]
    Runtime(String),
    #[error("Divided by 0")]
    ZeroDivision,
    #[error("{0}")]
    UserDefined(String),
}

impl Error {
    pub fn to_eval_error(
        &self,
        node: ast::Node,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> EvalError {
        match self {
            Error::UserDefined(message) => EvalError::UserDefined {
                message: message.to_owned(),
                token: (*token_arena.borrow()[node.token_id]).clone(),
            },
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
                args: args
                    .iter()
                    .map(|o| o.to_string().into())
                    .collect::<Vec<_>>(),
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
    runtime_value: &RuntimeValue,
    ident: &ast::Ident,
    args: &Args,
) -> Result<RuntimeValue, Error> {
    BUILTIN_FUNCTIONS.get(&ident.name).map_or_else(
        || Err(Error::NotDefined(ident.to_string())),
        |f| {
            let args = if f.num_params.is_valid(args.len() as u8) {
                args
            } else if f.num_params.is_missing_one_params(args.len() as u8) {
                &vec![runtime_value.clone()]
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

            (f.func)(ident, runtime_value, args)
        },
    )
}

pub fn eval_selector(node: &mq_markdown::Node, selector: &ast::Selector) -> bool {
    match selector {
        ast::Selector::Code(lang) if node.is_code(lang.clone()) => true,
        ast::Selector::InlineCode if node.is_inline_code() => true,
        ast::Selector::InlineMath if node.is_inline_math() => true,
        ast::Selector::Strong if node.is_strong() => true,
        ast::Selector::Emphasis if node.is_emphasis() => true,
        ast::Selector::Delete if node.is_delete() => true,
        ast::Selector::Link if node.is_link() => true,
        ast::Selector::LinkRef if node.is_link_ref() => true,
        ast::Selector::Image if node.is_image() => true,
        ast::Selector::Heading(depth) if node.is_heading(*depth) => true,
        ast::Selector::HorizontalRule if node.is_horizontal_rule() => true,
        ast::Selector::Blockquote if node.is_blockquote() => true,
        ast::Selector::Table(row, column) => match (row, column, node.clone()) {
            (
                Some(row1),
                Some(column1),
                mq_markdown::Node::TableCell(mq_markdown::TableCell {
                    column: column2,
                    row: row2,
                    last_cell_in_row: _,
                    last_cell_of_in_table: _,
                    ..
                }),
            ) => *row1 == row2 && *column1 == column2,
            (
                Some(row1),
                None,
                mq_markdown::Node::TableCell(mq_markdown::TableCell { row: row2, .. }),
            ) => *row1 == row2,
            (
                None,
                Some(column1),
                mq_markdown::Node::TableCell(mq_markdown::TableCell {
                    column: column2, ..
                }),
            ) => *column1 == column2,
            (None, None, mq_markdown::Node::TableCell(_)) => true,
            _ => false,
        },
        ast::Selector::Html if node.is_html() => true,
        ast::Selector::Footnote if node.is_footnote() => true,
        ast::Selector::MdxJsxFlowElement if node.is_mdx_jsx_flow_element() => true,
        ast::Selector::List(index, checked) => match (index, node.clone()) {
            (
                Some(index),
                mq_markdown::Node::List(mq_markdown::List {
                    index: list_index,
                    checked: list_checked,
                    ..
                }),
            ) => *index == list_index && *checked == list_checked,
            (_, mq_markdown::Node::List(mq_markdown::List { .. })) => true,
            _ => false,
        },
        ast::Selector::MdxJsEsm if node.is_msx_js_esm() => true,
        ast::Selector::Text if node.is_text() => true,
        ast::Selector::Toml if node.is_toml() => true,
        ast::Selector::Yaml if node.is_yaml() => true,
        ast::Selector::Break if node.is_break() => true,
        ast::Selector::MdxTextExpression if node.is_mdx_text_expression() => true,
        ast::Selector::FootnoteRef if node.is_footnote_ref() => true,
        ast::Selector::ImageRef if node.is_image_ref() => true,
        ast::Selector::MdxJsxTextElement if node.is_mdx_jsx_text_element() => true,
        ast::Selector::Math if node.is_math() => true,
        ast::Selector::MdxFlowExpression if node.is_mdx_flow_expression() => true,
        ast::Selector::Definition if node.is_definition() => true,
        _ => false,
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
    } else if let Ok(re) = RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .dfa_size_limit(1 << 20)
        .build()
    {
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
    } else if let Ok(re) = RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .dfa_size_limit(1 << 20)
        .build()
    {
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
            re.split(input)
                .map(|s| s.to_owned().into())
                .collect::<Vec<_>>(),
        ))
    } else if let Ok(re) = Regex::new(pattern) {
        cache.insert(pattern.to_string(), re.clone());
        Ok(RuntimeValue::Array(
            re.split(input)
                .map(|s| s.to_owned().into())
                .collect::<Vec<_>>(),
        ))
    } else {
        Err(Error::InvalidRegularExpression(pattern.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use mq_markdown::Node;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("type", smallvec![RuntimeValue::String("test".into())], Ok(RuntimeValue::String("string".into())))]
    #[case("len", smallvec![RuntimeValue::String("test".into())], Ok(RuntimeValue::Number(4.into())))]
    #[case("abs", smallvec![RuntimeValue::Number((-10).into())], Ok(RuntimeValue::Number(10.into())))]
    #[case("ceil", smallvec![RuntimeValue::Number(3.2.into())], Ok(RuntimeValue::Number(4.0.into())))]
    #[case("floor", smallvec![RuntimeValue::Number(3.8.into())], Ok(RuntimeValue::Number(3.0.into())))]
    #[case("round", smallvec![RuntimeValue::Number(3.5.into())], Ok(RuntimeValue::Number(4.0.into())))]
    #[case("add", smallvec![RuntimeValue::Number(3.0.into()), RuntimeValue::Number(2.0.into())], Ok(RuntimeValue::Number(5.0.into())))]
    #[case("sub", smallvec![RuntimeValue::Number(5.0.into()), RuntimeValue::Number(3.0.into())], Ok(RuntimeValue::Number(2.0.into())))]
    #[case("mul", smallvec![RuntimeValue::Number(4.0.into()), RuntimeValue::Number(2.0.into())], Ok(RuntimeValue::Number(8.0.into())))]
    #[case("div", smallvec![RuntimeValue::Number(8.0.into()), RuntimeValue::Number(2.0.into())], Ok(RuntimeValue::Number(4.0.into())))]
    #[case("eq", smallvec![RuntimeValue::String("test".into()), RuntimeValue::String("test".into())], Ok(RuntimeValue::Bool(true)))]
    #[case("ne", smallvec![RuntimeValue::String("test".into()), RuntimeValue::String("different".into())], Ok(RuntimeValue::Bool(true)))]
    fn test_eval_builtin(
        #[case] func_name: &str,
        #[case] args: Args,
        #[case] expected: Result<RuntimeValue, Error>,
    ) {
        let ident = ast::Ident {
            name: CompactString::new(func_name),
            token: None,
        };

        assert_eq!(eval_builtin(&RuntimeValue::None, &ident, &args), expected);
    }

    #[rstest]
    #[case("div", smallvec![RuntimeValue::Number(1.0.into()), RuntimeValue::Number(0.0.into())], Error::ZeroDivision)]
    #[case("unknown_func", smallvec![RuntimeValue::Number(1.0.into())], Error::NotDefined("unknown_func".to_string()))]
    #[case("add", SmallVec::new(), Error::InvalidNumberOfArguments("add".to_string(), 2, 0))]
    #[case("add", smallvec![RuntimeValue::String("test".into()), RuntimeValue::Number(1.0.into())],
        Error::InvalidTypes("add".to_string(), vec![RuntimeValue::String("test".into()), RuntimeValue::Number(1.0.into())]))]
    fn test_eval_builtin_errors(
        #[case] func_name: &str,
        #[case] args: Args,
        #[case] expected_error: Error,
    ) {
        let ident = ast::Ident {
            name: CompactString::new(func_name),
            token: None,
        };

        let result = eval_builtin(&RuntimeValue::None, &ident, &args);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), expected_error);
    }

    #[test]
    fn test_implicit_first_arg() {
        let ident = ast::Ident {
            name: CompactString::new("starts_with"),
            token: None,
        };

        let first_arg = RuntimeValue::String("hello world".into());
        let args = smallvec![RuntimeValue::String("hello".into())];

        let result = eval_builtin(&first_arg, &ident, &args);
        assert_eq!(result, Ok(RuntimeValue::Bool(true)));
    }
    #[rstest]
    #[case::code(
        Node::Code(mq_markdown::Code { value: "test".into(), lang: Some("rust".into()), fence: true, meta: None, position: None }),
        ast::Selector::Code(Some("rust".into())),
        true
    )]
    #[case::code_wrong_lang(
        Node::Code(mq_markdown::Code { value: "test".into(), lang: Some("rust".into()), fence: true, meta: None, position: None }),
        ast::Selector::Code(Some("python".into())),
        false
    )]
    #[case::inline_code(
        Node::CodeInline(mq_markdown::CodeInline { value: "test".into(), position: None }),
        ast::Selector::InlineCode,
        true
    )]
    #[case::inline_math(
        Node::MathInline(mq_markdown::MathInline { value: "test".into(), position: None }),
        ast::Selector::InlineMath,
        true
    )]
    #[case::strong(
        Node::Strong(mq_markdown::Strong { values: vec!["test".to_string().into()], position: None }),
        ast::Selector::Strong,
        true
    )]
    #[case::emphasis(
        Node::Emphasis(mq_markdown::Emphasis{ values: vec!["test".to_string().into()], position: None }),
        ast::Selector::Emphasis,
        true
    )]
    #[case::delete(
        Node::Delete(mq_markdown::Delete{ values: vec!["test".to_string().into()], position: None }),
        ast::Selector::Delete,
        true
    )]
    #[case::link(
        Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("https://example.com".into()), values: Vec::new(), title: None, position: None }),
        ast::Selector::Link,
        true
    )]
    #[case::heading_matching_depth(
        Node::Heading(mq_markdown::Heading { depth: 2, values: vec!["test".to_string().into()], position: None }),
        ast::Selector::Heading(Some(2)),
        true
    )]
    #[case::heading_wrong_depth(
        Node::Heading(mq_markdown::Heading { depth: 2, values: vec!["test".to_string().into()], position: None }),
        ast::Selector::Heading(Some(3)),
        false
    )]
    #[case::table_cell_with_matching_row_col(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()],
                                               last_cell_in_row: false, last_cell_of_in_table: false, position: None }),
        ast::Selector::Table(Some(1), Some(2)),
        true
    )]
    #[case::table_cell_with_wrong_row(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()],
                                               last_cell_in_row: false, last_cell_of_in_table: false, position: None }),
        ast::Selector::Table(Some(2), Some(2)),
        false
    )]
    #[case::table_cell_with_only_row(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()],
                                               last_cell_in_row: false, last_cell_of_in_table: false, position: None }),
        ast::Selector::Table(Some(1), None),
        true
    )]
    #[case::list_with_matching_index_checked(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], index: 1, level: 1, checked: Some(true), position: None }),
        ast::Selector::List(Some(1), Some(true)),
        true
    )]
    #[case::list_with_wrong_index(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], index: 1, level: 1, checked: Some(true), position: None }),
        ast::Selector::List(Some(2), Some(true)),
        false
    )]
    #[case::list_without_index(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], index: 1, level: 1, checked: Some(true), position: None }),
        ast::Selector::List(None, None),
        true
    )]
    #[case::text(
        Node::Text(mq_markdown::Text { value: "test".into(), position: None }),
        ast::Selector::Text,
        true
    )]
    #[case::html(
        Node::Html(mq_markdown::Html { value: "<div>test</div>".into(), position: None }),
        ast::Selector::Html,
        true
    )]
    #[case::yaml(
        Node::Yaml(mq_markdown::Yaml { value: "test".into(), position: None }),
        ast::Selector::Yaml,
        true
    )]
    #[case::toml(
        Node::Toml(mq_markdown::Toml { value: "test".into(), position: None }),
        ast::Selector::Toml,
        true
    )]
    #[case::break_(
        Node::Break(mq_markdown::Break{position: None}),
        ast::Selector::Break,
        true
    )]
    #[case::image(
        Node::Image(mq_markdown::Image { alt: "".to_string(), url: "".to_string(), title: None, position: None }),
        ast::Selector::Image,
        true
    )]
    #[case::image_ref(
        Node::ImageRef(mq_markdown::ImageRef{ alt: "".to_string(), ident: "".to_string(), label: None, position: None }),
        ast::Selector::ImageRef,
        true
    )]
    #[case::footnote(
        Node::Footnote(mq_markdown::Footnote{ident: "".to_string(), values: vec!["test".to_string().into()], position: None}),
        ast::Selector::Footnote,
        true
    )]
    #[case::footnote_ref(
        Node::FootnoteRef(mq_markdown::FootnoteRef{ident: "".to_string(), label: None, position: None}),
        ast::Selector::FootnoteRef,
        true
    )]
    #[case::math(
        Node::Math(mq_markdown::Math { value: "E=mc^2".into(), position: None }),
        ast::Selector::Math,
        true
    )]
    #[case::horizontal_rule(
        Node::HorizontalRule(mq_markdown::HorizontalRule{ position: None }),
        ast::Selector::HorizontalRule,
        true
    )]
    #[case::blockquote(
        Node::Blockquote(mq_markdown::Blockquote{ values: vec!["test".to_string().into()], position: None }),
        ast::Selector::Blockquote,
        true
    )]
    #[case::definition(
        Node::Definition(mq_markdown::Definition { ident: "id".to_string(), url: mq_markdown::Url::new("url".into()), label: None, title: None, position: None }),
        ast::Selector::Definition,
        true
    )]
    #[case::mdx_jsx_flow_element(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        ast::Selector::MdxJsxFlowElement,
        true
    )]
    #[case::mdx_flow_expression(
        Node::MdxFlowExpression(mq_markdown::MdxFlowExpression{ value: "value".into(), position: None }),
        ast::Selector::MdxFlowExpression,
        true
    )]
    #[case::mdx_text_expression(
        Node::MdxTextExpression(mq_markdown::MdxTextExpression{ value: "value".into(), position: None }),
        ast::Selector::MdxTextExpression,
        true
    )]
    #[case::mdx_js_esm(
        Node::MdxJsEsm(mq_markdown::MdxJsEsm{ value: "value".into(), position: None }),
        ast::Selector::MdxJsEsm,
        true
    )]
    fn test_eval_selector(
        #[case] node: Node,
        #[case] selector: ast::Selector,
        #[case] expected: bool,
    ) {
        assert_eq!(eval_selector(&node, &selector), expected);
    }

    #[rstest]
    #[case(ParamNum::None, 0, true)]
    #[case(ParamNum::None, 1, false)]
    #[case(ParamNum::Fixed(2), 2, true)]
    #[case(ParamNum::Fixed(2), 1, false)]
    #[case(ParamNum::Fixed(2), 3, false)]
    #[case(ParamNum::Range(1, 3), 1, true)]
    #[case(ParamNum::Range(1, 3), 2, true)]
    #[case(ParamNum::Range(1, 3), 3, true)]
    #[case(ParamNum::Range(1, 3), 0, false)]
    #[case(ParamNum::Range(1, 3), 4, false)]
    fn test_param_num_is_valid(
        #[case] param_num: ParamNum,
        #[case] num_args: u8,
        #[case] expected: bool,
    ) {
        assert_eq!(param_num.is_valid(num_args), expected);
    }

    #[rstest]
    #[case(ParamNum::None, 0)]
    #[case(ParamNum::Fixed(2), 2)]
    #[case(ParamNum::Range(1, 3), 1)]
    fn test_param_num_to_num(#[case] param_num: ParamNum, #[case] expected: u8) {
        assert_eq!(param_num.to_num(), expected);
    }

    #[rstest]
    #[case(ParamNum::None, 0, false)]
    #[case(ParamNum::Fixed(2), 1, true)]
    #[case(ParamNum::Fixed(2), 0, false)]
    #[case(ParamNum::Range(1, 3), 0, true)]
    #[case(ParamNum::Range(1, 3), 1, false)]
    fn test_param_num_is_missing_one_params(
        #[case] param_num: ParamNum,
        #[case] num_args: u8,
        #[case] expected: bool,
    ) {
        assert_eq!(param_num.is_missing_one_params(num_args), expected);
    }
}
