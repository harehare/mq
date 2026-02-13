use crate::arena::Arena;
use crate::ast::{constants, node as ast};
use crate::error::runtime::RuntimeError;
use crate::eval::env::{self, Env};
use crate::ident::all_symbols;
use crate::number::{self, Number};
use crate::selector::Selector;
use crate::{Ident, Shared, SharedCell, Token, get_token, parse_markdown_input, parse_mdx_input};
use base64::prelude::*;
use itertools::Itertools;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use regex_lite::{Regex, RegexBuilder};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use std::io;
use std::process::exit;
use std::{
    sync::{LazyLock, Mutex},
    vec,
};
use thiserror::Error;

use super::runtime_value::{self, RuntimeValue};
use mq_markdown;

static REGEX_CACHE: LazyLock<Mutex<FxHashMap<String, Regex>>> = LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// Maximum number of elements allowed in a generated range
const MAX_RANGE_SIZE: usize = 1_000_000;
const MAX_REPEAT_COUNT: usize = 1_000;

type FunctionName = String;
type ErrorArgs = Vec<RuntimeValue>;
type SharedEnv = Shared<SharedCell<Env>>;
pub type Args = Vec<RuntimeValue>;

#[derive(Clone, Debug)]
pub struct BuiltinFunction {
    pub name: &'static str,
    pub num_params: ParamNum,
    pub func: fn(&Ident, &RuntimeValue, Args, &SharedEnv) -> Result<RuntimeValue, Error>,
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
        name: &'static str,
        num_params: ParamNum,
        func: fn(&Ident, &RuntimeValue, Args, &SharedEnv) -> Result<RuntimeValue, Error>,
    ) -> Self {
        BuiltinFunction { name, num_params, func }
    }
}

macro_rules! define_builtin {
    ($name:ident, $params:expr, $body:expr) => {
        static $name: LazyLock<BuiltinFunction> =
            LazyLock::new(|| BuiltinFunction::new(stringify!($name).to_lowercase().leak(), $params, $body));
    };
}

define_builtin!(HALT, ParamNum::Fixed(1), |ident: &Ident,
                                           _: &RuntimeValue,
                                           mut args: Args,
                                           _| {
    match args.as_mut_slice() {
        [RuntimeValue::Number(exit_code)] => exit(exit_code.value() as i32),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!(),
    }
});

define_builtin!(
    ERROR,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::String(message)] => Err(Error::UserDefined(message.to_string())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    PRINT,
    ParamNum::Fixed(1),
    |_, current_value, args, _| match args.as_slice() {
        [a] => {
            println!("{}", a);
            Ok(current_value.clone())
        }
        _ => unreachable!(),
    }
);

define_builtin!(
    STDERR,
    ParamNum::Fixed(1),
    |_, current_value, args, _| match args.as_slice() {
        [a] => {
            eprintln!("{}", a);
            Ok(current_value.clone())
        }
        _ => unreachable!(),
    }
);

define_builtin!(TYPE, ParamNum::Fixed(1), |_, _, args, _| match args.first() {
    Some(value) => Ok(value.name().to_string().into()),
    None => Ok(RuntimeValue::NONE),
});

define_builtin!(ARRAY, ParamNum::Range(0, u8::MAX), |_, _, args, _| Ok(
    RuntimeValue::Array(args.to_vec())
));

define_builtin!(
    FLATTEN,
    ParamNum::Fixed(1),
    |_, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Array(arrays)] => Ok(flatten(std::mem::take(arrays)).into()),
        [a] => Ok(std::mem::take(a)),
        _ => unreachable!(),
    }
);

define_builtin!(
    FROM_DATE,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::String(date_str)] => from_date(date_str),
        [RuntimeValue::Markdown(node_value, _)] => from_date(node_value.value().as_str()),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    TO_DATE,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(ms), RuntimeValue::String(format)] => {
            to_date(*ms, Some(format.as_str()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
);

define_builtin!(NOW, ParamNum::None, |_, _, _, _| {
    Ok(RuntimeValue::Number(
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| Error::Runtime(format!("{}", e)))?
            .as_millis() as i64)
            .into(),
    ))
});

define_builtin!(BASE64, ParamNum::Fixed(1), |ident, _, mut args, _| {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!(),
    }
});

define_builtin!(
    BASE64D,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    MIN,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
            Ok(std::cmp::min(*n1, *n2).into())
        }
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
            Ok(std::mem::take(std::cmp::min(s1, s2)).into())
        }
        [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => {
            Ok(std::mem::take(std::cmp::min(s1, s2)).into())
        }
        [RuntimeValue::None, _] | [_, RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
);

define_builtin!(
    MAX,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
            Ok(std::cmp::max(*n1, *n2).into())
        }
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
            Ok(std::mem::take(std::cmp::max(s1, s2)).into())
        }
        [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => {
            Ok(std::mem::take(std::cmp::max(s1, s2)).into())
        }
        [RuntimeValue::None, a] | [a, RuntimeValue::None] => Ok(std::mem::take(a)),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
);

define_builtin!(
    TO_HTML,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [RuntimeValue::String(s)] => Ok(mq_markdown::to_html(s).into()),
        [RuntimeValue::Symbol(s)] => Ok(mq_markdown::to_html(&s.as_str()).into()),
        [RuntimeValue::Markdown(node_value, _)] => {
            Ok(mq_markdown::to_html(node_value.to_string().as_str()).into())
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(TO_MARKDOWN_STRING, ParamNum::Fixed(1), |_, _, args, _| {
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
});

define_builtin!(TO_STRING, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [o] => Ok(o.to_string().into()),
    _ => unreachable!(),
});

define_builtin!(TO_NUMBER, ParamNum::Fixed(1), |_, _, mut args, _| to_number(
    &mut args[0]
));

define_builtin!(
    TO_ARRAY,
    ParamNum::Fixed(1),
    |_, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(std::mem::take(array))),
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(
            s.chars().map(|c| RuntimeValue::String(c.to_string())).collect(),
        )),
        [RuntimeValue::None] => Ok(RuntimeValue::Array(Vec::new())),
        [value] => Ok(RuntimeValue::Array(vec![std::mem::take(value)])),
        _ => unreachable!(),
    }
);

define_builtin!(
    URL_ENCODE,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
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
);

define_builtin!(TO_TEXT, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [RuntimeValue::None] => Ok(RuntimeValue::NONE),
    [RuntimeValue::Markdown(node_value, _)] => Ok(node_value.value().into()),
    [RuntimeValue::Array(array)] => Ok(array
        .iter()
        .map(|a| { if a.is_none() { "".to_string() } else { a.to_string() } })
        .join(",")
        .into()),
    [value] => Ok(value.to_string().into()),
    _ => unreachable!(),
});

define_builtin!(ENDS_WITH, ParamNum::Fixed(2), |ident, _, mut args, env| {
    match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(md.value().ends_with(&*s).into()))
            .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.ends_with(&*s2).into()),
        [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
            .last()
            .map_or(Ok(RuntimeValue::FALSE), |o| {
                eval_builtin(o, ident, vec![RuntimeValue::String(std::mem::take(s))], env)
            })
            .unwrap_or(RuntimeValue::FALSE)),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(STARTS_WITH, ParamNum::Fixed(2), |ident, _, mut args, env| {
    match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(md.value().starts_with(&*s).into()))
            .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.starts_with(&*s2).into()),
        [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
            .first()
            .map_or(Ok(RuntimeValue::FALSE), |o| {
                eval_builtin(o, ident, vec![RuntimeValue::String(std::mem::take(s))], env)
            })
            .unwrap_or(RuntimeValue::FALSE)),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(REGEX_MATCH, ParamNum::Fixed(2), |ident, _, mut args, _| {
    match args.as_mut_slice() {
        [RuntimeValue::String(s), RuntimeValue::String(pattern)] => match_re(s, pattern),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(pattern)] => node
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
});

define_builtin!(CAPTURE, ParamNum::Fixed(2), |ident, _, mut args, _| {
    match args.as_mut_slice() {
        [RuntimeValue::String(s), RuntimeValue::String(pattern)] => capture_re(s, pattern),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(pattern)] => node
            .markdown_node()
            .map(|md| capture_re(&md.value(), pattern))
            .unwrap_or_else(|| Ok(RuntimeValue::new_dict())),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::new_dict()),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(DOWNCASE, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [node @ RuntimeValue::Markdown(_, _)] => node
        .markdown_node()
        .map(|md| Ok(node.update_markdown_value(md.value().to_lowercase().as_str())))
        .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
    [RuntimeValue::String(s)] => Ok(s.to_lowercase().into()),
    _ => Ok(RuntimeValue::NONE),
});

define_builtin!(GSUB, ParamNum::Fixed(3), |ident, _, mut args, _| {
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
            .map(|md| Ok(node.update_markdown_value(&replace_re(md.value().as_str(), &*s1, &*s2)?.to_string())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None, _, _] => Ok(RuntimeValue::NONE),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(REPLACE, ParamNum::Fixed(3), |ident, _, mut args, _| {
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
            .map(|md| Ok(node.update_markdown_value(md.value().replace(&*s1, &*s2).as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None, RuntimeValue::String(_), RuntimeValue::String(_)] => Ok(RuntimeValue::NONE),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(
    REPEAT,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [v, RuntimeValue::Number(n)] => {
            repeat(v, n.value() as usize)
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
);

define_builtin!(
    EXPLODE,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    IMPLODE,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => {
            let result: String = array
                .iter()
                .map(|o| match o {
                    RuntimeValue::Number(n) => std::char::from_u32(n.value() as u32).unwrap_or_default().to_string(),
                    _ => "".to_string(),
                })
                .collect();
            Ok(result.into())
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    TRIM,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(s.trim().to_string().into()),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.to_string().trim())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    UPCASE,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().to_uppercase().as_str())),)
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::String(s)] => Ok(s.to_uppercase().into()),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    UPDATE,
    ParamNum::Fixed(2),
    |_, _, mut args, _| match args.as_mut_slice() {
        [
            node1 @ RuntimeValue::Markdown(_, _),
            node2 @ RuntimeValue::Markdown(_, _),
        ] => node2
            .markdown_node()
            .map(|md| Ok(node1.update_markdown_value(&md.value())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::Markdown(node_value, _), RuntimeValue::String(s)] => Ok(node_value.with_value(s).into()),
        [RuntimeValue::None, _] => Ok(RuntimeValue::NONE),
        [_, a] => Ok(std::mem::take(a)),
        _ => unreachable!(),
    }
);

define_builtin!(SLICE, ParamNum::Fixed(3), |ident, _, mut args, _| {
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
        [RuntimeValue::None, RuntimeValue::Number(_), RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(POW, ParamNum::Fixed(2), |ident, _, mut args, _| {
    match args.as_mut_slice() {
        [RuntimeValue::Number(base), RuntimeValue::Number(exp)] => Ok(RuntimeValue::Number(
            (base.value() as i64).pow(exp.value() as u32).into(),
        )),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(INDEX, ParamNum::Fixed(2), |ident, _, mut args, _| {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(
            (s1.find(s2.as_str()).map(|v| v as isize).unwrap_or_else(|| -1) as i64).into(),
        )),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| {
                Ok(RuntimeValue::Number(
                    (md.value().find(&*s).map(|v| v as isize).unwrap_or_else(|| -1) as i64).into(),
                ))
            })
            .unwrap_or_else(|| Ok(RuntimeValue::Number((-1_i64).into()))),
        [RuntimeValue::Array(array), v] => Ok(array
            .iter()
            .position(|o| o == v)
            .map(|i| RuntimeValue::Number((i as i64).into()))
            .unwrap_or(RuntimeValue::Number((-1_i64).into()))),
        [RuntimeValue::None, _] => Ok(RuntimeValue::Number((-1_i64).into())),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(LEN, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.chars().count().into())),
    [node @ RuntimeValue::Markdown(_, _)] => node
        .markdown_node()
        .map(|md| Ok(RuntimeValue::Number(md.value().chars().count().into())))
        .unwrap_or_else(|| Ok(RuntimeValue::Number(0.into()))),
    [a] => Ok(RuntimeValue::Number(a.len().into())),
    _ => unreachable!(),
});

define_builtin!(UTF8BYTELEN, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [a] => Ok(RuntimeValue::Number(a.len().into())),
    _ => unreachable!(),
});

define_builtin!(
    RINDEX,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
            Ok(RuntimeValue::Number(
                s1.rfind(&*s2).map(|v| v as isize).unwrap_or_else(|| -1).into(),
            ))
        }
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| {
                Ok(RuntimeValue::Number(
                    md.value().rfind(&*s).map(|v| v as isize).unwrap_or_else(|| -1).into(),
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
);

define_builtin!(
    RANGE,
    ParamNum::Range(1, 3),
    |ident, _, mut args, _| match args.as_mut_slice() {
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
            generate_numeric_range(start_val, end_val, step_val).map(RuntimeValue::Array)
        }
        // String range: range("a", "z") or range("A", "Z") or range("aa", "zz")
        [RuntimeValue::String(start), RuntimeValue::String(end)] => {
            let start_chars: Vec<char> = start.chars().collect();
            let end_chars: Vec<char> = end.chars().collect();

            if start_chars.len() == 1 && end_chars.len() == 1 {
                generate_char_range(start_chars[0], end_chars[0], None).map(RuntimeValue::Array)
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
                generate_char_range(start_chars[0], end_chars[0], Some(step_val)).map(RuntimeValue::Array)
            } else {
                Err(Error::Runtime(
                    "String range with step is only supported for single characters".to_string(),
                ))
            }
        }
        _ => Err(Error::InvalidTypes(ident.to_string(), args.to_vec())),
    }
);

define_builtin!(
    DEL,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
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
            dict.remove(&Ident::new(key));
            Ok(RuntimeValue::Dict(dict))
        }
        [RuntimeValue::Dict(dict), RuntimeValue::Symbol(key)] => {
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
);

define_builtin!(
    JOIN,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Array(array), RuntimeValue::String(s)] => {
            Ok(array.iter().join(s).into())
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
);

define_builtin!(
    REVERSE,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => {
            let mut vec = std::mem::take(array);
            vec.reverse();
            Ok(RuntimeValue::Array(vec))
        }
        [RuntimeValue::String(s)] => Ok(s.chars().rev().collect::<String>().into()),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    SORT,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    _SORT_BY_IMPL,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    COMPACT,
    ParamNum::Fixed(1),
    |_, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(
            std::mem::take(array)
                .into_iter()
                .filter(|v| !v.is_none())
                .collect::<Vec<_>>(),
        )),
        [a] => Ok(std::mem::take(a)),
        _ => unreachable!(),
    }
);

define_builtin!(
    SPLIT,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
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
                return Ok(RuntimeValue::Array(vec![RuntimeValue::Array(std::mem::take(array))]));
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
);

define_builtin!(
    UNIQ,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => {
            let mut vec = std::mem::take(array);
            let mut seen = FxHashSet::default();
            vec.retain(|item| seen.insert(item.to_string()));
            Ok(RuntimeValue::Array(vec))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    CEIL,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ceil().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    FLOOR,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().floor().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    ROUND,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().round().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    TRUNC,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().trunc().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    ABS,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().abs().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(EQ, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
    [a, b] => Ok((a == b).into()),
    _ => unreachable!(),
});

define_builtin!(NE, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
    [a, b] => Ok((a != b).into()),
    _ => unreachable!(),
});

define_builtin!(GT, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 > s2).into()),
    [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok((s1 > s2).into()),
    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 > n2).into()),
    [RuntimeValue::Boolean(b1), RuntimeValue::Boolean(b2)] => Ok((b1 > b2).into()),
    [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => {
        Ok((n1 > n2).into())
    }
    [_, _] => Ok(RuntimeValue::FALSE),
    _ => unreachable!(),
});

define_builtin!(GTE, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 >= s2).into()),
    [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok((s1 >= s2).into()),
    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 >= n2).into()),
    [RuntimeValue::Boolean(b1), RuntimeValue::Boolean(b2)] => Ok((b1 >= b2).into()),
    [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => {
        Ok((n1 >= n2).into())
    }
    [_, _] => Ok(RuntimeValue::FALSE),
    _ => unreachable!(),
});

