use crate::Token;
use crate::arena::Arena;
use crate::ast::{constants, node as ast};
use crate::number::Number;
use base64::prelude::*;
use compact_str::CompactString;
use itertools::Itertools;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use regex_lite::{Regex, RegexBuilder};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::BTreeMap;
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
pub type Args = Vec<RuntimeValue>;

#[derive(Clone, Debug)]
pub struct BuiltinFunction {
    pub num_params: ParamNum,
    pub func: fn(&ast::Ident, &RuntimeValue, Args) -> Result<RuntimeValue, Error>,
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
        func: fn(&ast::Ident, &RuntimeValue, Args) -> Result<RuntimeValue, Error>,
    ) -> Self {
        BuiltinFunction { num_params, func }
    }
}

pub static BUILTIN_FUNCTIONS: LazyLock<FxHashMap<CompactString, BuiltinFunction>> =
    LazyLock::new(|| {
        let mut map = FxHashMap::with_capacity_and_hasher(100, FxBuildHasher);

        map.insert(
            CompactString::new("halt"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(exit_code)] => exit(exit_code.value() as i32),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("error"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(message)] => Err(Error::UserDefined(message.to_string())),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("print"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, current_value, args| {
                match args.as_slice() {
                    [a] => {
                        println!("{}", a);
                        Ok(current_value.clone())
                    }
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("stderr"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, current_value, args| {
                match args.as_slice() {
                    [a] => {
                        eprintln!("{}", a);
                        Ok(current_value.clone())
                    }
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("type"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.first() {
                Some(value) => Ok(value.name().to_string().into()),
                None => Ok(RuntimeValue::NONE),
            }),
        );
        map.insert(
            CompactString::new(constants::ARRAY),
            BuiltinFunction::new(ParamNum::Range(0, u8::MAX), |_, _, args| {
                Ok(RuntimeValue::Array(args.to_vec()))
            }),
        );
        map.insert(
            CompactString::new("flatten"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(arrays)] => Ok(flatten(std::mem::take(arrays)).into()),
                    [a] => Ok(std::mem::take(a)),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("from_date"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(date_str)] => from_date(date_str),
                    [RuntimeValue::Markdown(node_value, _)] => {
                        from_date(node_value.value().as_str())
                    }
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("to_date"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(ms), RuntimeValue::String(format)] => {
                        to_date(*ms, Some(format.as_str()))
                    }
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
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
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
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
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("base64d"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
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
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("min"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
                        Ok(std::cmp::min(*n1, *n2).into())
                    }
                    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                        Ok(std::mem::take(std::cmp::min(s1, s2)).into())
                    }
                    [RuntimeValue::None, _] | [_, RuntimeValue::None] => Ok(RuntimeValue::NONE),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("max"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
                        Ok(std::cmp::max(*n1, *n2).into())
                    }
                    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                        Ok(std::mem::take(std::cmp::max(s1, s2)).into())
                    }
                    [RuntimeValue::None, a] | [a, RuntimeValue::None] => Ok(std::mem::take(a)),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("to_html"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::None] => Ok(RuntimeValue::NONE),
                    [RuntimeValue::String(s)] => Ok(mq_markdown::to_html(s).into()),
                    [RuntimeValue::Markdown(node_value, _)] => {
                        Ok(mq_markdown::to_html(node_value.to_string().as_str()).into())
                    }
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("to_markdown_string"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| {
                let args = flatten(args);

                Ok(mq_markdown::Markdown::new(
                    args.iter()
                        .flat_map(|arg| match arg {
                            RuntimeValue::Markdown(node, _) => vec![node.clone()],
                            a => vec![a.to_string().into()],
                        })
                        .collect(),
                )
                .to_string()
                .into())
            }),
        );
        map.insert(
            CompactString::new("to_tsv"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::None] => Ok(RuntimeValue::NONE),
                [RuntimeValue::Array(array)] => Ok(array.iter().join("\t").into()),
                [a] => Ok(a.to_string().into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_string"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [o] => Ok(o.to_string().into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("to_number"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, mut args| to_number(&mut args[0])),
        );
        map.insert(
            CompactString::new("to_array"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(std::mem::take(array))),
                    [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(
                        s.chars()
                            .map(|c| RuntimeValue::String(c.to_string()))
                            .collect(),
                    )),
                    [RuntimeValue::None] => Ok(RuntimeValue::Array(Vec::new())),
                    [value] => Ok(RuntimeValue::Array(vec![std::mem::take(value)])),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("url_encode"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
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
                    [a] => url_encode(&a.to_string()),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("to_text"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::None] => Ok(RuntimeValue::NONE),
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
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                        .markdown_node()
                        .map(|md| Ok(md.value().ends_with(&*s).into()))
                        .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
                    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                        Ok(s1.ends_with(&*s2).into())
                    }
                    [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
                        .last()
                        .map_or(Ok(RuntimeValue::FALSE), |o| {
                            eval_builtin(o, ident, vec![RuntimeValue::String(std::mem::take(s))])
                        })
                        .unwrap_or(RuntimeValue::FALSE)),
                    [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("starts_with"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                        .markdown_node()
                        .map(|md| Ok(md.value().starts_with(&*s).into()))
                        .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
                    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                        Ok(s1.starts_with(&*s2).into())
                    }
                    [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
                        .first()
                        .map_or(Ok(RuntimeValue::FALSE), |o| {
                            eval_builtin(o, ident, vec![RuntimeValue::String(std::mem::take(s))])
                        })
                        .unwrap_or(RuntimeValue::FALSE)),
                    [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("match"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(s), RuntimeValue::String(pattern)] => {
                        match_re(s, pattern)
                    }
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
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
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
                _ => Ok(RuntimeValue::NONE),
            }),
        );
        map.insert(
            CompactString::new("gsub"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, mut args| {
                match args.as_mut_slice() {
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
                                &replace_re(md.value().as_str(), &*s1, &*s2)?.to_string(),
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
                        vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("replace"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::String(s1),
                        RuntimeValue::String(s2),
                        RuntimeValue::String(s3),
                    ] => Ok(s1.replace(&*s2, &*s3).into()),
                    [
                        node @ RuntimeValue::Markdown(_, _),
                        RuntimeValue::String(s1),
                        RuntimeValue::String(s2),
                    ] => node
                        .markdown_node()
                        .map(|md| {
                            Ok(node.update_markdown_value(md.value().replace(&*s1, &*s2).as_str()))
                        })
                        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                    [
                        RuntimeValue::None,
                        RuntimeValue::String(_),
                        RuntimeValue::String(_),
                    ] => Ok(RuntimeValue::NONE),
                    [a, b, c] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("repeat"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
                        Ok(s.repeat(n.value() as usize).into())
                    }
                    [node @ RuntimeValue::Markdown(_, _), RuntimeValue::Number(n)] => node
                        .markdown_node()
                        .map(|md| {
                            Ok(node.update_markdown_value(
                                md.value().repeat(n.value() as usize).as_str(),
                            ))
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
                    [RuntimeValue::None, _] => Ok(RuntimeValue::NONE),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("explode"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
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
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("implode"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
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
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("trim"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(s)] => Ok(s.trim().to_string().into()),
                    [node @ RuntimeValue::Markdown(_, _)] => node
                        .markdown_node()
                        .map(|md| Ok(node.update_markdown_value(md.to_string().trim())))
                        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                    [RuntimeValue::None] => Ok(RuntimeValue::NONE),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("upcase"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [node @ RuntimeValue::Markdown(_, _)] => node
                        .markdown_node()
                        .map(
                            |md| Ok(node.update_markdown_value(md.value().to_uppercase().as_str())),
                        )
                        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                    [RuntimeValue::String(s)] => Ok(s.to_uppercase().into()),
                    [RuntimeValue::None] => Ok(RuntimeValue::NONE),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("update"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
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
                    [_, a] => Ok(std::mem::take(a)),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new(constants::SLICE),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::String(s),
                        RuntimeValue::Number(start),
                        RuntimeValue::Number(end),
                    ] => {
                        let chars: Vec<char> = s.chars().collect();
                        let len = chars.len();
                        let start = start.value() as isize;
                        let end = end.value() as isize;

                        let real_start = if start < 0 {
                            (len as isize + start).max(0) as usize
                        } else {
                            (start as usize).min(len)
                        };

                        let real_end = if end < 0 {
                            (len as isize + end).max(0) as usize
                        } else {
                            (end as usize).min(len)
                        };

                        if real_start >= len || real_end <= real_start {
                            return Ok("".into());
                        }

                        let sub: String = chars[real_start..real_end].iter().collect();
                        Ok(sub.into())
                    }
                    [
                        RuntimeValue::Array(arrays),
                        RuntimeValue::Number(start),
                        RuntimeValue::Number(end),
                    ] => {
                        let len = arrays.len();
                        let start = start.value() as isize;
                        let end = end.value() as isize;

                        let real_start = if start < 0 {
                            (len as isize + start).max(0) as usize
                        } else {
                            (start as usize).min(len)
                        };
                        let real_end = if end < 0 {
                            (len as isize + end).max(0) as usize
                        } else {
                            (end as usize).min(len)
                        };

                        if real_start >= len || real_end <= real_start {
                            return Ok(RuntimeValue::EMPTY_ARRAY);
                        }

                        Ok(RuntimeValue::Array(arrays[real_start..real_end].to_vec()))
                    }
                    [
                        node @ RuntimeValue::Markdown(_, _),
                        RuntimeValue::Number(start),
                        RuntimeValue::Number(end),
                    ] => node
                        .markdown_node()
                        .map(|md| {
                            let chars: Vec<char> = md.value().chars().collect();
                            let len = chars.len();
                            let start = start.value() as isize;
                            let end = end.value() as isize;

                            let real_start = if start < 0 {
                                (len as isize + start).max(0) as usize
                            } else {
                                (start as usize).min(len)
                            };
                            let real_end = if end < 0 {
                                (len as isize + end).max(0) as usize
                            } else {
                                (end as usize).min(len)
                            };

                            if real_start >= len || real_end <= real_start {
                                return Ok(node.update_markdown_value(""));
                            }

                            let sub: String = chars[real_start..real_end].iter().collect();
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
                        vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("pow"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(base), RuntimeValue::Number(exp)] => Ok(
                        RuntimeValue::Number((base.value() as i64).pow(exp.value() as u32).into()),
                    ),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("index"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                        Ok(RuntimeValue::Number(
                            (s1.find(s2.as_str())
                                .map(|v| v as isize)
                                .unwrap_or_else(|| -1) as i64)
                                .into(),
                        ))
                    }
                    [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                        .markdown_node()
                        .map(|md| {
                            Ok(RuntimeValue::Number(
                                (md.value()
                                    .find(&*s)
                                    .map(|v| v as isize)
                                    .unwrap_or_else(|| -1) as i64)
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
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("len"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.chars().count().into())),
                [node @ RuntimeValue::Markdown(_, _)] => node
                    .markdown_node()
                    .map(|md| Ok(RuntimeValue::Number(md.value().chars().count().into())))
                    .unwrap_or_else(|| Ok(RuntimeValue::Number(0.into()))),
                [a] => Ok(RuntimeValue::Number(a.len().into())),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new("utf8bytelen"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [a] => Ok(RuntimeValue::Number(a.len().into())),
                _ => unreachable!(),
            }),
        );

        map.insert(
            CompactString::new("rindex"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                        Ok(RuntimeValue::Number(
                            s1.rfind(&*s2)
                                .map(|v| v as isize)
                                .unwrap_or_else(|| -1)
                                .into(),
                        ))
                    }
                    [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                        .markdown_node()
                        .map(|md| {
                            Ok(RuntimeValue::Number(
                                md.value()
                                    .rfind(&*s)
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
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new(constants::RANGE),
            BuiltinFunction::new(ParamNum::Range(1, 3), |ident, _, mut args| {
                match args.as_mut_slice() {
                    // Numeric range: range(end)
                    [RuntimeValue::Number(end)] => {
                        let end_val = end.value() as isize;
                        generate_numeric_range(0, end_val, 1).map(RuntimeValue::Array)
                    }
                    // Numeric range: range(start, end)
                    [RuntimeValue::Number(start), RuntimeValue::Number(end)] => {
                        let start_val = start.value() as isize;
                        let end_val = end.value() as isize;
                        let step = if start_val <= end_val { 1 } else { -1 };
                        generate_numeric_range(start_val, end_val, step).map(RuntimeValue::Array)
                    }
                    // Numeric range: range(start, end, step)
                    [
                        RuntimeValue::Number(start),
                        RuntimeValue::Number(end),
                        RuntimeValue::Number(step),
                    ] => {
                        let start_val = start.value() as isize;
                        let end_val = end.value() as isize;
                        let step_val = step.value() as isize;
                        generate_numeric_range(start_val, end_val, step_val)
                            .map(RuntimeValue::Array)
                    }
                    // String range: range("a", "z") or range("A", "Z") or range("aa", "zz")
                    [RuntimeValue::String(start), RuntimeValue::String(end)] => {
                        let start_chars: Vec<char> = start.chars().collect();
                        let end_chars: Vec<char> = end.chars().collect();

                        if start_chars.len() == 1 && end_chars.len() == 1 {
                            generate_char_range(start_chars[0], end_chars[0], None)
                                .map(RuntimeValue::Array)
                        } else {
                            generate_multi_char_range(start, end).map(RuntimeValue::Array)
                        }
                    }
                    // String range with step: range("a", "z", step)
                    [
                        RuntimeValue::String(start),
                        RuntimeValue::String(end),
                        RuntimeValue::Number(step),
                    ] => {
                        let start_chars: Vec<char> = start.chars().collect();
                        let end_chars: Vec<char> = end.chars().collect();

                        if start_chars.len() == 1 && end_chars.len() == 1 {
                            let step_val = step.value() as i32;
                            generate_char_range(start_chars[0], end_chars[0], Some(step_val))
                                .map(RuntimeValue::Array)
                        } else {
                            Err(Error::Runtime(
                                "String range with step is only supported for single characters"
                                    .to_string(),
                            ))
                        }
                    }
                    _ => Err(Error::InvalidTypes(ident.to_string(), args.to_vec())),
                }
            }),
        );
        map.insert(
            CompactString::new("del"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(array), RuntimeValue::Number(n)] => {
                        let mut array = std::mem::take(array);
                        array.remove(n.value() as usize);
                        Ok(RuntimeValue::Array(array))
                    }
                    [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
                        let mut s = std::mem::take(s).chars().collect::<Vec<_>>();
                        s.remove(n.value() as usize);
                        Ok(s.into_iter().collect::<String>().into())
                    }
                    [RuntimeValue::None, RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
                    [RuntimeValue::Dict(dict), RuntimeValue::String(key)] => {
                        let mut dict = std::mem::take(dict);
                        dict.remove(key);
                        Ok(RuntimeValue::Dict(dict))
                    }
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("join"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(array), RuntimeValue::String(s)] => {
                        Ok(array.iter().join(s).into())
                    }
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("reverse"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(array)] => {
                        let mut vec = std::mem::take(array);
                        vec.reverse();
                        Ok(RuntimeValue::Array(vec))
                    }
                    [RuntimeValue::String(s)] => Ok(s.chars().rev().collect::<String>().into()),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("sort"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(array)] => {
                        let mut vec = std::mem::take(array);
                        vec.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

                        let vec = vec
                            .into_iter()
                            .map(|v| match v {
                                RuntimeValue::Markdown(mut node, s) => {
                                    node.set_position(None);
                                    RuntimeValue::Markdown(node, s)
                                }
                                _ => v,
                            })
                            .collect();
                        Ok(RuntimeValue::Array(vec))
                    }
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("_sort_by_impl"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(array)] => {
                        let mut vec = std::mem::take(array);
                        vec.sort_by(|a, b| match (a, b) {
                            (RuntimeValue::Array(a1), RuntimeValue::Array(a2)) => a1
                                .first()
                                .unwrap()
                                .partial_cmp(a2.first().unwrap())
                                .unwrap_or(std::cmp::Ordering::Equal),
                            _ => unreachable!(),
                        });
                        let vec = vec
                            .into_iter()
                            .map(|v| match v {
                                RuntimeValue::Array(mut arr) if arr.len() >= 2 => {
                                    if let RuntimeValue::Markdown(node, s) = &arr[1] {
                                        let mut new_node = node.clone();
                                        new_node.set_position(None);

                                        arr[1] = RuntimeValue::Markdown(new_node, s.clone());
                                        RuntimeValue::Array(arr)
                                    } else {
                                        RuntimeValue::Array(arr)
                                    }
                                }
                                _ => unreachable!(),
                            })
                            .collect();

                        Ok(RuntimeValue::Array(vec))
                    }
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("compact"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(
                        std::mem::take(array)
                            .into_iter()
                            .filter(|v| !v.is_none())
                            .collect::<Vec<_>>(),
                    )),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("split"),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(split_re(s1, s2)?),
                    [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
                        .markdown_node()
                        .map(|md| split_re(md.value().as_str(), s))
                        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                    [RuntimeValue::Array(array), v] => {
                        if array.is_empty() {
                            return Ok(RuntimeValue::Array(vec![RuntimeValue::Array(Vec::new())]));
                        }

                        let mut positions = Vec::new();
                        for (i, a) in array.iter().enumerate() {
                            if a == v {
                                positions.push(i);
                            }
                        }

                        if positions.is_empty() {
                            return Ok(RuntimeValue::Array(vec![RuntimeValue::Array(
                                std::mem::take(array),
                            )]));
                        }

                        let mut result = Vec::with_capacity(positions.len() + 1);
                        let mut start = 0;

                        for pos in positions {
                            result.push(RuntimeValue::Array(array[start..pos].to_vec()));
                            start = pos + 1;
                        }

                        if start < array.len() {
                            result.push(RuntimeValue::Array(array[start..].to_vec()));
                        }

                        Ok(RuntimeValue::Array(result))
                    }
                    [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::EMPTY_ARRAY),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("uniq"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Array(array)] => {
                        let mut vec = std::mem::take(array);
                        let mut seen = FxHashSet::default();
                        vec.retain(|item| seen.insert(item.to_string()));
                        Ok(RuntimeValue::Array(vec))
                    }
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("ceil"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ceil().into())),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("floor"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().floor().into())),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("round"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().round().into())),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("trunc"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().trunc().into())),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("abs"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().abs().into())),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new(constants::EQ),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [a, b] => Ok((a == b).into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new(constants::NE),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [a, b] => Ok((a != b).into()),
                _ => unreachable!(),
            }),
        );
        map.insert(
            CompactString::new(constants::GT),
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
            CompactString::new(constants::GTE),
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
            CompactString::new(constants::LT),
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
            CompactString::new(constants::LTE),
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
            CompactString::new(constants::ADD),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
                        s1.push_str(s2);
                        Ok(std::mem::take(s1).into())
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
                        let mut a = std::mem::take(a1);
                        a.reserve(a2.len());
                        a.extend_from_slice(a2);
                        Ok(RuntimeValue::Array(a))
                    }
                    [RuntimeValue::Array(a1), a2] => {
                        let mut a = std::mem::take(a1);
                        a.reserve(1);
                        a.push(std::mem::take(a2));
                        Ok(RuntimeValue::Array(a))
                    }
                    [a, RuntimeValue::None] | [RuntimeValue::None, a] => Ok(std::mem::take(a)),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new(constants::SUB),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 - *n2).into()),
                    [a, b] => match (to_number(a)?, to_number(b)?) {
                        (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => {
                            Ok((n1 - n2).into())
                        }
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new(constants::DIV),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
                        if n2.is_zero() {
                            Err(Error::ZeroDivision)
                        } else {
                            Ok((*n1 / *n2).into())
                        }
                    }
                    [a, b] => match (to_number(a)?, to_number(b)?) {
                        (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => {
                            Ok((n1 / n2).into())
                        }
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new(constants::MUL),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 * *n2).into()),
                    [a, b] => match (to_number(a)?, to_number(b)?) {
                        (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => {
                            Ok((n1 * n2).into())
                        }
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new(constants::MOD),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 % *n2).into()),
                    [a, b] => match (to_number(a)?, to_number(b)?) {
                        (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => {
                            Ok((n1 % n2).into())
                        }
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new(constants::AND),
            BuiltinFunction::new(ParamNum::Range(2, u8::MAX), |_, _, args| {
                Ok(args.iter().all(|arg| arg.is_truthy()).into())
            }),
        );
        map.insert(
            CompactString::new(constants::OR),
            BuiltinFunction::new(ParamNum::Range(2, u8::MAX), |_, _, args| {
                Ok(args.iter().any(|arg| arg.is_truthy()).into())
            }),
        );
        map.insert(
            CompactString::new(constants::NOT),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [a] => Ok((!a.is_truthy()).into()),
                _ => unreachable!(),
            }),
        );

        // markdown
        map.insert(
            CompactString::new(constants::ATTR),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Markdown(node, _), RuntimeValue::String(attr)] => {
                        let value = node.attr(attr);
                        match value {
                            Some(val) => Ok(RuntimeValue::String(val)),
                            None => Ok(RuntimeValue::None),
                        }
                    }
                    [a, ..] => Ok(std::mem::take(a)),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("set_attr"),
            BuiltinFunction::new(ParamNum::Fixed(3), |_, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::Markdown(node, selector),
                        RuntimeValue::String(attr),
                        RuntimeValue::String(value),
                    ] => {
                        let mut new_node = std::mem::replace(node, mq_markdown::Node::Empty);
                        new_node.set_attr(attr, value);
                        Ok(RuntimeValue::Markdown(new_node, selector.take()))
                    }
                    [a, ..] => Ok(std::mem::take(a)),
                    _ => unreachable!(),
                }
            }),
        );
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
                _ => Ok(RuntimeValue::NONE),
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
                _ => Ok(RuntimeValue::NONE),
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
                _ => Ok(RuntimeValue::NONE),
            }),
        );
        map.insert(
            CompactString::new("increase_header_level"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Markdown(node, selector)] => {
                        if let mq_markdown::Node::Heading(heading) = node {
                            if heading.depth < 6 {
                                heading.depth += 1;
                            }
                            Ok(mq_markdown::Node::Heading(std::mem::take(heading)).into())
                        } else {
                            Ok(RuntimeValue::Markdown(
                                std::mem::replace(node, mq_markdown::Node::Empty),
                                selector.take(),
                            ))
                        }
                    }
                    [a] => Ok(std::mem::take(a)),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("decrease_header_level"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Markdown(node, selector)] => {
                        if let mq_markdown::Node::Heading(heading) = node {
                            if heading.depth > 1 {
                                heading.depth -= 1;
                            }
                            Ok(mq_markdown::Node::Heading(std::mem::take(heading)).into())
                        } else {
                            Ok(RuntimeValue::Markdown(
                                std::mem::replace(node, mq_markdown::Node::Empty),
                                selector.take(),
                            ))
                        }
                    }
                    [a] => Ok(std::mem::take(a)),
                    _ => unreachable!(),
                }
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
            BuiltinFunction::new(ParamNum::Fixed(3), |_, _, mut args| {
                match args.as_mut_slice() {
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
                            Some(mq_markdown::Title::new((&*title).into()))
                        },
                        position: None,
                    })
                    .into()),
                    _ => Ok(RuntimeValue::NONE),
                }
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
                _ => Ok(RuntimeValue::NONE),
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
                _ => Ok(RuntimeValue::NONE),
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
                _ => Ok(RuntimeValue::NONE),
            }),
        );
        map.insert(
            CompactString::new("to_md_name"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(node, _)] => Ok(node.name().to_string().into()),
                _ => Ok(RuntimeValue::NONE),
            }),
        );
        map.insert(
            CompactString::new("set_list_ordered"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::Markdown(mq_markdown::Node::List(list), _),
                        RuntimeValue::Bool(ordered),
                    ] => Ok(mq_markdown::Node::List(mq_markdown::List {
                        ordered: *ordered,
                        ..std::mem::take(list)
                    })
                    .into()),
                    [a, ..] => Ok(std::mem::take(a)),
                    _ => Ok(RuntimeValue::NONE),
                }
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
                _ => Ok(RuntimeValue::NONE),
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
                _ => Ok(RuntimeValue::NONE),
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
                _ => Ok(RuntimeValue::NONE),
            }),
        );
        map.insert(
            CompactString::new("to_md_list"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, args| match args.as_slice() {
                [RuntimeValue::Markdown(node, _), RuntimeValue::Number(level)] => {
                    Ok(mq_markdown::Node::List(mq_markdown::List {
                        values: node.node_values(),
                        index: 0,
                        ordered: false,
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
                        ordered: false,
                        level: level.value() as u8,
                        checked: None,
                        position: None,
                    })
                    .into())
                }
                _ => Ok(RuntimeValue::NONE),
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
            CompactString::new("get_title"),
            BuiltinFunction::new(ParamNum::Fixed(1), |_, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::Markdown(
                            mq_markdown::Node::Definition(mq_markdown::Definition {
                                title, ..
                            }),
                            _,
                        )
                        | RuntimeValue::Markdown(
                            mq_markdown::Node::Link(mq_markdown::Link { title, .. }),
                            _,
                        ),
                    ] => std::mem::take(title)
                        .map(|t| Ok(RuntimeValue::String(t.to_value())))
                        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                    [
                        RuntimeValue::Markdown(
                            mq_markdown::Node::Image(mq_markdown::Image { title, .. }),
                            _,
                        ),
                    ] => std::mem::take(title)
                        .map(|t| Ok(RuntimeValue::String(t)))
                        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                    [_] => Ok(RuntimeValue::NONE),
                    _ => unreachable!(),
                }
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
                _ => Ok(RuntimeValue::NONE),
            }),
        );
        map.insert(
            CompactString::new("set_check"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::Markdown(mq_markdown::Node::List(list), _),
                        RuntimeValue::Bool(checked),
                    ] => Ok(mq_markdown::Node::List(mq_markdown::List {
                        checked: Some(*checked),
                        ..std::mem::take(list)
                    })
                    .into()),
                    [a, ..] => Ok(std::mem::take(a)),
                    _ => Ok(RuntimeValue::NONE),
                }
            }),
        );
        map.insert(
            CompactString::new("set_ref"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::Markdown(mq_markdown::Node::Definition(def), _),
                        RuntimeValue::String(s),
                    ] => Ok(mq_markdown::Node::Definition(mq_markdown::Definition {
                        label: Some(s.to_owned()),
                        ..std::mem::take(def)
                    })
                    .into()),
                    [
                        RuntimeValue::Markdown(mq_markdown::Node::ImageRef(image_ref), _),
                        RuntimeValue::String(s),
                    ] => Ok(mq_markdown::Node::ImageRef(mq_markdown::ImageRef {
                        label: if s == &image_ref.ident {
                            None
                        } else {
                            Some(s.to_owned())
                        },
                        ..std::mem::take(image_ref)
                    })
                    .into()),
                    [
                        RuntimeValue::Markdown(mq_markdown::Node::LinkRef(link_ref), _),
                        RuntimeValue::String(s),
                    ] => Ok(mq_markdown::Node::LinkRef(mq_markdown::LinkRef {
                        label: if s == &link_ref.ident {
                            None
                        } else {
                            Some(s.to_owned())
                        },
                        ..std::mem::take(link_ref)
                    })
                    .into()),
                    [
                        RuntimeValue::Markdown(mq_markdown::Node::Footnote(footnote), _),
                        RuntimeValue::String(s),
                    ] => Ok(mq_markdown::Node::Footnote(mq_markdown::Footnote {
                        ident: s.to_owned(),
                        ..std::mem::take(footnote)
                    })
                    .into()),
                    [
                        RuntimeValue::Markdown(mq_markdown::Node::FootnoteRef(footnote_ref), _),
                        RuntimeValue::String(s),
                    ] => Ok(mq_markdown::Node::FootnoteRef(mq_markdown::FootnoteRef {
                        label: Some(s.to_owned()),
                        ..std::mem::take(footnote_ref)
                    })
                    .into()),
                    [a, ..] => Ok(std::mem::take(a)),
                    _ => Ok(RuntimeValue::NONE),
                }
            }),
        );

        map.insert(
            CompactString::new("set_code_block_lang"),
            BuiltinFunction::new(ParamNum::Fixed(2), |_, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::Markdown(mq_markdown::Node::Code(code), _),
                        RuntimeValue::String(lang),
                    ] => {
                        let mut new_code = std::mem::take(code);
                        new_code.lang = if lang.is_empty() {
                            None
                        } else {
                            Some(std::mem::take(lang))
                        };
                        Ok(mq_markdown::Node::Code(new_code).into())
                    }
                    [a, ..] => Ok(std::mem::take(a)),
                    _ => Ok(RuntimeValue::NONE),
                }
            }),
        );

        map.insert(
            CompactString::new(constants::DICT),
            BuiltinFunction::new(ParamNum::Range(0, u8::MAX), |_, _, args| {
                if args.is_empty() {
                    Ok(RuntimeValue::new_dict())
                } else {
                    let mut dict = BTreeMap::default();

                    let entries = match args.as_slice() {
                        [RuntimeValue::Array(entries)] => match entries.as_slice() {
                            [RuntimeValue::Array(_)] if args.len() == 1 => entries.clone(),
                            [RuntimeValue::Array(inner)] => inner.clone(),
                            [RuntimeValue::String(_), ..] => {
                                vec![entries.clone().into()]
                            }
                            _ => entries.clone(),
                        },
                        _ => args,
                    };

                    for entry in entries {
                        if let RuntimeValue::Array(arr) = entry {
                            if arr.len() >= 2 {
                                dict.insert(arr[0].to_string(), arr[1].clone());
                            } else {
                                return Err(Error::InvalidTypes("dict".to_string(), arr.clone()));
                            }
                        } else {
                            return Err(Error::InvalidTypes(
                                "dict".to_string(),
                                vec![entry.clone()],
                            ));
                        }
                    }

                    Ok(dict.into())
                }
            }),
        );
        map.insert(
            CompactString::new(constants::GET),
            BuiltinFunction::new(ParamNum::Fixed(2), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Dict(map), RuntimeValue::String(key)] => Ok(map
                        .get_mut(key)
                        .map(std::mem::take)
                        .unwrap_or(RuntimeValue::NONE)),
                    [RuntimeValue::Array(array), RuntimeValue::Number(index)] => Ok(array
                        .get_mut(index.value() as usize)
                        .map(std::mem::take)
                        .unwrap_or(RuntimeValue::NONE)),
                    [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
                        match s.chars().nth(n.value() as usize) {
                            Some(o) => Ok(o.to_string().into()),
                            None => Ok(RuntimeValue::NONE),
                        }
                    }
                    [RuntimeValue::Markdown(node, _), RuntimeValue::Number(i)] => {
                        Ok(RuntimeValue::Markdown(
                            std::mem::replace(node, mq_markdown::Node::Empty),
                            Some(runtime_value::Selector::Index(i.value() as usize)),
                        ))
                    }
                    [RuntimeValue::None, _] => Ok(RuntimeValue::NONE),
                    [a, b] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("set"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [
                        RuntimeValue::Dict(map_val),
                        RuntimeValue::String(key_val),
                        value_val,
                    ] => {
                        let mut new_dict = std::mem::take(map_val);
                        new_dict.insert(std::mem::take(key_val), std::mem::take(value_val));
                        Ok(RuntimeValue::Dict(new_dict))
                    }
                    [
                        RuntimeValue::Array(array_val),
                        RuntimeValue::Number(index_val),
                        value_val,
                    ] => {
                        let index = index_val.value() as usize;

                        // Extend array size if necessary
                        let mut new_array = if index >= array_val.len() {
                            // If index is out of bounds, extend array and fill with None
                            let mut resized_array = Vec::with_capacity(index + 1);
                            resized_array.extend_from_slice(array_val);
                            resized_array.resize(index + 1, RuntimeValue::NONE);
                            resized_array
                        } else {
                            // If index is within bounds, clone existing array
                            std::mem::take(array_val)
                        };

                        // Set value at specified index
                        new_array[index] = std::mem::take(value_val);
                        Ok(RuntimeValue::Array(new_array))
                    }
                    [a, b, c] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("keys"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Dict(map)] => {
                        let keys = map
                            .keys()
                            .map(|k| RuntimeValue::String(k.to_owned()))
                            .collect::<Vec<RuntimeValue>>();
                        Ok(RuntimeValue::Array(keys))
                    }
                    [RuntimeValue::None] => Ok(RuntimeValue::NONE),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("values"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Dict(map)] => {
                        let values = map.values().cloned().collect::<Vec<RuntimeValue>>();
                        Ok(RuntimeValue::Array(values))
                    }
                    [RuntimeValue::None] => Ok(RuntimeValue::NONE),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("entries"),
            BuiltinFunction::new(ParamNum::Fixed(1), |ident, _, mut args| {
                match args.as_mut_slice() {
                    [RuntimeValue::Dict(map)] => {
                        let entries = map
                            .iter()
                            .map(|(k, v)| {
                                RuntimeValue::Array(vec![
                                    RuntimeValue::String(k.to_owned()),
                                    v.to_owned(),
                                ])
                            })
                            .collect::<Vec<RuntimeValue>>();
                        Ok(RuntimeValue::Array(entries))
                    }
                    [RuntimeValue::None] => Ok(RuntimeValue::NONE),
                    [a] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a)],
                    )),
                    _ => unreachable!(),
                }
            }),
        );
        map.insert(
            CompactString::new("insert"),
            BuiltinFunction::new(ParamNum::Fixed(3), |ident, _, mut args| {
                match args.as_mut_slice() {
                    // Insert into array at index
                    [
                        RuntimeValue::Array(array),
                        RuntimeValue::Number(index),
                        value,
                    ] => {
                        let mut new_array = std::mem::take(array);
                        let idx = index.value() as usize;
                        if idx > new_array.len() {
                            new_array.resize(idx, RuntimeValue::NONE);
                        }
                        new_array.insert(idx, std::mem::take(value));
                        Ok(RuntimeValue::Array(new_array))
                    }
                    // Insert into string at index
                    [RuntimeValue::String(s), RuntimeValue::Number(index), value] => {
                        let mut chars: Vec<char> = s.chars().collect();
                        let idx = index.value() as usize;
                        let insert_str = value.to_string();
                        if idx > chars.len() {
                            chars.resize(idx, ' ');
                        }
                        for (i, c) in insert_str.chars().enumerate() {
                            chars.insert(idx + i, c);
                        }
                        let result: String = chars.into_iter().collect();
                        Ok(RuntimeValue::String(result))
                    }
                    // Insert into dict (same as set, but error if key exists)
                    [
                        RuntimeValue::Dict(map_val),
                        RuntimeValue::String(key_val),
                        value_val,
                    ] => {
                        let mut new_dict = std::mem::take(map_val);
                        new_dict.insert(std::mem::take(key_val), std::mem::take(value_val));
                        Ok(RuntimeValue::Dict(new_dict))
                    }
                    [a, b, c] => Err(Error::InvalidTypes(
                        ident.to_string(),
                        vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
                    )),
                    _ => unreachable!(),
                }
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
        let mut map = FxHashMap::with_capacity_and_hasher(100, FxBuildHasher);

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
            CompactString::new(".h6"),
            BuiltinSelectorDoc {
                description: "Selects a heading node with the 6 depth.",
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

pub static INTERNAL_FUNCTION_DOC: LazyLock<FxHashMap<CompactString, BuiltinFunctionDoc>> =
    LazyLock::new(|| {
        let mut map = FxHashMap::default();

        map.insert(
            CompactString::new("_sort_by_impl"),
            BuiltinFunctionDoc{
                description: "Internal implementation of sort_by functionality that sorts arrays of arrays using the first element as the key.",
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
        let mut map = FxHashMap::with_capacity_and_hasher(100, FxBuildHasher);

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
            CompactString::new("print"),
            BuiltinFunctionDoc {
                description: "Prints a message to standard output and returns the current value.",
                params: &["message"],
            },
        );
        map.insert(
            CompactString::new("stderr"),
            BuiltinFunctionDoc {
                description: "Prints a message to standard error and returns the current value.",
                params: &["message"],
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
            CompactString::new(constants::ARRAY),
            BuiltinFunctionDoc {
                description: "Creates an array from the given values.",
                params: &["values"],
            },
        );
        map.insert(
            CompactString::new("flatten"),
            BuiltinFunctionDoc {
                description: "Flattens a nested array into a single level array.",
                params: &["array"],
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
            CompactString::new("to_string"),
            BuiltinFunctionDoc {
                description: "Converts the given value to a string.",
                params: &["value"],
            },
        );
        map.insert(
            CompactString::new("to_markdown_string"),
            BuiltinFunctionDoc {
                description: "Converts the given value(s) to a markdown string representation.",
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
            CompactString::new("to_array"),
            BuiltinFunctionDoc {
                description: "Converts the given value to an array.",
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
                params: &["from","pattern",  "to"],
            },
        );
        map.insert(
            CompactString::new("replace"),
            BuiltinFunctionDoc {
                description: "Replaces all occurrences of a substring with another substring.",
                params: &["from", "pattern", "to"],
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
            CompactString::new(constants::SLICE),
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
            CompactString::new(constants::EQ),
            BuiltinFunctionDoc {
                description: "Checks if two values are equal.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::NE),
            BuiltinFunctionDoc {
                description: "Checks if two values are not equal.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::GT),
            BuiltinFunctionDoc {
                description: "Checks if the first value is greater than the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::GTE),
            BuiltinFunctionDoc {
                description:
                    "Checks if the first value is greater than or equal to the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::LT),
            BuiltinFunctionDoc {
                description: "Checks if the first value is less than the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::LTE),
            BuiltinFunctionDoc {
                description: "Checks if the first value is less than or equal to the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::ADD),
            BuiltinFunctionDoc {
                description: "Adds two values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::SUB),
            BuiltinFunctionDoc {
                description: "Subtracts the second value from the first value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::DIV),
            BuiltinFunctionDoc {
                description: "Divides the first value by the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::MUL),
            BuiltinFunctionDoc {
                description: "Multiplies two values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::MOD),
            BuiltinFunctionDoc {
                description: "Calculates the remainder of the division of the first value by the second value.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::AND),
            BuiltinFunctionDoc {
                description: "Performs a logical AND operation on two boolean values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::OR),
            BuiltinFunctionDoc {
                description: "Performs a logical OR operation on two boolean values.",
                params: &["value1", "value2"],
            },
        );
        map.insert(
            CompactString::new(constants::NOT),
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
            CompactString::new(constants::ATTR),
            BuiltinFunctionDoc {
                description: "Retrieves the value of the specified attribute from a markdown node.",
                params: &["markdown", "attribute"],
            },
        );
        map.insert(
            CompactString::new("set_attr"),
            BuiltinFunctionDoc {
                description: "Sets the value of the specified attribute on a markdown node.",
                params: &["markdown", "attribute", "value"],
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
            CompactString::new("set_list_ordered"),
            BuiltinFunctionDoc {
                description: "Sets the ordered property of a markdown list node.",
                params: &["list", "ordered"],
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
        map.insert(
            CompactString::new("set_code_block_lang"),
            BuiltinFunctionDoc {
                description: "Sets the language of a markdown code block node.",
                params: &["code_block", "language"],
            },
        );
        map.insert(
            CompactString::new(constants::DICT),
            BuiltinFunctionDoc {
                description: "Creates a new, empty dict.",
                params: &[],
            },
        );
        map.insert(
            CompactString::new(constants::GET),
            BuiltinFunctionDoc {
                description: "Retrieves a value from a dict by its key. Returns None if the key is not found.",
                params: &["obj", "key"],
            },
        );
        map.insert(
            CompactString::new("set"),
            BuiltinFunctionDoc {
                description: "Sets a key-value pair in a dict. If the key exists, its value is updated. Returns the modified map.",
                params: &["obj", "key", "value"],
            },
        );
        map.insert(
            CompactString::new("keys"),
            BuiltinFunctionDoc {
                description: "Returns an array of keys from the dict.",
                params: &["dict"],
            },
        );
        map.insert(
            CompactString::new("values"),
            BuiltinFunctionDoc {
                description: "Returns an array of values from the dict.",
                params: &["dict"],
            },
        );
        map.insert(
            CompactString::new("entries"),
            BuiltinFunctionDoc {
                description: "Returns an array of key-value pairs from the dict as arrays.",
                params: &["dict"],
            },
        );
        map.insert(
            CompactString::new(constants::RANGE),
            BuiltinFunctionDoc {
                description: "Creates an array from start to end with an optional step.",
                params: &["start", "end", "step"],
            },
        );
        map.insert(
            CompactString::new("insert"),
            BuiltinFunctionDoc {
            description: "Inserts a value into an array or string at the specified index, or into a dict with the specified key.",
            params: &["target", "index_or_key", "value"],
            },
        );
        map.insert(
            CompactString::new("increase_header_level"),
            BuiltinFunctionDoc {
                description:
                    "Increases the level of a markdown heading node by one, up to a maximum of 6.",
                params: &["heading_node"],
            },
        );
        map.insert(
            CompactString::new("decrease_header_level"),
            BuiltinFunctionDoc {
            description: "Decreases the level of a markdown heading node by one, down to a minimum of 1.",
            params: &["heading_node"],
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
    args: Args,
) -> Result<RuntimeValue, Error> {
    BUILTIN_FUNCTIONS.get(&ident.name).map_or_else(
        || Err(Error::NotDefined(ident.to_string())),
        |f| {
            let args_len = args.len() as u8;
            if f.num_params.is_valid(args_len) {
                (f.func)(ident, runtime_value, args)
            } else if f.num_params.is_missing_one_params(args_len) {
                let mut new_args: Args = vec![runtime_value.clone()];
                new_args.extend(args);
                (f.func)(ident, runtime_value, new_args)
            } else {
                Err(Error::InvalidNumberOfArguments(
                    ident.to_string(),
                    f.num_params.to_num(),
                    args_len,
                ))
            }
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
    } else if let Ok(re) = RegexBuilder::new(pattern).size_limit(1 << 20).build() {
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
    } else if let Ok(re) = RegexBuilder::new(pattern).size_limit(1 << 20).build() {
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

#[inline(always)]
fn generate_numeric_range(
    start: isize,
    end: isize,
    step: isize,
) -> Result<Vec<RuntimeValue>, Error> {
    if step == 0 {
        return Err(Error::Runtime(
            "step for range must not be zero".to_string(),
        ));
    }

    let mut result = Vec::with_capacity(((end - start) / step).unsigned_abs() + 1);
    let mut current = start;

    if step > 0 {
        while current <= end {
            result.push(RuntimeValue::Number(current.into()));
            current += step;
        }
    } else {
        while current >= end {
            result.push(RuntimeValue::Number(current.into()));
            current += step;
        }
    }

    Ok(result)
}

#[inline(always)]
fn generate_char_range(
    start_char: char,
    end_char: char,
    step: Option<i32>,
) -> Result<Vec<RuntimeValue>, Error> {
    let step = step.unwrap_or(if start_char <= end_char { 1 } else { -1 });

    if step == 0 {
        return Err(Error::Runtime(
            "step for range must not be zero".to_string(),
        ));
    }

    let capacity = (end_char as i32 - start_char as i32).unsigned_abs() as usize + 1;
    let mut result = Vec::with_capacity(capacity);
    let mut current = start_char as i32;
    let end = end_char as i32;

    if step > 0 {
        while current <= end {
            if let Some(ch) = char::from_u32(current as u32) {
                result.push(RuntimeValue::String(ch.to_string()));
            }
            current += step;
        }
    } else {
        while current >= end {
            if let Some(ch) = char::from_u32(current as u32) {
                result.push(RuntimeValue::String(ch.to_string()));
            }
            current += step;
        }
    }

    Ok(result)
}

#[inline(always)]
fn generate_multi_char_range(start: &str, end: &str) -> Result<Vec<RuntimeValue>, Error> {
    if start.len() != end.len() {
        return Err(Error::Runtime(
            "String range requires strings of equal length".to_string(),
        ));
    }

    let start_bytes = start.as_bytes();
    let end_bytes = end.as_bytes();
    let mut result = Vec::with_capacity(
        (end_bytes.iter().zip(start_bytes.iter()))
            .map(|(e, s)| e.max(s) - e.min(s))
            .sum::<u8>() as usize
            + 1,
    );
    let mut current = start_bytes.to_vec();

    loop {
        if let Ok(s) = String::from_utf8(current.clone()) {
            result.push(RuntimeValue::String(s));
        }

        if current.as_slice() == end_bytes {
            break;
        }

        // Lexicographic increment
        let mut carry = true;
        for byte in current.iter_mut().rev() {
            if carry {
                if *byte < 255 {
                    *byte += 1;
                    carry = false;
                } else {
                    *byte = 0;
                }
            }
        }

        if carry || current.as_slice() > end_bytes {
            break;
        }
    }

    Ok(result)
}

#[inline(always)]
fn flatten(args: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
    let mut result = Vec::new();
    for arg in args {
        match arg {
            RuntimeValue::Array(arr) => result.extend(flatten(arr)),
            other => result.push(other),
        }
    }
    result
}

fn to_number(value: &mut RuntimeValue) -> Result<RuntimeValue, Error> {
    match value {
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
        RuntimeValue::Array(array) => {
            let result_value: Result<Vec<RuntimeValue>, Error> = std::mem::take(array)
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
                    RuntimeValue::Bool(b) => Ok(RuntimeValue::Number(if b { 1 } else { 0 }.into())),
                    n @ RuntimeValue::Number(_) => Ok(n),
                    _ => Ok(RuntimeValue::Number(0.into())),
                })
                .collect();

            result_value.map(RuntimeValue::Array)
        }
        RuntimeValue::Bool(true) => Ok(RuntimeValue::Number(1.into())),
        RuntimeValue::Bool(false) => Ok(RuntimeValue::Number(0.into())),
        _ => Ok(RuntimeValue::Number(0.into())),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mq_markdown::Node;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("type", vec![RuntimeValue::String("test".into())], Ok(RuntimeValue::String("string".into())))]
    #[case("len", vec![RuntimeValue::String("test".into())], Ok(RuntimeValue::Number(4.into())))]
    #[case("abs", vec![RuntimeValue::Number((-10).into())], Ok(RuntimeValue::Number(10.into())))]
    #[case("ceil", vec![RuntimeValue::Number(3.2.into())], Ok(RuntimeValue::Number(4.0.into())))]
    #[case("floor", vec![RuntimeValue::Number(3.8.into())], Ok(RuntimeValue::Number(3.0.into())))]
    #[case("round", vec![RuntimeValue::Number(3.5.into())], Ok(RuntimeValue::Number(4.0.into())))]
    #[case("add", vec![RuntimeValue::Number(3.0.into()), RuntimeValue::Number(2.0.into())], Ok(RuntimeValue::Number(5.0.into())))]
    #[case("sub", vec![RuntimeValue::Number(5.0.into()), RuntimeValue::Number(3.0.into())], Ok(RuntimeValue::Number(2.0.into())))]
    #[case("mul", vec![RuntimeValue::Number(4.0.into()), RuntimeValue::Number(2.0.into())], Ok(RuntimeValue::Number(8.0.into())))]
    #[case("div", vec![RuntimeValue::Number(8.0.into()), RuntimeValue::Number(2.0.into())], Ok(RuntimeValue::Number(4.0.into())))]
    #[case("eq", vec![RuntimeValue::String("test".into()), RuntimeValue::String("test".into())], Ok(RuntimeValue::Bool(true)))]
    #[case("ne", vec![RuntimeValue::String("test".into()), RuntimeValue::String("different".into())], Ok(RuntimeValue::Bool(true)))]
    fn test_eval_builtin(
        #[case] func_name: &str,
        #[case] args: Args,
        #[case] expected: Result<RuntimeValue, Error>,
    ) {
        let ident = ast::Ident {
            name: CompactString::new(func_name),
            token: None,
        };

        assert_eq!(eval_builtin(&RuntimeValue::None, &ident, args), expected);
    }

    #[rstest]
    #[case("div", vec![RuntimeValue::Number(1.0.into()), RuntimeValue::Number(0.0.into())], Error::ZeroDivision)]
    #[case("unknown_func", vec![RuntimeValue::Number(1.0.into())], Error::NotDefined("unknown_func".to_string()))]
    #[case("add", Vec::new(), Error::InvalidNumberOfArguments("add".to_string(), 2, 0))]
    #[case("add", vec![RuntimeValue::String("test".into()), RuntimeValue::Number(1.0.into())],
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

        let result = eval_builtin(&RuntimeValue::None, &ident, args);
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
        let args = vec![RuntimeValue::String("hello".into())];

        let result = eval_builtin(&first_arg, &ident, args);
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
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        ast::Selector::List(Some(1), Some(true)),
        true
    )]
    #[case::list_with_wrong_index(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        ast::Selector::List(Some(2), Some(true)),
        false
    )]
    #[case::list_without_index(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
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

    // Tests for Dict functions
    #[test]
    fn test_eval_builtin_new_dict() {
        let ident = ast::Ident {
            name: CompactString::new("dict"),
            token: None,
        };
        let result = eval_builtin(&RuntimeValue::None, &ident, vec![]);
        assert!(result.is_ok());
        let map_val = result.unwrap();
        match map_val {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 0);
            }
            _ => panic!("Expected Dict, got {:?}", map_val),
        }

        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Array(vec![
                RuntimeValue::String("key".into()),
                RuntimeValue::String("value".into()),
            ])],
        );
        assert_eq!(
            result,
            Ok(RuntimeValue::Dict(BTreeMap::from([(
                "key".into(),
                RuntimeValue::String("value".into())
            )])))
        );
    }

    #[test]
    fn test_eval_builtin_set_dict() {
        let ident_set = ast::Ident {
            name: CompactString::new("set"),
            token: None,
        };
        let initial_map = RuntimeValue::new_dict();

        let args1 = vec![
            initial_map.clone(),
            RuntimeValue::String("name".into()),
            RuntimeValue::String("Jules".into()),
        ];
        let result1 = eval_builtin(&RuntimeValue::None, &ident_set, args1);
        assert!(result1.is_ok());
        let map_val1 = result1.unwrap();
        match &map_val1 {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 1);
                assert_eq!(map.get("name"), Some(&RuntimeValue::String("Jules".into())));
            }
            _ => panic!("Expected Dict, got {:?}", map_val1),
        }

        let args2 = vec![
            map_val1.clone(),
            RuntimeValue::String("age".into()),
            RuntimeValue::Number(30.into()),
        ];
        let result2 = eval_builtin(&RuntimeValue::None, &ident_set, args2);
        assert!(result2.is_ok());
        let map_val2 = result2.unwrap();
        match &map_val2 {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(map.get("name"), Some(&RuntimeValue::String("Jules".into())));
                assert_eq!(map.get("age"), Some(&RuntimeValue::Number(30.into())));
            }
            _ => panic!("Expected Dict, got {:?}", map_val2),
        }

        let args3 = vec![
            map_val2.clone(),
            RuntimeValue::String("name".into()),
            RuntimeValue::String("Vincent".into()),
        ];
        let result3 = eval_builtin(&RuntimeValue::None, &ident_set, args3);
        assert!(result3.is_ok());
        let map_val3 = result3.unwrap();
        match &map_val3 {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(
                    map.get("name"),
                    Some(&RuntimeValue::String("Vincent".into()))
                );
                assert_eq!(map.get("age"), Some(&RuntimeValue::Number(30.into())));
            }
            _ => panic!("Expected Dict, got {:?}", map_val3),
        }

        let mut nested_map_data = BTreeMap::default();
        nested_map_data.insert("level".into(), RuntimeValue::Number(2.into()));
        let nested_map: RuntimeValue = nested_map_data.into();
        let args4 = vec![
            map_val3.clone(),
            RuntimeValue::String("nested".into()),
            nested_map.clone(),
        ];
        let result4 = eval_builtin(&RuntimeValue::None, &ident_set, args4);
        assert!(result4.is_ok());
        match result4.unwrap() {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 3);
                assert_eq!(map.get("nested"), Some(&nested_map));
            }
            _ => panic!("Expected Dict"),
        }

        let args_err1 = vec![
            RuntimeValue::String("not_a_map".into()),
            RuntimeValue::String("key".into()),
            RuntimeValue::String("value".into()),
        ];
        let result_err1 = eval_builtin(&RuntimeValue::None, &ident_set, args_err1);
        assert_eq!(
            result_err1,
            Err(Error::InvalidTypes(
                "set".to_string(),
                vec![
                    RuntimeValue::String("not_a_map".into()),
                    RuntimeValue::String("key".into()),
                    RuntimeValue::String("value".into())
                ]
            ))
        );

        let args_err2 = vec![
            initial_map.clone(),
            RuntimeValue::Number(123.into()),
            RuntimeValue::String("value".into()),
        ];
        let result_err2 = eval_builtin(&RuntimeValue::None, &ident_set, args_err2);
        assert_eq!(
            result_err2,
            Err(Error::InvalidTypes(
                "set".to_string(),
                vec![
                    initial_map.clone(),
                    RuntimeValue::Number(123.into()),
                    RuntimeValue::String("value".into())
                ]
            ))
        );
    }

    #[test]
    fn test_eval_builtin_get_map() {
        let ident_get = ast::Ident {
            name: CompactString::new("get"),
            token: None,
        };
        let mut map_data = BTreeMap::default();
        map_data.insert("name".into(), RuntimeValue::String("Jules".into()));
        map_data.insert("age".into(), RuntimeValue::Number(30.into()));
        let map_val: RuntimeValue = map_data.into();

        let args1 = vec![map_val.clone(), RuntimeValue::String("name".into())];
        let result1 = eval_builtin(&RuntimeValue::None, &ident_get, args1);
        assert_eq!(result1, Ok(RuntimeValue::String("Jules".into())));

        let args2 = vec![map_val.clone(), RuntimeValue::String("location".into())];
        let result2 = eval_builtin(&RuntimeValue::None, &ident_get, args2);
        assert_eq!(result2, Ok(RuntimeValue::None));

        let args_err1 = vec![
            RuntimeValue::String("not_a_map".into()),
            RuntimeValue::String("key".into()),
        ];
        let result_err1 = eval_builtin(&RuntimeValue::None, &ident_get, args_err1);
        assert_eq!(
            result_err1,
            Err(Error::InvalidTypes(
                "get".to_string(),
                vec![
                    RuntimeValue::String("not_a_map".into()),
                    RuntimeValue::String("key".into())
                ]
            ))
        );

        let args_err2 = vec![map_val.clone(), RuntimeValue::Number(123.into())];
        let result_err2 = eval_builtin(&RuntimeValue::None, &ident_get, args_err2);
        assert_eq!(
            result_err2,
            Err(Error::InvalidTypes(
                "get".to_string(),
                vec![map_val.clone(), RuntimeValue::Number(123.into())]
            ))
        );
    }

    #[test]
    fn test_eval_builtin_keys_dict() {
        let ident_keys = ast::Ident {
            name: CompactString::new("keys"),
            token: None,
        };

        let empty_map = RuntimeValue::new_dict();
        let args1 = vec![empty_map.clone()];
        let result1 = eval_builtin(&RuntimeValue::None, &ident_keys, args1);
        assert_eq!(result1, Ok(RuntimeValue::Array(vec![])));

        let mut map_data = BTreeMap::default();
        map_data.insert("name".into(), RuntimeValue::String("Jules".into()));
        map_data.insert("age".into(), RuntimeValue::Number(30.into()));
        let map_val: RuntimeValue = map_data.into();
        let args2 = vec![map_val.clone()];
        let result2 = eval_builtin(&RuntimeValue::None, &ident_keys, args2);
        assert!(result2.is_ok());
        match result2.unwrap() {
            RuntimeValue::Array(keys_array) => {
                assert_eq!(keys_array.len(), 2);
                let keys_str: Vec<String> = keys_array
                    .into_iter()
                    .map(|k| match k {
                        RuntimeValue::String(s) => s,
                        _ => panic!("Expected string key"),
                    })
                    .collect();
                assert_eq!(keys_str, vec!["age".to_string(), "name".to_string()]);
            }
            _ => panic!("Expected Array of keys"),
        }

        let args_err1 = vec![RuntimeValue::String("not_a_map".into())];
        let result_err1 = eval_builtin(&RuntimeValue::None, &ident_keys, args_err1);
        assert_eq!(
            result_err1,
            Err(Error::InvalidTypes(
                "keys".to_string(),
                vec![RuntimeValue::String("not_a_map".into())]
            ))
        );

        let args_err2 = vec![map_val.clone(), RuntimeValue::String("extra".into())];
        let result_err2 = eval_builtin(&RuntimeValue::None, &ident_keys, args_err2);
        assert_eq!(
            result_err2,
            Err(Error::InvalidNumberOfArguments("keys".to_string(), 1, 2))
        );
    }

    #[test]
    fn test_eval_builtin_values_dict() {
        let ident_values = ast::Ident {
            name: CompactString::new("values"),
            token: None,
        };

        let empty_map = RuntimeValue::new_dict();
        let args1 = vec![empty_map.clone()];
        let result1 = eval_builtin(&RuntimeValue::None, &ident_values, args1);
        assert_eq!(result1, Ok(RuntimeValue::Array(vec![])));

        let mut map_data = BTreeMap::default();
        map_data.insert("name".into(), RuntimeValue::String("Jules".into()));
        map_data.insert("age".into(), RuntimeValue::Number(30.into()));
        let map_val: RuntimeValue = map_data.into();
        let args2 = vec![map_val.clone()];
        let result2 = eval_builtin(&RuntimeValue::None, &ident_values, args2);
        assert!(result2.is_ok());
        match result2.unwrap() {
            RuntimeValue::Array(values_array) => {
                assert_eq!(values_array.len(), 2);
                assert!(values_array.contains(&RuntimeValue::String("Jules".into())));
                assert!(values_array.contains(&RuntimeValue::Number(30.into())));
            }
            _ => panic!("Expected Array of values"),
        }

        let args_err1 = vec![RuntimeValue::String("not_a_map".into())];
        let result_err1 = eval_builtin(&RuntimeValue::None, &ident_values, args_err1);
        assert_eq!(
            result_err1,
            Err(Error::InvalidTypes(
                "values".to_string(),
                vec![RuntimeValue::String("not_a_map".into())]
            ))
        );

        let args_err2 = vec![map_val.clone(), RuntimeValue::String("extra".into())];
        let result_err2 = eval_builtin(&RuntimeValue::None, &ident_values, args_err2);
        assert_eq!(
            result_err2,
            Err(Error::InvalidNumberOfArguments("values".to_string(), 1, 2))
        );
    }
}