define_builtin!(LT, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 < s2).into()),
    [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok((s1 < s2).into()),
    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 < n2).into()),
    [RuntimeValue::Boolean(b1), RuntimeValue::Boolean(b2)] => Ok((b1 < b2).into()),
    [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => {
        Ok((n1 < n2).into())
    }
    [_, _] => Ok(RuntimeValue::FALSE),
    _ => unreachable!(),
});

define_builtin!(LTE, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
    [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 <= s2).into()),
    [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok((s1 <= s2).into()),
    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 <= n2).into()),
    [RuntimeValue::Boolean(b1), RuntimeValue::Boolean(b2)] => Ok((b1 <= b2).into()),
    [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => {
        Ok((n1 <= n2).into())
    }
    [_, _] => Ok(RuntimeValue::FALSE),
    _ => unreachable!(),
});

define_builtin!(ADD, ParamNum::Fixed(2), |ident, _, mut args, _| {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
            s1.push_str(s2);
            Ok(std::mem::take(s1).into())
        }
        [RuntimeValue::String(s), RuntimeValue::Number(n)] | [RuntimeValue::Number(n), RuntimeValue::String(s)] => {
            s.push_str(n.to_string().as_str());
            Ok(std::mem::take(s).into())
        }
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(format!("{}{}", md.value(), s).as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::String(s), node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(format!("{}{}", s, md.value()).as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [
            node1 @ RuntimeValue::Markdown(_, _),
            node2 @ RuntimeValue::Markdown(_, _),
        ] => Ok(node2
            .markdown_node()
            .and_then(|md2| {
                node1
                    .markdown_node()
                    .map(|md1| node1.update_markdown_value(format!("{}{}", md1.value(), md2.value()).as_str()))
            })
            .unwrap_or(RuntimeValue::NONE)),
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 + *n2).into()),
        [RuntimeValue::Array(a1), RuntimeValue::Array(a2)] => {
            let total_size = a1.len().saturating_add(a2.len());
            if total_size > MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "array concatenation size {} exceeds maximum allowed size of {}",
                    total_size, MAX_RANGE_SIZE
                )));
            }
            let mut a = std::mem::take(a1);
            a.reserve(a2.len());
            a.extend_from_slice(a2);
            Ok(RuntimeValue::Array(a))
        }
        [RuntimeValue::Array(a1), a2] => {
            let total_size = a1.len().saturating_add(1);
            if total_size > MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "array size {} exceeds maximum allowed size of {}",
                    total_size, MAX_RANGE_SIZE
                )));
            }

            let mut a = std::mem::take(a1);
            a.reserve(1);
            a.push(std::mem::take(a2));
            Ok(RuntimeValue::Array(a))
        }
        [v, RuntimeValue::Array(a)] => {
            let total_size = a.len().saturating_add(1);
            if total_size > MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "array size {} exceeds maximum allowed size of {}",
                    total_size, MAX_RANGE_SIZE
                )));
            }

            let mut arr = Vec::with_capacity(total_size);
            arr.push(std::mem::take(v));
            arr.extend(std::mem::take(a));

            Ok(RuntimeValue::Array(arr))
        }
        [a, RuntimeValue::None] | [RuntimeValue::None, a] => Ok(std::mem::take(a)),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(SUB, ParamNum::Fixed(2), |_, _, mut args, _| {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 - *n2).into()),
        [a, b] => match (to_number(a)?, to_number(b)?) {
            (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 - n2).into()),
            _ => Err(Error::InvalidTypes(
                "Both operands could not be converted to numbers: {:?}, {:?}".to_string(),
                vec![std::mem::take(a), std::mem::take(b)],
            )),
        },
        _ => unreachable!(),
    }
});

define_builtin!(DIV, ParamNum::Fixed(2), |_, _, mut args, _| match args.as_mut_slice() {
    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
        if n2.is_zero() {
            Err(Error::ZeroDivision)
        } else {
            Ok((*n1 / *n2).into())
        }
    }
    [a, b] => match (to_number(a)?, to_number(b)?) {
        (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 / n2).into()),
        (RuntimeValue::None, _) | (_, RuntimeValue::None) => Ok(RuntimeValue::NONE),
        _ => Err(Error::InvalidTypes(
            "Both operands could not be converted to numbers: {:?}, {:?}".to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
    },
    _ => unreachable!(),
});

define_builtin!(MUL, ParamNum::Fixed(2), |_, _, mut args, _| match args.as_mut_slice() {
    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 * *n2).into()),
    [RuntimeValue::Array(array), RuntimeValue::Number(n)] | [RuntimeValue::Number(n), RuntimeValue::Array(array)] => {
        if n.is_int() && n.value() >= 0.0 && n.value() <= MAX_REPEAT_COUNT as f64 {
            // Integer multiplication within repeat limit: repeat the array
            repeat(&mut RuntimeValue::Array(std::mem::take(array)), n.value() as usize)
        } else {
            // Non-integer, negative, or too large multiplication: multiply each element
            let result: Result<Vec<RuntimeValue>, Error> = std::mem::take(array)
                .into_iter()
                .map(|v| {
                    let mut args = vec![v, RuntimeValue::Number(*n)];
                    match args.as_mut_slice() {
                        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 * *n2).into()),
                        [a, b] => match (to_number(a)?, to_number(b)?) {
                            (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 * n2).into()),
                            (RuntimeValue::None, _) | (_, RuntimeValue::None) => Ok(RuntimeValue::NONE),
                            _ => Err(Error::InvalidTypes(
                                constants::builtins::MUL.to_string(),
                                vec![std::mem::take(&mut args[0]), std::mem::take(&mut args[1])],
                            )),
                        },
                        _ => unreachable!(),
                    }
                })
                .collect();
            result.map(RuntimeValue::Array)
        }
    }
    [v, RuntimeValue::Number(n)] | [RuntimeValue::Number(n), v] => {
        if n.is_int() && n.value() >= 0.0 {
            repeat(v, n.value() as usize)
        } else {
            Err(Error::InvalidTypes(
                constants::builtins::MUL.to_string(),
                vec![std::mem::take(v), RuntimeValue::Number(*n)],
            ))
        }
    }
    [a, b] => match (to_number(a)?, to_number(b)?) {
        (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 * n2).into()),
        (RuntimeValue::None, _) | (_, RuntimeValue::None) => Ok(RuntimeValue::NONE),
        _ => Ok(RuntimeValue::Number(0.into())),
    },
    _ => unreachable!(),
});

define_builtin!(MOD, ParamNum::Fixed(2), |_, _, mut args, _| match args.as_mut_slice() {
    [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 % *n2).into()),
    [a, b] => match (to_number(a)?, to_number(b)?) {
        (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 % n2).into()),
        _ => Err(Error::InvalidTypes(
            "".to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
    },
    _ => unreachable!(),
});

define_builtin!(AND, ParamNum::Range(2, u8::MAX), |_, _, args, _| {
    let mut last_truthy = None;
    for arg in args {
        if !arg.is_truthy() {
            return Ok(RuntimeValue::Boolean(false));
        }
        let mut arg = arg;
        last_truthy = Some(std::mem::take(&mut arg));
    }
    Ok(last_truthy.unwrap_or(RuntimeValue::Boolean(true)))
});

define_builtin!(OR, ParamNum::Range(2, u8::MAX), |_, _, args, _| {
    for arg in args {
        if arg.is_truthy() {
            let mut arg = arg;
            return Ok(std::mem::take(&mut arg));
        }
    }
    Ok(RuntimeValue::Boolean(false))
});

define_builtin!(NOT, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [a] => Ok((!a.is_truthy()).into()),
    _ => unreachable!(),
});

define_builtin!(
    ATTR,
    ParamNum::Fixed(2),
    |_, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::String(attr)] =>
            Ok(node.attr(attr).map(Into::into).unwrap_or(RuntimeValue::NONE)),
        [a, ..] => Ok(std::mem::take(a)),
        _ => unreachable!(),
    }
);

define_builtin!(
    SET_ATTR,
    ParamNum::Fixed(3),
    |_, _, mut args, _| match args.as_mut_slice() {
        [
            RuntimeValue::Markdown(node, selector),
            RuntimeValue::String(attr),
            value,
        ] => {
            let mut new_node = std::mem::replace(node, mq_markdown::Node::Empty);
            let value = match value {
                RuntimeValue::String(s) => mq_markdown::AttrValue::String(s.to_string()),
                RuntimeValue::Number(n) => {
                    if n.is_int() {
                        mq_markdown::AttrValue::Integer(n.value() as i64)
                    } else {
                        mq_markdown::AttrValue::Number(n.value())
                    }
                }
                RuntimeValue::Boolean(b) => mq_markdown::AttrValue::Boolean(*b),
                RuntimeValue::None => mq_markdown::AttrValue::Null,
                _ => {
                    return Err(Error::InvalidTypes(
                        "set_attr".to_string(),
                        vec![
                            RuntimeValue::Markdown(std::mem::replace(node, mq_markdown::Node::Empty), selector.take()),
                            RuntimeValue::String(attr.clone()),
                            std::mem::take(value),
                        ],
                    ));
                }
            };
            new_node.set_attr(attr, value);
            Ok(RuntimeValue::Markdown(new_node, selector.take()))
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => unreachable!(),
    }
);

define_builtin!(TO_CODE, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
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
});

define_builtin!(
    TO_CODE_INLINE,
    ParamNum::Fixed(1),
    |_, _, args, _| match args.as_slice() {
        [a] if !a.is_none() => Ok(mq_markdown::Node::CodeInline(mq_markdown::CodeInline {
            value: a.to_string().into(),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
);

define_builtin!(TO_H, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
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
});

define_builtin!(
    INCREASE_HEADER_LEVEL,
    ParamNum::Fixed(1),
    |_, _, mut args, _| match args.as_mut_slice() {
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
);

define_builtin!(
    DECREASE_HEADER_LEVEL,
    ParamNum::Fixed(1),
    |_, _, mut args, _| match args.as_mut_slice() {
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
);

define_builtin!(TO_HR, ParamNum::Fixed(0), |_, _, _, _| {
    Ok(mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }).into())
});

define_builtin!(
    TO_LINK,
    ParamNum::Fixed(3),
    |_, _, mut args, _| match args.as_mut_slice() {
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
);

define_builtin!(TO_IMAGE, ParamNum::Fixed(3), |_, _, args, _| match args.as_slice() {
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
});

define_builtin!(TO_MATH, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [a] => Ok(mq_markdown::Node::Math(mq_markdown::Math {
        value: a.to_string(),
        position: None,
    })
    .into()),
    _ => Ok(RuntimeValue::NONE),
});

define_builtin!(
    TO_MATH_INLINE,
    ParamNum::Fixed(1),
    |_, _, args, _| match args.as_slice() {
        [a] => Ok(mq_markdown::Node::MathInline(mq_markdown::MathInline {
            value: a.to_string().into(),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
);

define_builtin!(TO_MD_NAME, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [RuntimeValue::Markdown(node, _)] => Ok(node.name().to_string().into()),
    _ => Ok(RuntimeValue::NONE),
});

define_builtin!(
    SET_LIST_ORDERED,
    ParamNum::Fixed(2),
    |_, _, mut args, _| match args.as_mut_slice() {
        [
            RuntimeValue::Markdown(mq_markdown::Node::List(list), _),
            RuntimeValue::Boolean(ordered),
        ] => Ok(mq_markdown::Node::List(mq_markdown::List {
            ordered: *ordered,
            ..std::mem::take(list)
        })
        .into()),
        [a, ..] => Ok(std::mem::take(a)),
        _ => Ok(RuntimeValue::NONE),
    }
);

define_builtin!(TO_STRONG, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
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
});

define_builtin!(TO_EM, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
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
});

define_builtin!(TO_MD_TEXT, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
    [a] if !a.is_none() => Ok(mq_markdown::Node::Text(mq_markdown::Text {
        value: a.to_string(),
        position: None,
    })
    .into()),
    _ => Ok(RuntimeValue::NONE),
});

define_builtin!(TO_MD_LIST, ParamNum::Fixed(2), |_, _, args, _| match args.as_slice() {
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
});

define_builtin!(TO_MD_TABLE_ROW, ParamNum::Range(1, u8::MAX), |_, _, args, _| {
    let mut current_index = 0;
    let values = args
        .iter()
        .flat_map(|arg| match arg {
            RuntimeValue::Array(array) => array
                .iter()
                .map(move |v| {
                    current_index += 1;
                    mq_markdown::Node::TableCell(mq_markdown::TableCell {
                        row: 0,
                        column: current_index - 1,
                        values: vec![v.to_string().into()],
                        position: None,
                    })
                })
                .collect::<Vec<_>>(),
            v => {
                current_index += 1;
                vec![mq_markdown::Node::TableCell(mq_markdown::TableCell {
                    row: 0,
                    column: current_index - 1,
                    values: vec![v.to_string().into()],
                    position: None,
                })]
            }
        })
        .collect::<Vec<_>>();

    Ok(RuntimeValue::Markdown(
        mq_markdown::Node::TableRow(mq_markdown::TableRow { values, position: None }),
        None,
    ))
});

define_builtin!(TO_MD_TABLE_CELL, ParamNum::Fixed(3), |_, _, mut args, _| {
    match args.as_mut_slice() {
        [value, RuntimeValue::Number(row), RuntimeValue::Number(column)] => Ok(RuntimeValue::Markdown(
            mq_markdown::Node::TableCell(mq_markdown::TableCell {
                row: row.value() as usize,
                column: column.value() as usize,
                values: vec![value.to_string().into()],
                position: None,
            }),
            None,
        )),
        [a, b, c] => Err(Error::InvalidTypes(
            "table_cell".to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!(),
    }
});

define_builtin!(GET_TITLE, ParamNum::Fixed(1), |_, _, mut args, _| {
    match args.as_mut_slice() {
        [
            RuntimeValue::Markdown(mq_markdown::Node::Definition(mq_markdown::Definition { title, .. }), _)
            | RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link { title, .. }), _),
        ] => std::mem::take(title)
            .map(|t| Ok(RuntimeValue::String(t.to_value())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image { title, .. }), _)] => {
            std::mem::take(title)
                .map(|t| Ok(RuntimeValue::String(t)))
                .unwrap_or_else(|| Ok(RuntimeValue::NONE))
        }
        [_] => Ok(RuntimeValue::NONE),
        _ => unreachable!(),
    }
});

define_builtin!(GET_URL, ParamNum::Fixed(1), |_, _, args, _| match args.as_slice() {
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
});

define_builtin!(SET_CHECK, ParamNum::Fixed(2), |_, _, mut args, _| {
    match args.as_mut_slice() {
        [
            RuntimeValue::Markdown(mq_markdown::Node::List(list), _),
            RuntimeValue::Boolean(checked),
        ] => Ok(mq_markdown::Node::List(mq_markdown::List {
            checked: Some(*checked),
            ..std::mem::take(list)
        })
        .into()),
        [a, ..] => Ok(std::mem::take(a)),
        _ => Ok(RuntimeValue::NONE),
    }
});

define_builtin!(SET_REF, ParamNum::Fixed(2), |_, _, mut args, _| {
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
            label: if s == &link_ref.ident { None } else { Some(s.to_owned()) },
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
});

define_builtin!(
    SET_CODE_BLOCK_LANG,
    ParamNum::Fixed(2),
    |_, _, mut args, _| match args.as_mut_slice() {
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
);

define_builtin!(DICT, ParamNum::Range(0, u8::MAX), |_, _, args, _| {
    if args.is_empty() {
        Ok(RuntimeValue::new_dict())
    } else {
        let mut dict = BTreeMap::default();

        let entries = match args.as_slice() {
            [RuntimeValue::Array(entries)] => match entries.as_slice() {
                [RuntimeValue::Array(_)] if args.len() == 1 => entries.clone(),
                [RuntimeValue::Array(inner)] => inner.clone(),
                [RuntimeValue::String(_), ..] | [RuntimeValue::Symbol(_), ..] => {
                    vec![entries.clone().into()]
                }
                _ => entries.clone(),
            },
            _ => args,
        };

        for entry in entries {
            if let RuntimeValue::Array(arr) = entry {
                match arr.as_slice() {
                    [RuntimeValue::Symbol(key), value] => {
                        dict.insert(*key, value.clone());
                        continue;
                    }
                    [key, value] => {
                        dict.insert(Ident::new(&key.to_string()), value.clone());
                        continue;
                    }
                    a => return Err(Error::InvalidTypes("dict".to_string(), a.to_vec())),
                }
            } else {
                return Err(Error::InvalidTypes("dict".to_string(), vec![entry.clone()]));
            }
        }

        Ok(dict.into())
    }
});

define_builtin!(
    GET,
    ParamNum::Fixed(2),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Dict(map), RuntimeValue::String(key)] => Ok(map
            .get_mut(&Ident::new(key))
            .map(std::mem::take)
            .unwrap_or(RuntimeValue::NONE)),
        [RuntimeValue::Dict(map), RuntimeValue::Symbol(key)] =>
            Ok(map.get_mut(key).map(std::mem::take).unwrap_or(RuntimeValue::NONE)),
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
        [RuntimeValue::None, _] | [_, RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
);

define_builtin!(
    SET,
    ParamNum::Fixed(3),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Dict(map_val), RuntimeValue::String(key_val), value_val] => {
            let mut new_dict = std::mem::take(map_val);
            new_dict.insert(Ident::new(key_val), std::mem::take(value_val));
            Ok(RuntimeValue::Dict(new_dict))
        }
        [RuntimeValue::Dict(map_val), RuntimeValue::Symbol(key_val), value_val] => {
            let mut new_dict = std::mem::take(map_val);
            new_dict.insert(*key_val, std::mem::take(value_val));
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
);

define_builtin!(
    KEYS,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Dict(map)] => {
            let keys = map
                .keys()
                .map(|k| RuntimeValue::String(k.as_str()))
                .collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(keys))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    VALUES,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Dict(map)] => {
            let values = map.values().cloned().collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(values))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    ENTRIES,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Dict(map)] => {
            let entries = map
                .iter()
                .map(|(k, v)| RuntimeValue::Array(vec![RuntimeValue::String(k.as_str()), v.to_owned()]))
                .collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(entries))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    INSERT,
    ParamNum::Fixed(3),
    |ident, _, mut args, _| match args.as_mut_slice() {
        // Insert into array at index
        [RuntimeValue::Array(array), RuntimeValue::Number(index), value] => {
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
        [RuntimeValue::Dict(map_val), RuntimeValue::String(key_val), value_val] => {
            let mut new_dict = std::mem::take(map_val);
            new_dict.insert(Ident::new(key_val), std::mem::take(value_val));
            Ok(RuntimeValue::Dict(new_dict))
        }
        [RuntimeValue::Dict(map_val), RuntimeValue::Symbol(key_val), value_val] => {
            let mut new_dict = std::mem::take(map_val);
            new_dict.insert(*key_val, std::mem::take(value_val));
            Ok(RuntimeValue::Dict(new_dict))
        }
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!(),
    }
);

define_builtin!(
    NEGATE,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(-(*n))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    INTERN,
    ParamNum::Fixed(1),
    |_, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            Ok(RuntimeValue::String(Ident::new(s).as_str()))
        }
        [a] => {
            Ok(RuntimeValue::String(Ident::new(&a.to_string()).as_str()))
        }
        _ => unreachable!(),
    }
);

define_builtin!(NAN, ParamNum::None, |_, _, _, _| {
    Ok(RuntimeValue::Number(number::NAN))
});

define_builtin!(
    IS_NAN,
    ParamNum::Fixed(1),
    |_, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => {
            Ok(RuntimeValue::Boolean(n.is_nan()))
        }
        [_] => {
            Ok(RuntimeValue::FALSE)
        }
        _ => unreachable!(),
    }
);

define_builtin!(INFINITE, ParamNum::None, |_, _, _, _| {
    Ok(RuntimeValue::Number(number::INFINITE))
});

define_builtin!(
    COALESCE,
    ParamNum::Fixed(2),
    |_, _, mut args, _| match args.as_mut_slice() {
        [a, b] => {
            if a.is_none() {
                Ok(std::mem::take(b))
            } else {
                Ok(std::mem::take(a))
            }
        }
        _ => unreachable!(),
    }
);

define_builtin!(INPUT, ParamNum::None, |_, _, _, _| {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| Error::Runtime(format!("Failed to read from stdin: {}", e)))?;
    input.truncate(input.trim_end_matches(&['\n', '\r'][..]).len());

    Ok(RuntimeValue::String(input))
});

define_builtin!(ALL_SYMBOLS, ParamNum::None, |_, _, _, _| {
    Ok(RuntimeValue::Array(
        all_symbols()
            .into_iter()
            .map(|symbol| RuntimeValue::Symbol(Ident::new(&symbol)))
            .collect(),
    ))
});

define_builtin!(
    TO_MARKDOWN,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::String(s)] =>
            Ok(RuntimeValue::Array(parse_markdown_input(s).map_err(|e| {
                Error::Runtime(format!("Failed to parse markdown: {}", e))
            })?)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    TO_MDX,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::String(s)] =>
            Ok(RuntimeValue::Array(parse_mdx_input(s).map_err(|e| {
                Error::Runtime(format!("Failed to parse mdx: {}", e))
            })?)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    _GET_MARKDOWN_POSITION,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _)] => {
            node.position()
                .map(|pos| {
                    Ok(vec![
                        ("start_line".to_string(), pos.start.line.into()),
                        ("start_column".to_string(), pos.start.column.into()),
                        ("end_line".to_string(), pos.end.line.into()),
                        ("end_column".to_string(), pos.end.column.into()),
                    ]
                    .into())
                })
                .unwrap_or(Ok(RuntimeValue::NONE))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(
    SET_VARIABLE,
    ParamNum::Fixed(2),
    |ident, value, mut args, env| match args.as_mut_slice() {
        [RuntimeValue::Symbol(var_ident), v] => {
            #[cfg(not(feature = "sync"))]
            {
                env.borrow_mut().define(std::mem::take(var_ident), std::mem::take(v));
            }

            #[cfg(feature = "sync")]
            {
                env.write()
                    .unwrap()
                    .define(std::mem::take(var_ident), std::mem::take(v));
            }

            Ok(value.clone())
        }
        [RuntimeValue::String(var_name), v] => {
            #[cfg(not(feature = "sync"))]
            {
                env.borrow_mut().define(Ident::new(var_name), std::mem::take(v));
            }

            #[cfg(feature = "sync")]
            {
                env.write().unwrap().define(Ident::new(var_name), std::mem::take(v));
            }

            Ok(value.clone())
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!(),
    }
);

define_builtin!(
    GET_VARIABLE,
    ParamNum::Fixed(1),
    |ident, _, mut args, env| match args.as_mut_slice() {
        [RuntimeValue::Symbol(var_name)] => {
            #[cfg(not(feature = "sync"))]
            {
                env.borrow().resolve(std::mem::take(var_name)).map_err(Into::into)
            }

            #[cfg(feature = "sync")]
            {
                env.read()
                    .unwrap()
                    .resolve(std::mem::take(var_name))
                    .map_err(Into::into)
            }
        }
        [RuntimeValue::String(var_name)] => {
            #[cfg(not(feature = "sync"))]
            {
                env.borrow().resolve(Ident::new(var_name)).map_err(Into::into)
            }

            #[cfg(feature = "sync")]
            {
                env.read().unwrap().resolve(Ident::new(var_name)).map_err(Into::into)
            }
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

define_builtin!(IS_DEBUG_MODE, ParamNum::None, |_, _, _, _| {
    #[cfg(feature = "debugger")]
    {
        Ok(RuntimeValue::TRUE)
    }
    #[cfg(not(feature = "debugger"))]
    {
        Ok(RuntimeValue::FALSE)
    }
});

// AST related built-ins
define_builtin!(_AST_GET_ARGS, ParamNum::Fixed(1), |_, _, args, _| {
    match args.as_slice() {
        [RuntimeValue::Ast(ast)] => match &*ast.expr {
            ast::Expr::Call(_, args) | ast::Expr::CallDynamic(_, args) => Ok(args
                .iter()
                .map(|arg| RuntimeValue::Ast(Shared::clone(arg)))
                .collect::<Vec<_>>()
                .into()),
            _ => Ok(RuntimeValue::NONE),
        },
        _ => Ok(RuntimeValue::NONE),
    }
});

define_builtin!(_AST_TO_CODE, ParamNum::Fixed(1), |_, _, args, _| {
    match args.as_slice() {
        [RuntimeValue::Ast(ast)] => Ok(ast.to_code().into()),
        [a] => Ok(a.to_string().into()),
        _ => Ok(RuntimeValue::NONE),
    }
});

#[cfg(feature = "file-io")]
define_builtin!(
    READ_FILE,
    ParamNum::Fixed(1),
    |ident, _, mut args, _| match args.as_mut_slice() {
        [RuntimeValue::String(path)] => match std::fs::read_to_string(&path) {
            Ok(content) => Ok(RuntimeValue::String(content)),
            Err(e) => Err(Error::Runtime(format!("Failed to read file {}: {}", path, e))),
        },
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)],)),
        _ => unreachable!(),
    }
);

const fn fnv1a_hash_64(s: &str) -> u64 {
    const FNV_OFFSET_BASIS_64: u64 = 14695981039346656037;
    const FNV_PRIME_64: u64 = 1099511628211;

    let bytes = s.as_bytes();
    let mut hash = FNV_OFFSET_BASIS_64;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME_64);
        i += 1;
    }
    hash
}

const HASH_ABS: u64 = fnv1a_hash_64("abs");
const HASH_ADD: u64 = fnv1a_hash_64("add");
const HASH_AND: u64 = fnv1a_hash_64("and");
const HASH_ALL_SYMBOLS: u64 = fnv1a_hash_64("all_symbols");
const HASH_ARRAY: u64 = fnv1a_hash_64(constants::builtins::ARRAY);
const HASH_ATTR: u64 = fnv1a_hash_64(constants::builtins::ATTR);
const HASH_BASE64: u64 = fnv1a_hash_64("base64");
const HASH_BASE64D: u64 = fnv1a_hash_64("base64d");
const HASH_CAPTURE: u64 = fnv1a_hash_64("capture");
const HASH_CEIL: u64 = fnv1a_hash_64("ceil");
const HASH_COMPACT: u64 = fnv1a_hash_64("compact");
const HASH_COALESCE: u64 = fnv1a_hash_64("coalesce");
const HASH_DECREASE_HEADER_LEVEL: u64 = fnv1a_hash_64("decrease_header_level");
const HASH_DEL: u64 = fnv1a_hash_64("del");
const HASH_DICT: u64 = fnv1a_hash_64(constants::builtins::DICT);
const HASH_DIV: u64 = fnv1a_hash_64(constants::builtins::DIV);
const HASH_DOWNCASE: u64 = fnv1a_hash_64("downcase");
const HASH_ENDS_WITH: u64 = fnv1a_hash_64("ends_with");
const HASH_ENTRIES: u64 = fnv1a_hash_64("entries");
const HASH_EQ: u64 = fnv1a_hash_64(constants::builtins::EQ);
const HASH_ERROR: u64 = fnv1a_hash_64("error");
const HASH_EXPLODE: u64 = fnv1a_hash_64("explode");
const HASH_AST_GET_ARGS: u64 = fnv1a_hash_64("_ast_get_args");
const HASH_AST_TO_CODE: u64 = fnv1a_hash_64("_ast_to_code");
const HASH_FLATTEN: u64 = fnv1a_hash_64("flatten");
const HASH_FLOOR: u64 = fnv1a_hash_64(constants::builtins::FLOOR);
const HASH_FROM_DATE: u64 = fnv1a_hash_64("from_date");
const HASH_GET: u64 = fnv1a_hash_64(constants::builtins::GET);
const HASH_GT: u64 = fnv1a_hash_64(constants::builtins::GT);
const HASH_GTE: u64 = fnv1a_hash_64(constants::builtins::GTE);
const HASH_GET_TITLE: u64 = fnv1a_hash_64("get_title");
const HASH_GET_URL: u64 = fnv1a_hash_64("get_url");
const HASH_GET_VARIABLE: u64 = fnv1a_hash_64("get_variable");
const HASH_GSUB: u64 = fnv1a_hash_64("gsub");
const HASH_HALT: u64 = fnv1a_hash_64("halt");
const HASH_IMPLODE: u64 = fnv1a_hash_64("implode");
const HASH_INCREASE_HEADER_LEVEL: u64 = fnv1a_hash_64("increase_header_level");
const HASH_INDEX: u64 = fnv1a_hash_64("index");
const HASH_INSERT: u64 = fnv1a_hash_64("insert");
const HASH_INFINITE: u64 = fnv1a_hash_64("infinite");
const HASH_INPUT: u64 = fnv1a_hash_64("input");
const HASH_IS_DEBUG_MODE: u64 = fnv1a_hash_64("is_debug_mode");
const HASH_IS_NAN: u64 = fnv1a_hash_64("is_nan");
const HASH_JOIN: u64 = fnv1a_hash_64("join");
const HASH_KEYS: u64 = fnv1a_hash_64("keys");
const HASH_LEN: u64 = fnv1a_hash_64("len");
const HASH_LT: u64 = fnv1a_hash_64(constants::builtins::LT);
const HASH_LTE: u64 = fnv1a_hash_64(constants::builtins::LTE);
const HASH_REGEX_MATCH: u64 = fnv1a_hash_64("regex_match");
const HASH_MAX: u64 = fnv1a_hash_64("max");
const HASH_MIN: u64 = fnv1a_hash_64("min");
const HASH_NAN: u64 = fnv1a_hash_64("nan");
const HASH_NEGATE: u64 = fnv1a_hash_64("negate");
const HASH_MOD: u64 = fnv1a_hash_64(constants::builtins::MOD);
const HASH_MUL: u64 = fnv1a_hash_64(constants::builtins::MUL);
const HASH_NE: u64 = fnv1a_hash_64(constants::builtins::NE);
const HASH_NOT: u64 = fnv1a_hash_64(constants::builtins::NOT);
const HASH_NOW: u64 = fnv1a_hash_64("now");
const HASH_OR: u64 = fnv1a_hash_64("or");
const HASH_POW: u64 = fnv1a_hash_64("pow");
const HASH_PRINT: u64 = fnv1a_hash_64("print");
const HASH_RANGE: u64 = fnv1a_hash_64(constants::builtins::RANGE);
const HASH_REPEAT: u64 = fnv1a_hash_64("repeat");
const HASH_REPLACE: u64 = fnv1a_hash_64("replace");
const HASH_REVERSE: u64 = fnv1a_hash_64("reverse");
const HASH_RINDEX: u64 = fnv1a_hash_64("rindex");
const HASH_ROUND: u64 = fnv1a_hash_64("round");
const HASH_SET: u64 = fnv1a_hash_64("set");
const HASH_SET_ATTR: u64 = fnv1a_hash_64("set_attr");
const HASH_SET_CHECK: u64 = fnv1a_hash_64("set_check");
const HASH_SET_CODE_BLOCK_LANG: u64 = fnv1a_hash_64("set_code_block_lang");
const HASH_SET_LIST_ORDERED: u64 = fnv1a_hash_64("set_list_ordered");
const HASH_SET_REF: u64 = fnv1a_hash_64("set_ref");
const HASH_SET_VARIABLE: u64 = fnv1a_hash_64("set_variable");
const HASH_SLICE: u64 = fnv1a_hash_64(constants::builtins::SLICE);
const HASH_SORT: u64 = fnv1a_hash_64("sort");
const HASH_SORT_BY_IMPL: u64 = fnv1a_hash_64("_sort_by_impl");
const HASH_SPLIT: u64 = fnv1a_hash_64("split");
const HASH_STARTS_WITH: u64 = fnv1a_hash_64("starts_with");
const HASH_STDERR: u64 = fnv1a_hash_64("stderr");
const HASH_SUB: u64 = fnv1a_hash_64(constants::builtins::SUB);
const HASH_TO_ARRAY: u64 = fnv1a_hash_64("to_array");
const HASH_TO_CODE: u64 = fnv1a_hash_64("to_code");
const HASH_TO_CODE_INLINE: u64 = fnv1a_hash_64("to_code_inline");
const HASH_TO_DATE: u64 = fnv1a_hash_64("to_date");
const HASH_TO_EM: u64 = fnv1a_hash_64("to_em");
const HASH_TO_H: u64 = fnv1a_hash_64("to_h");
const HASH_TO_HR: u64 = fnv1a_hash_64("to_hr");
const HASH_TO_HTML: u64 = fnv1a_hash_64("to_html");
const HASH_TO_IMAGE: u64 = fnv1a_hash_64("to_image");
const HASH_TO_LINK: u64 = fnv1a_hash_64("to_link");
const HASH_TO_MARKDOWN_STRING: u64 = fnv1a_hash_64("to_markdown_string");
const HASH_TO_MARKDOWN: u64 = fnv1a_hash_64("to_markdown");
const HASH_TO_MDX: u64 = fnv1a_hash_64("to_mdx");
const HASH_TO_MATH: u64 = fnv1a_hash_64("to_math");
const HASH_TO_MATH_INLINE: u64 = fnv1a_hash_64("to_math_inline");
const HASH_TO_MD_LIST: u64 = fnv1a_hash_64("to_md_list");
const HASH_TO_MD_NAME: u64 = fnv1a_hash_64("to_md_name");
const HASH_TO_MD_TABLE_ROW: u64 = fnv1a_hash_64("to_md_table_row");
const HASH_TO_MD_TABLE_CELL: u64 = fnv1a_hash_64("to_md_table_cell");
const HASH_TO_MD_TEXT: u64 = fnv1a_hash_64("to_md_text");
const HASH_TO_NUMBER: u64 = fnv1a_hash_64("to_number");
const HASH_TO_STRING: u64 = fnv1a_hash_64("to_string");
const HASH_TO_STRONG: u64 = fnv1a_hash_64("to_strong");
const HASH_TO_TEXT: u64 = fnv1a_hash_64("to_text");
const HASH_TRUNC: u64 = fnv1a_hash_64("trunc");
const HASH_TRIM: u64 = fnv1a_hash_64("trim");
const HASH_TYPE: u64 = fnv1a_hash_64("type");
const HASH_UNIQ: u64 = fnv1a_hash_64("uniq");
const HASH_UPDATE: u64 = fnv1a_hash_64("update");
const HASH_UPCASE: u64 = fnv1a_hash_64("upcase");
const HASH_URL_ENCODE: u64 = fnv1a_hash_64("url_encode");
const HASH_UTF8BYTELEN: u64 = fnv1a_hash_64("utf8bytelen");
const HASH_VALUES: u64 = fnv1a_hash_64("values");
const HASH_INTERN: u64 = fnv1a_hash_64("intern");
const HASH_GET_MARKDOWN_POSITION: u64 = fnv1a_hash_64("_get_markdown_position");
#[cfg(feature = "file-io")]
const HASH_READ_FILE: u64 = fnv1a_hash_64("read_file");

pub fn get_builtin_functions(name: &Ident) -> Option<&'static BuiltinFunction> {
    name.resolve_with(get_builtin_functions_by_str)
}

pub fn get_builtin_functions_by_str(name_str: &str) -> Option<&'static BuiltinFunction> {
    match fnv1a_hash_64(name_str) {
        HASH_ABS => Some(&ABS),
        HASH_ADD => Some(&ADD),
        HASH_AND => Some(&AND),
        HASH_ALL_SYMBOLS => Some(&ALL_SYMBOLS),
        HASH_ARRAY => Some(&ARRAY),
        HASH_AST_GET_ARGS => Some(&_AST_GET_ARGS),
        HASH_AST_TO_CODE => Some(&_AST_TO_CODE),
        HASH_ATTR => Some(&ATTR),
        HASH_BASE64 => Some(&BASE64),
        HASH_BASE64D => Some(&BASE64D),
        HASH_CAPTURE => Some(&CAPTURE),
        HASH_CEIL => Some(&CEIL),
        HASH_COMPACT => Some(&COMPACT),
        HASH_COALESCE => Some(&COALESCE),
        HASH_DECREASE_HEADER_LEVEL => Some(&DECREASE_HEADER_LEVEL),
        HASH_DEL => Some(&DEL),
        HASH_DICT => Some(&DICT),
        HASH_DIV => Some(&DIV),
        HASH_DOWNCASE => Some(&DOWNCASE),
        HASH_ENDS_WITH => Some(&ENDS_WITH),
        HASH_ENTRIES => Some(&ENTRIES),
        HASH_EQ => Some(&EQ),
        HASH_ERROR => Some(&ERROR),
        HASH_EXPLODE => Some(&EXPLODE),
        HASH_FLATTEN => Some(&FLATTEN),
        HASH_FLOOR => Some(&FLOOR),
        HASH_FROM_DATE => Some(&FROM_DATE),
        HASH_GET => Some(&GET),
        HASH_GT => Some(&GT),
        HASH_GTE => Some(&GTE),
        HASH_GET_TITLE => Some(&GET_TITLE),
        HASH_GET_URL => Some(&GET_URL),
        HASH_GET_VARIABLE => Some(&GET_VARIABLE),
        HASH_GSUB => Some(&GSUB),
        HASH_HALT => Some(&HALT),
        HASH_IMPLODE => Some(&IMPLODE),
        HASH_INCREASE_HEADER_LEVEL => Some(&INCREASE_HEADER_LEVEL),
        HASH_INDEX => Some(&INDEX),
        HASH_INFINITE => Some(&INFINITE),
        HASH_IS_DEBUG_MODE => Some(&IS_DEBUG_MODE),
        HASH_IS_NAN => Some(&IS_NAN),
        HASH_INSERT => Some(&INSERT),
        HASH_INPUT => Some(&INPUT),
        HASH_JOIN => Some(&JOIN),
        HASH_KEYS => Some(&KEYS),
        HASH_LEN => Some(&LEN),
        HASH_LT => Some(&LT),
        HASH_LTE => Some(&LTE),
        HASH_REGEX_MATCH => Some(&REGEX_MATCH),
        HASH_MAX => Some(&MAX),
        HASH_MIN => Some(&MIN),
        HASH_NEGATE => Some(&NEGATE),
        HASH_MOD => Some(&MOD),
        HASH_MUL => Some(&MUL),
        HASH_NE => Some(&NE),
        HASH_NOT => Some(&NOT),
        HASH_NOW => Some(&NOW),
        HASH_NAN => Some(&NAN),
        HASH_OR => Some(&OR),
        HASH_POW => Some(&POW),
        HASH_PRINT => Some(&PRINT),
        HASH_RANGE => Some(&RANGE),
        HASH_REPEAT => Some(&REPEAT),
        HASH_REPLACE => Some(&REPLACE),
        HASH_REVERSE => Some(&REVERSE),
        HASH_RINDEX => Some(&RINDEX),
        HASH_ROUND => Some(&ROUND),
        HASH_SET => Some(&SET),
        HASH_SET_ATTR => Some(&SET_ATTR),
        HASH_SET_CHECK => Some(&SET_CHECK),
        HASH_SET_CODE_BLOCK_LANG => Some(&SET_CODE_BLOCK_LANG),
        HASH_SET_LIST_ORDERED => Some(&SET_LIST_ORDERED),
        HASH_SET_REF => Some(&SET_REF),
        HASH_SET_VARIABLE => Some(&SET_VARIABLE),
        HASH_SLICE => Some(&SLICE),
        HASH_SORT => Some(&SORT),
        HASH_SORT_BY_IMPL => Some(&_SORT_BY_IMPL),
        HASH_SPLIT => Some(&SPLIT),
        HASH_STARTS_WITH => Some(&STARTS_WITH),
        HASH_STDERR => Some(&STDERR),
        HASH_SUB => Some(&SUB),
        HASH_TO_ARRAY => Some(&TO_ARRAY),
        HASH_TO_CODE => Some(&TO_CODE),
        HASH_TO_CODE_INLINE => Some(&TO_CODE_INLINE),
        HASH_TO_DATE => Some(&TO_DATE),
        HASH_TO_EM => Some(&TO_EM),
        HASH_TO_H => Some(&TO_H),
        HASH_TO_HR => Some(&TO_HR),
        HASH_TO_HTML => Some(&TO_HTML),
        HASH_TO_IMAGE => Some(&TO_IMAGE),
        HASH_TO_LINK => Some(&TO_LINK),
        HASH_TO_MARKDOWN_STRING => Some(&TO_MARKDOWN_STRING),
        HASH_TO_MARKDOWN => Some(&TO_MARKDOWN),
        HASH_TO_MDX => Some(&TO_MDX),
        HASH_TO_MATH => Some(&TO_MATH),
        HASH_TO_MATH_INLINE => Some(&TO_MATH_INLINE),
        HASH_TO_MD_LIST => Some(&TO_MD_LIST),
        HASH_TO_MD_NAME => Some(&TO_MD_NAME),
        HASH_TO_MD_TABLE_ROW => Some(&TO_MD_TABLE_ROW),
        HASH_TO_MD_TABLE_CELL => Some(&TO_MD_TABLE_CELL),
        HASH_TO_MD_TEXT => Some(&TO_MD_TEXT),
        HASH_TO_NUMBER => Some(&TO_NUMBER),
        HASH_TO_STRING => Some(&TO_STRING),
        HASH_TO_STRONG => Some(&TO_STRONG),
        HASH_TO_TEXT => Some(&TO_TEXT),
        HASH_TRUNC => Some(&TRUNC),
        HASH_TRIM => Some(&TRIM),
        HASH_TYPE => Some(&TYPE),
        HASH_UNIQ => Some(&UNIQ),
        HASH_UPDATE => Some(&UPDATE),
        HASH_UPCASE => Some(&UPCASE),
        HASH_URL_ENCODE => Some(&URL_ENCODE),
        HASH_UTF8BYTELEN => Some(&UTF8BYTELEN),
        HASH_VALUES => Some(&VALUES),
        HASH_INTERN => Some(&INTERN),
        HASH_GET_MARKDOWN_POSITION => Some(&_GET_MARKDOWN_POSITION),
        #[cfg(feature = "file-io")]
        HASH_READ_FILE => Some(&READ_FILE),
        _ => None,
    }
    // This code checks for hash collisions among built-in function names.
    // If two different function names produce the same hash, this assertion will fail.
    // This ensures that the hash-based dispatch in get_builtin_functions is safe.
    .filter(|func| func.name == name_str)
    .map(|v| &**v)
}

#[derive(Clone, Debug)]
pub struct BuiltinSelectorDoc {
    pub description: &'static str,
    pub params: &'static [&'static str],
}

pub static BUILTIN_SELECTOR_DOC: LazyLock<FxHashMap<SmolStr, BuiltinSelectorDoc>> = LazyLock::new(|| {
    let mut map = FxHashMap::with_capacity_and_hasher(100, FxBuildHasher);

    map.insert(
        SmolStr::new(".h"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the specified depth.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".text"),
        BuiltinSelectorDoc {
            description: "Selects a text node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".h1"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 1 depth.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".h2"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 2 depth.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".h3"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 3 depth.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".h4"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 4 depth.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".h5"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 5 depth.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".h6"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 6 depth.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".code"),
        BuiltinSelectorDoc {
            description: "Selects a code block node with the specified language.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".code_inline"),
        BuiltinSelectorDoc {
            description: "Selects an inline code node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".inline_math"),
        BuiltinSelectorDoc {
            description: "Selects an inline math node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".strong"),
        BuiltinSelectorDoc {
            description: "Selects a strong (bold) node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".emphasis"),
        BuiltinSelectorDoc {
            description: "Selects an emphasis (italic) node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".delete"),
        BuiltinSelectorDoc {
            description: "Selects a delete (strikethrough) node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".link"),
        BuiltinSelectorDoc {
            description: "Selects a link node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".link_ref"),
        BuiltinSelectorDoc {
            description: "Selects a link reference node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".image"),
        BuiltinSelectorDoc {
            description: "Selects an image node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".heading"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the specified depth.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".horizontal_rule"),
        BuiltinSelectorDoc {
            description: "Selects a horizontal rule node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".blockquote"),
        BuiltinSelectorDoc {
            description: "Selects a blockquote node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".[][]"),
        BuiltinSelectorDoc {
            description: "Selects a table cell node with the specified row and column.",
            params: &["row", "column"],
        },
    );

    map.insert(
        SmolStr::new(".table"),
        BuiltinSelectorDoc {
            description: "Selects a table cell node with the specified row and column.",
            params: &["row", "column"],
        },
    );

    map.insert(
        SmolStr::new(".table_align"),
        BuiltinSelectorDoc {
            description: "Selects a table align node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".html"),
        BuiltinSelectorDoc {
            description: "Selects an HTML node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".<>"),
        BuiltinSelectorDoc {
            description: "Selects an HTML node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".footnote"),
        BuiltinSelectorDoc {
            description: "Selects a footnote node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".mdx_jsx_flow_element"),
        BuiltinSelectorDoc {
            description: "Selects an MDX JSX flow element node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".list"),
        BuiltinSelectorDoc {
            description: "Selects a list node with the specified index and checked state.",
            params: &["indent", "checked"],
        },
    );

    map.insert(
        SmolStr::new(".[]"),
        BuiltinSelectorDoc {
            description: "Selects a list node with the specified index and checked state.",
            params: &["indent", "checked"],
        },
    );

    map.insert(
        SmolStr::new(".mdx_js_esm"),
        BuiltinSelectorDoc {
            description: "Selects an MDX JS ESM node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".toml"),
        BuiltinSelectorDoc {
            description: "Selects a TOML node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".yaml"),
        BuiltinSelectorDoc {
            description: "Selects a YAML node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".break"),
        BuiltinSelectorDoc {
            description: "Selects a break node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".mdx_text_expression"),
        BuiltinSelectorDoc {
            description: "Selects an MDX text expression node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".footnote_ref"),
        BuiltinSelectorDoc {
            description: "Selects a footnote reference node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".image_ref"),
        BuiltinSelectorDoc {
            description: "Selects an image reference node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".mdx_jsx_text_element"),
        BuiltinSelectorDoc {
            description: "Selects an MDX JSX text element node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".math"),
        BuiltinSelectorDoc {
            description: "Selects a math node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".math_inline"),
        BuiltinSelectorDoc {
            description: "Selects a math inline node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".mdx_flow_expression"),
        BuiltinSelectorDoc {
            description: "Selects an MDX flow expression node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".definition"),
        BuiltinSelectorDoc {
            description: "Selects a definition node.",
            params: &[],
        },
    );

    map
});

pub static INTERNAL_FUNCTION_DOC: LazyLock<FxHashMap<SmolStr, BuiltinFunctionDoc>> = LazyLock::new(|| {
    let mut map = FxHashMap::default();

    map.insert(
            SmolStr::new("_sort_by_impl"),
            BuiltinFunctionDoc{
                description: "Internal implementation of sort_by functionality that sorts arrays of arrays using the first element as the key.",
                params: &[],
            },
        );
    map.insert(
            SmolStr::new("_get_markdown_position"),
            BuiltinFunctionDoc {
            description: "Internal function to get the position information of a markdown node, returning row and column data if available.",
            params: &["markdown_node"],
            },
        );
    map.insert(
        SmolStr::new("is_debug_mode"),
        BuiltinFunctionDoc {
            description: "Checks if the runtime is currently in debug mode, returning true if a debugger is attached.",
            params: &[],
        },
    );
    map.insert(
        SmolStr::new("_ast_get_args"),
        BuiltinFunctionDoc {
            description: "Internal function to extract arguments from an AST call expression, returning an array of arguments to their AST nodes.",
            params: &["ast_node"],
        },
    );
    map.insert(
        SmolStr::new("_ast_to_code"),
        BuiltinFunctionDoc {
            description: "Internal function to convert an AST node back to its source code representation as a string.",
            params: &["ast_node"],
        },
    );
    map
});

#[derive(Clone, Debug)]
pub struct BuiltinFunctionDoc {
    pub description: &'static str,
    pub params: &'static [&'static str],
}

pub static BUILTIN_FUNCTION_DOC: LazyLock<FxHashMap<SmolStr, BuiltinFunctionDoc>> = LazyLock::new(|| {
    let mut map = FxHashMap::with_capacity_and_hasher(110, FxBuildHasher);

    map.insert(
        SmolStr::new("halt"),
        BuiltinFunctionDoc {
            description: "Terminates the program with the given exit code.",
            params: &["exit_code"],
        },
    );
    map.insert(
        SmolStr::new("error"),
        BuiltinFunctionDoc {
            description: "Raises a user-defined error with the specified message.",
            params: &["message"],
        },
    );
    map.insert(
        SmolStr::new("assert"),
        BuiltinFunctionDoc {
            description: "Asserts that two values are equal, returns the value if true, otherwise raises an error.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new("print"),
        BuiltinFunctionDoc {
            description: "Prints a message to standard output and returns the current value.",
            params: &["message"],
        },
    );
    map.insert(
        SmolStr::new("stderr"),
        BuiltinFunctionDoc {
            description: "Prints a message to standard error and returns the current value.",
            params: &["message"],
        },
    );
    map.insert(
        SmolStr::new("type"),
        BuiltinFunctionDoc {
            description: "Returns the type of the given value.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::ARRAY),
        BuiltinFunctionDoc {
            description: "Creates an array from the given values.",
            params: &["values"],
        },
    );
    map.insert(
        SmolStr::new("flatten"),
        BuiltinFunctionDoc {
            description: "Flattens a nested array into a single level array.",
            params: &["array"],
        },
    );
    map.insert(
        SmolStr::new("from_date"),
        BuiltinFunctionDoc {
            description: "Converts a date string to a timestamp.",
            params: &["date_str"],
        },
    );
    map.insert(
        SmolStr::new("to_date"),
        BuiltinFunctionDoc {
            description: "Converts a timestamp to a date string with the given format.",
            params: &["timestamp", "format"],
        },
    );
    map.insert(
        SmolStr::new("now"),
        BuiltinFunctionDoc {
            description: "Returns the current timestamp.",
            params: &[],
        },
    );
    map.insert(
        SmolStr::new("base64"),
        BuiltinFunctionDoc {
            description: "Encodes the given string to base64.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("base64d"),
        BuiltinFunctionDoc {
            description: "Decodes the given base64 string.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("min"),
        BuiltinFunctionDoc {
            description: "Returns the minimum of two values.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new("max"),
        BuiltinFunctionDoc {
            description: "Returns the maximum of two values.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new("to_html"),
        BuiltinFunctionDoc {
            description: "Converts the given markdown string to HTML.",
            params: &["markdown"],
        },
    );
    map.insert(
        SmolStr::new("to_string"),
        BuiltinFunctionDoc {
            description: "Converts the given value to a string.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_markdown_string"),
        BuiltinFunctionDoc {
            description: "Converts the given value(s) to a markdown string representation.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_number"),
        BuiltinFunctionDoc {
            description: "Converts the given value to a number.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_array"),
        BuiltinFunctionDoc {
            description: "Converts the given value to an array.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("url_encode"),
        BuiltinFunctionDoc {
            description: "URL-encodes the given string.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("to_text"),
        BuiltinFunctionDoc {
            description: "Converts the given markdown node to plain text.",
            params: &["markdown"],
        },
    );
    map.insert(
        SmolStr::new("ends_with"),
        BuiltinFunctionDoc {
            description: "Checks if the given string ends with the specified substring.",
            params: &["string", "substring"],
        },
    );
    map.insert(
        SmolStr::new("starts_with"),
        BuiltinFunctionDoc {
            description: "Checks if the given string starts with the specified substring.",
            params: &["string", "substring"],
        },
    );
    map.insert(
        SmolStr::new("regex_match"),
        BuiltinFunctionDoc {
            description: "Finds all matches of the given pattern in the string.",
            params: &["string", "pattern"],
        },
    );
    map.insert(
        SmolStr::new("downcase"),
        BuiltinFunctionDoc {
            description: "Converts the given string to lowercase.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("gsub"),
        BuiltinFunctionDoc {
            description: "Replaces all occurrences matching a regular expression pattern with the replacement string.",
            params: &["from", "pattern", "to"],
        },
    );
    map.insert(
        SmolStr::new("replace"),
        BuiltinFunctionDoc {
            description: "Replaces all occurrences of a substring with another substring.",
            params: &["from", "pattern", "to"],
        },
    );
    map.insert(
        SmolStr::new("repeat"),
        BuiltinFunctionDoc {
            description: "Repeats the given string a specified number of times.",
            params: &["string", "count"],
        },
    );
    map.insert(
        SmolStr::new("explode"),
        BuiltinFunctionDoc {
            description: "Splits the given string into an array of characters.",
            params: &["string"],
        },
    );
    map.insert(
        SmolStr::new("implode"),
        BuiltinFunctionDoc {
            description: "Joins an array of characters into a string.",
            params: &["array"],
        },
    );
    map.insert(
        SmolStr::new("trim"),
        BuiltinFunctionDoc {
            description: "Trims whitespace from both ends of the given string.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("upcase"),
        BuiltinFunctionDoc {
            description: "Converts the given string to uppercase.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::SLICE),
        BuiltinFunctionDoc {
            description: "Extracts a substring from the given string.",
            params: &["string", "start", "end"],
        },
    );
    map.insert(
        SmolStr::new("update"),
        BuiltinFunctionDoc {
            description: "Update the value with specified value.",
            params: &["target_value", "source_value"],
        },
    );
    map.insert(
        SmolStr::new("pow"),
        BuiltinFunctionDoc {
            description: "Raises the base to the power of the exponent.",
            params: &["base", "exponent"],
        },
    );
    map.insert(
        SmolStr::new("index"),
        BuiltinFunctionDoc {
            description: "Finds the first occurrence of a substring in the given string.",
            params: &["string", "substring"],
        },
    );
    map.insert(
        SmolStr::new("len"),
        BuiltinFunctionDoc {
            description: "Returns the length of the given string or array.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("rindex"),
        BuiltinFunctionDoc {
            description: "Finds the last occurrence of a substring in the given string.",
            params: &["string", "substring"],
        },
    );
    map.insert(
        SmolStr::new("join"),
        BuiltinFunctionDoc {
            description: "Joins the elements of an array into a string with the given separator.",
            params: &["array", "separator"],
        },
    );
    map.insert(
        SmolStr::new("reverse"),
        BuiltinFunctionDoc {
            description: "Reverses the given string or array.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("sort"),
        BuiltinFunctionDoc {
            description: "Sorts the elements of the given array.",
            params: &["array"],
        },
    );
    map.insert(
        SmolStr::new("compact"),
        BuiltinFunctionDoc {
            description: "Removes None values from the given array.",
            params: &["array"],
        },
    );
    map.insert(
        SmolStr::new("split"),
        BuiltinFunctionDoc {
            description: "Splits the given string by the specified separator.",
            params: &["string", "separator"],
        },
    );
    map.insert(
        SmolStr::new("uniq"),
        BuiltinFunctionDoc {
            description: "Removes duplicate elements from the given array.",
            params: &["array"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::EQ),
        BuiltinFunctionDoc {
            description: "Checks if two values are equal.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::NE),
        BuiltinFunctionDoc {
            description: "Checks if two values are not equal.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::GT),
        BuiltinFunctionDoc {
            description: "Checks if the first value is greater than the second value.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::GTE),
        BuiltinFunctionDoc {
            description: "Checks if the first value is greater than or equal to the second value.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::LT),
        BuiltinFunctionDoc {
            description: "Checks if the first value is less than the second value.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::LTE),
        BuiltinFunctionDoc {
            description: "Checks if the first value is less than or equal to the second value.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::ADD),
        BuiltinFunctionDoc {
            description: "Adds two values.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::SUB),
        BuiltinFunctionDoc {
            description: "Subtracts the second value from the first value.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::DIV),
        BuiltinFunctionDoc {
            description: "Divides the first value by the second value.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::MUL),
        BuiltinFunctionDoc {
            description: "Multiplies two values.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::MOD),
        BuiltinFunctionDoc {
            description: "Calculates the remainder of the division of the first value by the second value.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new("and"),
        BuiltinFunctionDoc {
            description: "Performs a logical AND operation on two boolean values.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new("or"),
        BuiltinFunctionDoc {
            description: "Performs a logical OR operation on two boolean values.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::NOT),
        BuiltinFunctionDoc {
            description: "Performs a logical NOT operation on a boolean value.",
            params: &["value"],
        },
    );

    map.insert(
        SmolStr::new("round"),
        BuiltinFunctionDoc {
            description: "Rounds the given number to the nearest integer.",
            params: &["number"],
        },
    );
    map.insert(
        SmolStr::new("trunc"),
        BuiltinFunctionDoc {
            description: "Truncates the given number to an integer by removing the fractional part.",
            params: &["number"],
        },
    );
    map.insert(
        SmolStr::new("ceil"),
        BuiltinFunctionDoc {
            description: "Rounds the given number up to the nearest integer.",
            params: &["number"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::FLOOR),
        BuiltinFunctionDoc {
            description: "Rounds the given number down to the nearest integer.",
            params: &["number"],
        },
    );
    map.insert(
        SmolStr::new("del"),
        BuiltinFunctionDoc {
            description: "Deletes the element at the specified index in the array or string.",
            params: &["array_or_string", "index"],
        },
    );
    map.insert(
        SmolStr::new("abs"),
        BuiltinFunctionDoc {
            description: "Returns the absolute value of the given number.",
            params: &["number"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::ATTR),
        BuiltinFunctionDoc {
            description: "Retrieves the value of the specified attribute from a markdown node.",
            params: &["markdown", "attribute"],
        },
    );
    map.insert(
        SmolStr::new("set_attr"),
        BuiltinFunctionDoc {
            description: "Sets the value of the specified attribute on a markdown node.",
            params: &["markdown", "attribute", "value"],
        },
    );
    map.insert(
        SmolStr::new("to_md_name"),
        BuiltinFunctionDoc {
            description: "Returns the name of the given markdown node.",
            params: &["markdown"],
        },
    );
    map.insert(
        SmolStr::new("set_list_ordered"),
        BuiltinFunctionDoc {
            description: "Sets the ordered property of a markdown list node.",
            params: &["list", "ordered"],
        },
    );
    map.insert(
        SmolStr::new("to_md_text"),
        BuiltinFunctionDoc {
            description: "Creates a markdown text node with the given value.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_image"),
        BuiltinFunctionDoc {
            description: "Creates a markdown image node with the given URL, alt text, and title.",
            params: &["url", "alt", "title"],
        },
    );
    map.insert(
        SmolStr::new("to_code"),
        BuiltinFunctionDoc {
            description: "Creates a markdown code block with the given value and language.",
            params: &["value", "language"],
        },
    );
    map.insert(
        SmolStr::new("to_code_inline"),
        BuiltinFunctionDoc {
            description: "Creates an inline markdown code node with the given value.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_h"),
        BuiltinFunctionDoc {
            description: "Creates a markdown heading node with the given value and depth.",
            params: &["value", "depth"],
        },
    );
    map.insert(
        SmolStr::new("to_math"),
        BuiltinFunctionDoc {
            description: "Creates a markdown math block with the given value.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_math_inline"),
        BuiltinFunctionDoc {
            description: "Creates an inline markdown math node with the given value.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_strong"),
        BuiltinFunctionDoc {
            description: "Creates a markdown strong (bold) node with the given value.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_em"),
        BuiltinFunctionDoc {
            description: "Creates a markdown emphasis (italic) node with the given value.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("to_hr"),
        BuiltinFunctionDoc {
            description: "Creates a markdown horizontal rule node.",
            params: &[],
        },
    );
    map.insert(
        SmolStr::new("to_link"),
        BuiltinFunctionDoc {
            description: "Creates a markdown link node  with the given  url and title.",
            params: &["url", "value", "title"],
        },
    );
    map.insert(
        SmolStr::new("to_md_list"),
        BuiltinFunctionDoc {
            description: "Creates a markdown list node with the given value and indent level.",
            params: &["value", "indent"],
        },
    );
    map.insert(
        SmolStr::new("to_md_table_row"),
        BuiltinFunctionDoc {
            description: "Creates a markdown table row node with the given values.",
            params: &["cells"],
        },
    );
    map.insert(
        SmolStr::new("to_md_table_cell"),
        BuiltinFunctionDoc {
            description: "Creates a markdown table cell node with the given value at the specified row and column.",
            params: &["value", "row", "column"],
        },
    );

    map.insert(
        SmolStr::new("get_title"),
        BuiltinFunctionDoc {
            description: "Returns the title of a markdown node.",
            params: &["node"],
        },
    );
    map.insert(
        SmolStr::new("get_url"),
        BuiltinFunctionDoc {
            description: "Returns the url of a markdown node.",
            params: &["node"],
        },
    );
    map.insert(
        SmolStr::new("set_check"),
        BuiltinFunctionDoc {
            description: "Creates a markdown list node with the given checked state.",
            params: &["list", "checked"],
        },
    );
    map.insert(
            SmolStr::new("set_ref"),
            BuiltinFunctionDoc {
            description: "Sets the reference identifier for markdown nodes that support references (e.g., Definition, LinkRef, ImageRef, Footnote, FootnoteRef).",
            params: &["node", "reference_id"],
            },
        );
    map.insert(
        SmolStr::new("set_code_block_lang"),
        BuiltinFunctionDoc {
            description: "Sets the language of a markdown code block node.",
            params: &["code_block", "language"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::DICT),
        BuiltinFunctionDoc {
            description: "Creates a new, empty dict.",
            params: &[],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::GET),
        BuiltinFunctionDoc {
            description: "Retrieves a value from a dict by its key. Returns None if the key is not found.",
            params: &["obj", "key"],
        },
    );
    map.insert(
            SmolStr::new("set"),
            BuiltinFunctionDoc {
                description: "Sets a key-value pair in a dict. If the key exists, its value is updated. Returns the modified map.",
                params: &["obj", "key", "value"],
            },
        );
    map.insert(
        SmolStr::new("keys"),
        BuiltinFunctionDoc {
            description: "Returns an array of keys from the dict.",
            params: &["dict"],
        },
    );
    map.insert(
        SmolStr::new("values"),
        BuiltinFunctionDoc {
            description: "Returns an array of values from the dict.",
            params: &["dict"],
        },
    );
    map.insert(
        SmolStr::new("entries"),
        BuiltinFunctionDoc {
            description: "Returns an array of key-value pairs from the dict as arrays.",
            params: &["dict"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::RANGE),
        BuiltinFunctionDoc {
            description: "Creates an array from start to end with an optional step.",
            params: &["start", "end", "step"],
        },
    );
    map.insert(
            SmolStr::new("insert"),
            BuiltinFunctionDoc {
            description: "Inserts a value into an array or string at the specified index, or into a dict with the specified key.",
            params: &["target", "index_or_key", "value"],
            },
        );
    map.insert(
        SmolStr::new("increase_header_level"),
        BuiltinFunctionDoc {
            description: "Increases the level of a markdown heading node by one, up to a maximum of 6.",
            params: &["heading_node"],
        },
    );
    map.insert(
        SmolStr::new("decrease_header_level"),
        BuiltinFunctionDoc {
            description: "Decreases the level of a markdown heading node by one, down to a minimum of 1.",
            params: &["heading_node"],
        },
    );

    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("read_file"),
        BuiltinFunctionDoc {
            description: "Reads the contents of a file at the given path and returns it as a string.",
            params: &["path"],
        },
    );
    map.insert(
        SmolStr::new("negate"),
        BuiltinFunctionDoc {
            description: "Returns the negation of the given number.",
            params: &["number"],
        },
    );
    map.insert(
        SmolStr::new("intern"),
        BuiltinFunctionDoc {
            description: "Interns the given string, returning a canonical reference for efficient comparison.",
            params: &["string"],
        },
    );
    map.insert(
        SmolStr::new("nan"),
        BuiltinFunctionDoc {
            description: "Returns a Not-a-Number (NaN) value.",
            params: &[],
        },
    );
    map.insert(
        SmolStr::new("infinite"),
        BuiltinFunctionDoc {
            description: "Returns an infinite number value.",
            params: &[],
        },
    );
    map.insert(
        SmolStr::new("coalesce"),
        BuiltinFunctionDoc {
            description: "Returns the first non-None value from the two provided arguments.",
            params: &["value1", "value2"],
        },
    );
    map.insert(
        SmolStr::new("input"),
        BuiltinFunctionDoc {
            description: "Reads a line from standard input and returns it as a string.",
            params: &[],
        },
    );
    map.insert(
        SmolStr::new("all_symbols"),
        BuiltinFunctionDoc {
            description: "Returns an array of all interned symbols.",
            params: &[],
        },
    );
    map.insert(
        SmolStr::new("to_markdown"),
        BuiltinFunctionDoc {
            description: "Parses a markdown string and returns an array of markdown nodes.",
            params: &["markdown_string"],
        },
    );
    map.insert(
        SmolStr::new("to_mdx"),
        BuiltinFunctionDoc {
            description: "Parses an MDX string and returns an array of MDX nodes.",
            params: &["mdx_string"],
        },
    );
    map.insert(
        SmolStr::new("set_variable"),
        BuiltinFunctionDoc {
            description: "Sets a symbol or variable in the current environment with the given value.",
            params: &["symbol_or_string", "value"],
        },
    );
    map.insert(
        SmolStr::new("get_variable"),
        BuiltinFunctionDoc {
            description: "Retrieves the value of a symbol or variable from the current environment.",
            params: &["symbol_or_string"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::BREAKPOINT),
        BuiltinFunctionDoc {
            description: "Sets a breakpoint for debugging; execution will pause at this point if a debugger is attached.",
            params: &[],
            },
    );
    map.insert(
        SmolStr::new("capture"),
        BuiltinFunctionDoc {
            description: "Captures groups from the given string based on the specified regular expression pattern.",
            params: &["string", "pattern"],
        },
    );

    map
});

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("")]
    InvalidBase64String(#[from] base64::DecodeError),
    #[error("")]
    NotDefined(FunctionName),
    #[error("")]
    InvalidDefinition(String),
    #[error("")]
    InvalidDateTimeFormat(String),
    #[error("")]
    InvalidTypes(FunctionName, ErrorArgs),
    #[error("")]
    InvalidNumberOfArguments(FunctionName, u8, u8),
    #[error("")]
    InvalidRegularExpression(String),
    #[error("")]
    Runtime(String),
    #[error("")]
    ZeroDivision,
    #[error("")]
    UserDefined(String),
    #[error("")]
    AssignToImmutable(String),
    #[error("")]
    UndefinedVariable(String),
}

impl From<env::EnvError> for Error {
    fn from(e: env::EnvError) -> Self {
        match e {
            env::EnvError::InvalidDefinition(name) => Error::InvalidDefinition(name),
            env::EnvError::AssignToImmutable(name) => Error::AssignToImmutable(name),
            env::EnvError::UndefinedVariable(name) => Error::UndefinedVariable(name),
        }
    }
}

impl Error {
    #[cold]
    pub fn to_runtime_error(
        &self,
        node: ast::Node,
        token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
    ) -> RuntimeError {
        match self {
            Error::UserDefined(message) => RuntimeError::UserDefined {
                message: message.to_owned(),
                token: (*get_token(token_arena, node.token_id)).clone(),
            },
            Error::InvalidBase64String(e) => {
                RuntimeError::InvalidBase64String((*get_token(token_arena, node.token_id)).clone(), e.to_string())
            }
            Error::NotDefined(name) => {
                RuntimeError::NotDefined((*get_token(token_arena, node.token_id)).clone(), name.clone())
            }
            Error::InvalidDefinition(a) => {
                RuntimeError::InvalidDefinition((*get_token(token_arena, node.token_id)).clone(), a.clone())
            }
            Error::InvalidDateTimeFormat(msg) => {
                RuntimeError::DateTimeFormatError((*get_token(token_arena, node.token_id)).clone(), msg.clone())
            }
            Error::InvalidTypes(name, args) => RuntimeError::InvalidTypes {
                token: (*get_token(token_arena, node.token_id)).clone(),
                name: name.clone(),
                args: args.iter().map(|o| format!("{:?}", o).into()).collect::<Vec<_>>(),
            },
            Error::InvalidNumberOfArguments(name, expected, got) => RuntimeError::InvalidNumberOfArguments {
                token: (*get_token(token_arena, node.token_id)).clone(),
                name: name.clone(),
                expected: *expected,
                actual: *got,
            },
            Error::InvalidRegularExpression(regex) => {
                RuntimeError::InvalidRegularExpression((*get_token(token_arena, node.token_id)).clone(), regex.clone())
            }
            Error::Runtime(msg) => RuntimeError::Runtime((*get_token(token_arena, node.token_id)).clone(), msg.clone()),
            Error::ZeroDivision => RuntimeError::ZeroDivision((*get_token(token_arena, node.token_id)).clone()),
            Error::AssignToImmutable(name) => {
                RuntimeError::AssignToImmutable((*get_token(token_arena, node.token_id)).clone(), name.clone())
            }
            Error::UndefinedVariable(name) => {
                RuntimeError::UndefinedVariable((*get_token(token_arena, node.token_id)).clone(), name.clone())
            }
        }
    }
}
#[inline(always)]
pub fn eval_builtin(
    runtime_value: &RuntimeValue,
    ident: &Ident,
    args: Args,
    env: &Shared<SharedCell<Env>>,
) -> Result<RuntimeValue, Error> {
    get_builtin_functions(ident).map_or_else(
        || Err(Error::NotDefined(ident.to_string())),
        |f| {
            let args_len = args.len() as u8;
            if f.num_params.is_valid(args_len) {
                (f.func)(ident, runtime_value, args, env)
            } else if f.num_params.is_missing_one_params(args_len) {
                let mut new_args: Args = vec![runtime_value.clone()];
                new_args.extend(args);
                (f.func)(ident, runtime_value, new_args, env)
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

pub fn eval_selector(node: &mq_markdown::Node, selector: &Selector) -> RuntimeValue {
    let is_match = match selector {
        Selector::Code => node.is_code(None),
        Selector::InlineCode => node.is_inline_code(),
        Selector::InlineMath => node.is_inline_math(),
        Selector::Strong => node.is_strong(),
        Selector::Emphasis => node.is_emphasis(),
        Selector::Delete => node.is_delete(),
        Selector::Link => node.is_link(),
        Selector::LinkRef => node.is_link_ref(),
        Selector::Image => node.is_image(),
        Selector::Heading(depth) => node.is_heading(*depth),
        Selector::HorizontalRule => node.is_horizontal_rule(),
        Selector::Blockquote => node.is_blockquote(),
        Selector::Table(row, column) => match (row, column, node.clone()) {
            (
                Some(row1),
                Some(column1),
                mq_markdown::Node::TableCell(mq_markdown::TableCell {
                    column: column2,
                    row: row2,
                    ..
                }),
            ) => *row1 == row2 && *column1 == column2,
            (Some(row1), None, mq_markdown::Node::TableCell(mq_markdown::TableCell { row: row2, .. })) => *row1 == row2,
            (None, Some(column1), mq_markdown::Node::TableCell(mq_markdown::TableCell { column: column2, .. })) => {
                *column1 == column2
            }
            (None, None, mq_markdown::Node::TableCell(_)) | (None, None, mq_markdown::Node::TableAlign(_)) => true,
            _ => false,
        },
        Selector::TableAlign => node.is_table_align(),
        Selector::Html => node.is_html(),
        Selector::Footnote => node.is_footnote(),
        Selector::MdxJsxFlowElement => node.is_mdx_jsx_flow_element(),
        Selector::List(index, checked) => match (index, node.clone()) {
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
        Selector::MdxJsEsm => node.is_mdx_js_esm(),
        Selector::Text => node.is_text(),
        Selector::Toml => node.is_toml(),
        Selector::Yaml => node.is_yaml(),
        Selector::Break => node.is_break(),
        Selector::MdxTextExpression => node.is_mdx_text_expression(),
        Selector::FootnoteRef => node.is_footnote_ref(),
        Selector::ImageRef => node.is_image_ref(),
        Selector::MdxJsxTextElement => node.is_mdx_jsx_text_element(),
        Selector::Math => node.is_math(),
        Selector::MdxFlowExpression => node.is_mdx_flow_expression(),
        Selector::Definition => node.is_definition(),
        Selector::Attr(_) => false, // Attribute selectors don't match nodes directly
        Selector::Recursive => return eval_recursive_selector(node),
    };

    if is_match {
        RuntimeValue::Markdown(node.clone(), None)
    } else {
        RuntimeValue::NONE
    }
}

fn extract_recursive_node(node: &mq_markdown::Node) -> Vec<mq_markdown::Node> {
    let mut children = vec![];

    for child in node.children().into_iter() {
        children.extend(extract_recursive_node(&child));
        children.push(child);
    }

    children
}

/// Evaluates the recursive selector and returns all descendant nodes.
fn eval_recursive_selector(node: &mq_markdown::Node) -> RuntimeValue {
    RuntimeValue::Array(
        extract_recursive_node(node)
            .into_iter()
            .map(|n| RuntimeValue::Markdown(n, None))
            .collect(),
    )
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

fn _capture_re(re: &Regex, input: &str) -> Result<RuntimeValue, Error> {
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

fn capture_re(input: &str, pattern: &str) -> Result<RuntimeValue, Error> {
    let mut cache = REGEX_CACHE.lock().unwrap();
    if let Some(re) = cache.get(pattern) {
        _capture_re(re, input)
    } else if let Ok(re) = RegexBuilder::new(pattern).size_limit(1 << 20).build() {
        cache.insert(pattern.to_string(), re.clone());
        _capture_re(&re, input)
    } else {
        Err(Error::InvalidRegularExpression(pattern.to_string()))
    }
}

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
            re.split(input).map(|s| s.to_owned().into()).collect::<Vec<_>>(),
        ))
    } else if let Ok(re) = Regex::new(pattern) {
        cache.insert(pattern.to_string(), re.clone());
        Ok(RuntimeValue::Array(
            re.split(input).map(|s| s.to_owned().into()).collect::<Vec<_>>(),
        ))
    } else {
        Err(Error::InvalidRegularExpression(pattern.to_string()))
    }
}

fn generate_numeric_range(start: isize, end: isize, step: isize) -> Result<Vec<RuntimeValue>, Error> {
    if step == 0 {
        return Err(Error::Runtime("step for range must not be zero".to_string()));
    }

    // Calculate the size of the range to prevent capacity overflow
    let range_size = if (step > 0 && end >= start) || (step < 0 && end <= start) {
        let diff = (end as i128) - (start as i128);
        let step_i128 = step as i128;
        ((diff / step_i128).abs() + 1) as usize
    } else {
        0
    };

    if range_size > MAX_RANGE_SIZE {
        return Err(Error::Runtime(format!(
            "range size {} exceeds maximum allowed size of {}",
            range_size, MAX_RANGE_SIZE
        )));
    }

    let mut result = Vec::with_capacity(range_size);
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

fn generate_char_range(start_char: char, end_char: char, step: Option<i32>) -> Result<Vec<RuntimeValue>, Error> {
    let step = step.unwrap_or(if start_char <= end_char { 1 } else { -1 });

    if step == 0 {
        return Err(Error::Runtime("step for range must not be zero".to_string()));
    }

    // Calculate the size of the range to prevent capacity overflow
    let range_size = if (step > 0 && end_char >= start_char) || (step < 0 && end_char <= start_char) {
        let diff = (end_char as i64) - (start_char as i64);
        let step_i64 = step as i64;
        ((diff / step_i64).abs() + 1) as usize
    } else {
        0
    };

    if range_size > MAX_RANGE_SIZE {
        return Err(Error::Runtime(format!(
            "range size {} exceeds maximum allowed size of {}",
            range_size, MAX_RANGE_SIZE
        )));
    }

    let mut result = Vec::with_capacity(range_size);
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

fn generate_multi_char_range(start: &str, end: &str) -> Result<Vec<RuntimeValue>, Error> {
    if start.len() != end.len() {
        return Err(Error::Runtime(
            "String range requires strings of equal length".to_string(),
        ));
    }

    let start_bytes = start.as_bytes();
    let end_bytes = end.as_bytes();

    // Calculate the approximate size of the range to prevent capacity overflow
    let capacity_estimate = (end_bytes.iter().zip(start_bytes.iter()))
        .map(|(e, s)| (e.max(s) - e.min(s)) as usize)
        .try_fold(0usize, |acc, diff| {
            // Prevent overflow during calculation
            acc.checked_add(diff)
                .ok_or_else(|| Error::Runtime(format!("range size exceeds maximum allowed size of {}", MAX_RANGE_SIZE)))
        })?
        + 1;

    if capacity_estimate > MAX_RANGE_SIZE {
        return Err(Error::Runtime(format!(
            "range size {} exceeds maximum allowed size of {}",
            capacity_estimate, MAX_RANGE_SIZE
        )));
    }

    let mut result = Vec::with_capacity(capacity_estimate);
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
                    RuntimeValue::Boolean(b) => Ok(RuntimeValue::Number(if b { 1 } else { 0 }.into())),
                    n @ RuntimeValue::Number(_) => Ok(n),
                    _ => Ok(RuntimeValue::Number(0.into())),
                })
                .collect();

            result_value.map(RuntimeValue::Array)
        }
        RuntimeValue::Boolean(true) => Ok(RuntimeValue::Number(1.into())),
        RuntimeValue::Boolean(false) => Ok(RuntimeValue::Number(0.into())),
        RuntimeValue::Number(n) => Ok(RuntimeValue::Number(*n)),
        _ => Ok(RuntimeValue::Number(0.into())),
    }
}

fn repeat(value: &mut RuntimeValue, n: usize) -> Result<RuntimeValue, Error> {
    match &*value {
        RuntimeValue::String(s) => {
            let total_size = s.len().saturating_mul(n);
            if total_size > MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "string repeat size {} exceeds maximum allowed size of {}",
                    total_size, MAX_RANGE_SIZE
                )));
            }
            Ok(s.repeat(n).into())
        }
        node @ RuntimeValue::Markdown(_, _) => {
            if let Some(md) = node.markdown_node() {
                let total_size = md.value().len().saturating_mul(n);
                if total_size > MAX_RANGE_SIZE {
                    return Err(Error::Runtime(format!(
                        "markdown repeat size {} exceeds maximum allowed size of {}",
                        total_size, MAX_RANGE_SIZE
                    )));
                }
                Ok(node.update_markdown_value(md.value().repeat(n).as_str()))
            } else {
                Ok(RuntimeValue::NONE)
            }
        }
        RuntimeValue::Array(array) => {
            if n == 0 {
                return Ok(RuntimeValue::EMPTY_ARRAY);
            }

            let total_size = array.len().saturating_mul(n);
            if total_size > MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "array repeat size {} exceeds maximum allowed size of {}",
                    total_size, MAX_RANGE_SIZE
                )));
            }

            let mut repeated_array = Vec::with_capacity(total_size);
            for _ in 0..n {
                repeated_array.extend_from_slice(array);
            }
            Ok(RuntimeValue::Array(repeated_array))
        }
        RuntimeValue::None => Ok(RuntimeValue::NONE),
        _ => Err(Error::InvalidTypes(
            constants::builtins::MUL.to_string(),
            vec![std::mem::take(value), RuntimeValue::Number(n.into())],
        )),
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
    #[case("eq", vec![RuntimeValue::String("test".into()), RuntimeValue::String("test".into())], Ok(RuntimeValue::Boolean(true)))]
    #[case("ne", vec![RuntimeValue::String("test".into()), RuntimeValue::String("different".into())], Ok(RuntimeValue::Boolean(true)))]
    fn test_eval_builtin(#[case] func_name: &str, #[case] args: Args, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new(func_name);
        assert_eq!(
            eval_builtin(
                &RuntimeValue::None,
                &ident,
                args,
                &Shared::new(SharedCell::new(Env::default()))
            ),
            expected
        );
    }

    #[rstest]
    #[case("div", vec![RuntimeValue::Number(1.0.into()), RuntimeValue::Number(0.0.into())], Error::ZeroDivision)]
    #[case("unknown_func", vec![RuntimeValue::Number(1.0.into())], Error::NotDefined("unknown_func".to_string()))]
    #[case("add", Vec::new(), Error::InvalidNumberOfArguments("add".to_string(), 2, 0))]
    #[case("add", vec![RuntimeValue::Boolean(true), RuntimeValue::Number(1.0.into())],
        Error::InvalidTypes("add".to_string(), vec![RuntimeValue::Boolean(true), RuntimeValue::Number(1.0.into())]))]
    fn test_eval_builtin_errors(#[case] func_name: &str, #[case] args: Args, #[case] expected_error: Error) {
        let ident = Ident::new(func_name);
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            args,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), expected_error);
    }

    #[test]
    fn test_implicit_first_arg() {
        let ident = Ident::new("starts_with");
        let first_arg = RuntimeValue::String("hello world".into());
        let args = vec![RuntimeValue::String("hello".into())];

        let result = eval_builtin(&first_arg, &ident, args, &Shared::new(SharedCell::new(Env::default())));
        assert_eq!(result, Ok(RuntimeValue::Boolean(true)));
    }

    #[rstest]
    #[case::code(
        Node::Code(mq_markdown::Code { value: "test".into(), lang: Some("rust".into()), fence: true, meta: None, position: None }),
        Selector::Code,
        true
    )]
    #[case::inline_code(
        Node::CodeInline(mq_markdown::CodeInline { value: "test".into(), position: None }),
        Selector::InlineCode,
        true
    )]
    #[case::inline_math(
        Node::MathInline(mq_markdown::MathInline { value: "test".into(), position: None }),
        Selector::InlineMath,
        true
    )]
    #[case::strong(
        Node::Strong(mq_markdown::Strong { values: vec!["test".to_string().into()], position: None }),
        Selector::Strong,
        true
    )]
    #[case::emphasis(
        Node::Emphasis(mq_markdown::Emphasis{ values: vec!["test".to_string().into()], position: None }),
        Selector::Emphasis,
        true
    )]
    #[case::delete(
        Node::Delete(mq_markdown::Delete{ values: vec!["test".to_string().into()], position: None }),
        Selector::Delete,
        true
    )]
    #[case::link(
        Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("https://example.com".into()), values: Vec::new(), title: None, position: None }),
        Selector::Link,
        true
    )]
    #[case::heading_matching_depth(
        Node::Heading(mq_markdown::Heading { depth: 2, values: vec!["test".to_string().into()], position: None }),
        Selector::Heading(Some(2)),
        true
    )]
    #[case::heading_wrong_depth(
        Node::Heading(mq_markdown::Heading { depth: 2, values: vec!["test".to_string().into()], position: None }),
        Selector::Heading(Some(3)),
        false
    )]
    #[case::table_cell_with_matching_row_col(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()], position: None }),
        Selector::Table(Some(1), Some(2)),
        true
    )]
    #[case::table_cell_with_wrong_row(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()], position: None }),
        Selector::Table(Some(2), Some(2)),
        false
    )]
    #[case::table_cell_with_only_row(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()], position: None }),
        Selector::Table(Some(1), None),
        true
    )]
    #[case::table_header_with_no_row_col(
        Node::TableAlign(mq_markdown::TableAlign { align: vec![], position: None }),
        Selector::Table(None, None),
        true
    )]
    #[case::table_header_with_only_row(
        Node::TableAlign(mq_markdown::TableAlign { align: vec![], position: None }),
        Selector::Table(Some(2), None),
        false
    )]
    #[case::table_header_with_only_col(
        Node::TableAlign(mq_markdown::TableAlign { align: vec![], position: None }),
        Selector::Table(None, Some(3)),
        false
    )]
    #[case::table_header_with_row_col(
        Node::TableAlign(mq_markdown::TableAlign { align: vec![], position: None }),
        Selector::Table(Some(1), Some(1)),
        false
    )]
    #[case::list_with_matching_index_checked(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(Some(1), Some(true)),
        true
    )]
    #[case::list_with_wrong_index(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(Some(2), Some(true)),
        false
    )]
    #[case::list_without_index(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(None, None),
        true
    )]
    #[case::text(
        Node::Text(mq_markdown::Text { value: "test".into(), position: None }),
        Selector::Text,
        true
    )]
    #[case::html(
        Node::Html(mq_markdown::Html { value: "<div>test</div>".into(), position: None }),
        Selector::Html,
        true
    )]
    #[case::yaml(
        Node::Yaml(mq_markdown::Yaml { value: "test".into(), position: None }),
        Selector::Yaml,
        true
    )]
    #[case::toml(
        Node::Toml(mq_markdown::Toml { value: "test".into(), position: None }),
        Selector::Toml,
        true
    )]
    #[case::break_(
        Node::Break(mq_markdown::Break{position: None}),
        Selector::Break,
        true
    )]
    #[case::image(
        Node::Image(mq_markdown::Image { alt: "".to_string(), url: "".to_string(), title: None, position: None }),
        Selector::Image,
        true
    )]
    #[case::image_ref(
        Node::ImageRef(mq_markdown::ImageRef{ alt: "".to_string(), ident: "".to_string(), label: None, position: None }),
        Selector::ImageRef,
        true
    )]
    #[case::footnote(
        Node::Footnote(mq_markdown::Footnote{ident: "".to_string(), values: vec!["test".to_string().into()], position: None}),
        Selector::Footnote,
        true
    )]
    #[case::footnote_ref(
        Node::FootnoteRef(mq_markdown::FootnoteRef{ident: "".to_string(), label: None, position: None}),
        Selector::FootnoteRef,
        true
    )]
    #[case::math(
        Node::Math(mq_markdown::Math { value: "E=mc^2".into(), position: None }),
        Selector::Math,
        true
    )]
    #[case::horizontal_rule(
        Node::HorizontalRule(mq_markdown::HorizontalRule{ position: None }),
        Selector::HorizontalRule,
        true
    )]
    #[case::blockquote(
        Node::Blockquote(mq_markdown::Blockquote{ values: vec!["test".to_string().into()], position: None }),
        Selector::Blockquote,
        true
    )]
    #[case::definition(
        Node::Definition(mq_markdown::Definition { ident: "id".to_string(), url: mq_markdown::Url::new("url".into()), label: None, title: None, position: None }),
        Selector::Definition,
        true
    )]
    #[case::mdx_jsx_flow_element(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        true
    )]
    #[case::mdx_flow_expression(
        Node::MdxFlowExpression(mq_markdown::MdxFlowExpression{ value: "value".into(), position: None }),
        Selector::MdxFlowExpression,
        true
    )]
    #[case::mdx_text_expression(
        Node::MdxTextExpression(mq_markdown::MdxTextExpression{ value: "value".into(), position: None }),
        Selector::MdxTextExpression,
        true
    )]
    #[case::mdx_js_esm(
        Node::MdxJsEsm(mq_markdown::MdxJsEsm{ value: "value".into(), position: None }),
        Selector::MdxJsEsm,
        true
    )]
    fn test_eval_selector(#[case] node: Node, #[case] selector: Selector, #[case] expected: bool) {
        assert_eq!(!eval_selector(&node, &selector).is_none(), expected);
    }

    #[test]
    fn test_eval_recursive_selector_with_children() {
        let node = Node::Heading(mq_markdown::Heading {
            values: vec![
                Node::Text(mq_markdown::Text {
                    value: "hello".into(),
                    position: None,
                }),
                Node::Link(mq_markdown::Link {
                    url: mq_markdown::Url::new("url".into()),
                    title: None,
                    values: Vec::new(),
                    position: None,
                }),
            ],
            position: None,
            depth: 1,
        });
        let result = eval_selector(&node, &Selector::Recursive);
        assert_eq!(
            result,
            RuntimeValue::Array(vec![
                RuntimeValue::Markdown(
                    Node::Text(mq_markdown::Text {
                        value: "hello".into(),
                        position: None,
                    }),
                    None
                ),
                RuntimeValue::Markdown(
                    Node::Link(mq_markdown::Link {
                        url: mq_markdown::Url::new("url".into()),
                        title: None,
                        values: Vec::new(),
                        position: None,
                    }),
                    None
                ),
            ])
        );
    }

    #[test]
    fn test_eval_recursive_selector_leaf_node() {
        let node = Node::Text(mq_markdown::Text {
            value: "leaf".into(),
            position: None,
        });
        let result = eval_selector(&node, &Selector::Recursive);
        assert_eq!(result, RuntimeValue::Array(vec![]));
    }

    #[test]
    fn test_eval_recursive_selector_nested() {
        let inner_text = Node::Text(mq_markdown::Text {
            value: "nested".into(),
            position: None,
        });
        let heading = Node::Heading(mq_markdown::Heading {
            values: vec![inner_text.clone()],
            position: None,
            depth: 2,
        });
        let node = Node::Blockquote(mq_markdown::Blockquote {
            values: vec![heading.clone()],
            position: None,
        });
        let result = eval_selector(&node, &Selector::Recursive);
        assert_eq!(
            result,
            RuntimeValue::Array(vec![
                RuntimeValue::Markdown(inner_text, None),
                RuntimeValue::Markdown(heading, None),
            ])
        );
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
    fn test_param_num_is_valid(#[case] param_num: ParamNum, #[case] num_args: u8, #[case] expected: bool) {
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
    fn test_param_num_is_missing_one_params(#[case] param_num: ParamNum, #[case] num_args: u8, #[case] expected: bool) {
        assert_eq!(param_num.is_missing_one_params(num_args), expected);
    }

    // Tests for Dict functions
    #[test]
    fn test_eval_builtin_new_dict() {
        let ident = Ident::new("dict");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![],
            &Shared::new(SharedCell::new(Env::default())),
        );
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
            &Shared::new(SharedCell::new(Env::default())),
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
        let ident_set = Ident::new("set");
        let initial_map = RuntimeValue::new_dict();

        let args1 = vec![
            initial_map.clone(),
            RuntimeValue::String("name".into()),
            RuntimeValue::String("Jules".into()),
        ];
        let result1 = eval_builtin(
            &RuntimeValue::None,
            &ident_set,
            args1,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result1.is_ok());
        let map_val1 = result1.unwrap();
        match &map_val1 {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 1);
                assert_eq!(
                    map.get(&Ident::new("name")),
                    Some(&RuntimeValue::String("Jules".into()))
                );
            }
            _ => panic!("Expected Dict, got {:?}", map_val1),
        }

        let args2 = vec![
            map_val1.clone(),
            RuntimeValue::String("age".into()),
            RuntimeValue::Number(30.into()),
        ];
        let result2 = eval_builtin(
            &RuntimeValue::None,
            &ident_set,
            args2,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result2.is_ok());
        let map_val2 = result2.unwrap();
        match &map_val2 {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(
                    map.get(&Ident::new("name")),
                    Some(&RuntimeValue::String("Jules".into()))
                );
                assert_eq!(map.get(&Ident::new("age")), Some(&RuntimeValue::Number(30.into())));
            }
            _ => panic!("Expected Dict, got {:?}", map_val2),
        }

        let args3 = vec![
            map_val2.clone(),
            RuntimeValue::String("name".into()),
            RuntimeValue::String("Vincent".into()),
        ];
        let result3 = eval_builtin(
            &RuntimeValue::None,
            &ident_set,
            args3,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result3.is_ok());
        let map_val3 = result3.unwrap();
        match &map_val3 {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(
                    map.get(&Ident::new("name")),
                    Some(&RuntimeValue::String("Vincent".into()))
                );
                assert_eq!(map.get(&Ident::new("age")), Some(&RuntimeValue::Number(30.into())));
            }
            _ => panic!("Expected Dict, got {:?}", map_val3),
        }

        let mut nested_map_data = BTreeMap::default();
        nested_map_data.insert(Ident::new("level"), RuntimeValue::Number(2.into()));
        let nested_map: RuntimeValue = nested_map_data.into();
        let args4 = vec![
            map_val3.clone(),
            RuntimeValue::String("nested".into()),
            nested_map.clone(),
        ];
        let result4 = eval_builtin(
            &RuntimeValue::None,
            &ident_set,
            args4,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result4.is_ok());
        match result4.unwrap() {
            RuntimeValue::Dict(map) => {
                assert_eq!(map.len(), 3);
                assert_eq!(map.get(&Ident::new("nested")), Some(&nested_map));
            }
            _ => panic!("Expected Dict"),
        }

        let args_err1 = vec![
            RuntimeValue::String("not_a_map".into()),
            RuntimeValue::String("key".into()),
            RuntimeValue::String("value".into()),
        ];
        let result_err1 = eval_builtin(
            &RuntimeValue::None,
            &ident_set,
            args_err1,
            &Shared::new(SharedCell::new(Env::default())),
        );
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
        let result_err2 = eval_builtin(
            &RuntimeValue::None,
            &ident_set,
            args_err2,
            &Shared::new(SharedCell::new(Env::default())),
        );
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
        let ident_get = Ident::new("get");
        let mut map_data = BTreeMap::default();
        map_data.insert("name".into(), RuntimeValue::String("Jules".into()));
        map_data.insert("age".into(), RuntimeValue::Number(30.into()));
        let map_val: RuntimeValue = map_data.into();

        let args1 = vec![map_val.clone(), RuntimeValue::String("name".into())];
        let result1 = eval_builtin(
            &RuntimeValue::None,
            &ident_get,
            args1,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result1, Ok(RuntimeValue::String("Jules".into())));

        let args2 = vec![map_val.clone(), RuntimeValue::String("location".into())];
        let result2 = eval_builtin(
            &RuntimeValue::None,
            &ident_get,
            args2,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result2, Ok(RuntimeValue::None));

        let args_err1 = vec![
            RuntimeValue::String("not_a_map".into()),
            RuntimeValue::String("key".into()),
        ];
        let result_err1 = eval_builtin(
            &RuntimeValue::None,
            &ident_get,
            args_err1,
            &Shared::new(SharedCell::new(Env::default())),
        );
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
        let result_err2 = eval_builtin(
            &RuntimeValue::None,
            &ident_get,
            args_err2,
            &Shared::new(SharedCell::new(Env::default())),
        );
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
        let ident_keys = Ident::new("keys");
        let empty_map = RuntimeValue::new_dict();
        let args1 = vec![empty_map.clone()];
        let result1 = eval_builtin(
            &RuntimeValue::None,
            &ident_keys,
            args1,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result1, Ok(RuntimeValue::Array(vec![])));

        let mut map_data = BTreeMap::default();
        map_data.insert("name".into(), RuntimeValue::String("Jules".into()));
        map_data.insert("age".into(), RuntimeValue::Number(30.into()));
        let map_val: RuntimeValue = map_data.into();
        let args2 = vec![map_val.clone()];
        let result2 = eval_builtin(
            &RuntimeValue::None,
            &ident_keys,
            args2,
            &Shared::new(SharedCell::new(Env::default())),
        );
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
                assert_eq!(keys_str, vec!["name".to_string(), "age".to_string()]);
            }
            _ => panic!("Expected Array of keys"),
        }

        let args_err1 = vec![RuntimeValue::String("not_a_map".into())];
        let result_err1 = eval_builtin(
            &RuntimeValue::None,
            &ident_keys,
            args_err1,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result_err1,
            Err(Error::InvalidTypes(
                "keys".to_string(),
                vec![RuntimeValue::String("not_a_map".into())]
            ))
        );

        let args_err2 = vec![map_val.clone(), RuntimeValue::String("extra".into())];
        let result_err2 = eval_builtin(
            &RuntimeValue::None,
            &ident_keys,
            args_err2,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result_err2,
            Err(Error::InvalidNumberOfArguments("keys".to_string(), 1, 2))
        );
    }

    #[test]
    fn test_eval_builtin_values_dict() {
        let ident_values = Ident::new("values");
        let empty_map = RuntimeValue::new_dict();
        let args1 = vec![empty_map.clone()];
        let result1 = eval_builtin(
            &RuntimeValue::None,
            &ident_values,
            args1,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result1, Ok(RuntimeValue::Array(vec![])));

        let mut map_data = BTreeMap::default();
        map_data.insert("name".into(), RuntimeValue::String("Jules".into()));
        map_data.insert("age".into(), RuntimeValue::Number(30.into()));
        let map_val: RuntimeValue = map_data.into();
        let args2 = vec![map_val.clone()];
        let result2 = eval_builtin(
            &RuntimeValue::None,
            &ident_values,
            args2,
            &Shared::new(SharedCell::new(Env::default())),
        );
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
        let result_err1 = eval_builtin(
            &RuntimeValue::None,
            &ident_values,
            args_err1,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result_err1,
            Err(Error::InvalidTypes(
                "values".to_string(),
                vec![RuntimeValue::String("not_a_map".into())]
            ))
        );

        let args_err2 = vec![map_val.clone(), RuntimeValue::String("extra".into())];
        let result_err2 = eval_builtin(
            &RuntimeValue::None,
            &ident_values,
            args_err2,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result_err2,
            Err(Error::InvalidNumberOfArguments("values".to_string(), 1, 2))
        );
    }

    #[rstest]
    #[case::excessively_large_range(0, 2_000_000, 1)]
    #[case::negative_step_large_range(10_000_000, 0, -1)]
    #[case::just_over_limit(0, 1_000_000, 1)]
    fn test_range_size_limit_exceeds(#[case] start: isize, #[case] end: isize, #[case] step: isize) {
        let result = generate_numeric_range(start, end, step);
        assert!(result.is_err());
        if let Err(Error::Runtime(msg)) = result {
            assert!(msg.contains("exceeds maximum allowed size"));
        } else {
            panic!("Expected Runtime error");
        }
    }

    #[rstest]
    #[case::reasonable_range(0, 100, 1, 101)]
    #[case::exactly_at_limit(0, 999_999, 1, 1_000_000)]
    fn test_range_size_limit_success(
        #[case] start: isize,
        #[case] end: isize,
        #[case] step: isize,
        #[case] expected_len: usize,
    ) {
        let result = generate_numeric_range(start, end, step);
        assert!(result.is_ok());
        if let Ok(vec) = result {
            assert_eq!(vec.len(), expected_len);
        }
    }

    #[rstest]
    #[case::unicode_max_range('\u{0000}', '\u{10FFFF}', Some(1))]
    fn test_char_range_size_limit_exceeds(#[case] start: char, #[case] end: char, #[case] step: Option<i32>) {
        let result = generate_char_range(start, end, step);
        assert!(result.is_err());
        if let Err(Error::Runtime(msg)) = result {
            assert!(msg.contains("exceeds maximum allowed size"));
        } else {
            panic!("Expected Runtime error");
        }
    }

    #[rstest]
    #[case::reasonable_char_range('a', 'z', None, 26)]
    fn test_char_range_size_limit_success(
        #[case] start: char,
        #[case] end: char,
        #[case] step: Option<i32>,
        #[case] expected_len: usize,
    ) {
        let result = generate_char_range(start, end, step);
        assert!(result.is_ok());
        if let Ok(vec) = result {
            assert_eq!(vec.len(), expected_len);
        }
    }

    #[rstest]
    #[case::excessively_large_array_repeat(
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        600_000,
        "array repeat size"
    )]
    #[case::just_over_limit(
        vec![RuntimeValue::Number(1.into())],
        1_000_001,
        "exceeds maximum allowed size"
    )]
    fn test_repeat_array_size_limit_exceeds(
        #[case] array: Vec<RuntimeValue>,
        #[case] n: usize,
        #[case] expected_msg: &str,
    ) {
        let mut value = RuntimeValue::Array(array);
        let result = repeat(&mut value, n);
        assert!(result.is_err());
        if let Err(Error::Runtime(msg)) = result {
            assert!(msg.contains(expected_msg));
        } else {
            panic!("Expected Runtime error for array repeat");
        }
    }

    #[rstest]
    #[case::reasonable_array_repeat(
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        10,
        20
    )]
    #[case::exactly_at_limit(
        vec![RuntimeValue::Number(1.into())],
        1_000_000,
        1_000_000
    )]
    fn test_repeat_array_size_limit_success(
        #[case] array: Vec<RuntimeValue>,
        #[case] n: usize,
        #[case] expected_len: usize,
    ) {
        let mut value = RuntimeValue::Array(array);
        let result = repeat(&mut value, n);
        assert!(result.is_ok());
        if let Ok(RuntimeValue::Array(vec)) = result {
            assert_eq!(vec.len(), expected_len);
        } else {
            panic!("Expected successful array repeat");
        }
    }

    #[rstest]
    #[case::excessively_large_string_repeat("test", 300_000, "string repeat size")]
    fn test_repeat_string_size_limit_exceeds(#[case] string: &str, #[case] n: usize, #[case] expected_msg: &str) {
        let mut value = RuntimeValue::String(string.to_string());
        let result = repeat(&mut value, n);
        assert!(result.is_err());
        if let Err(Error::Runtime(msg)) = result {
            assert!(msg.contains(expected_msg));
        } else {
            panic!("Expected Runtime error for string repeat");
        }
    }

    #[rstest]
    #[case::reasonable_string_repeat("test", 10, 40)]
    fn test_repeat_string_size_limit_success(#[case] string: &str, #[case] n: usize, #[case] expected_len: usize) {
        let mut value = RuntimeValue::String(string.to_string());
        let result = repeat(&mut value, n);
        assert!(result.is_ok());
        if let Ok(RuntimeValue::String(s)) = result {
            assert_eq!(s.len(), expected_len);
        } else {
            panic!("Expected successful string repeat");
        }
    }
}
