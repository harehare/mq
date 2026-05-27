pub(super) mod bytes;
pub(super) mod convert;
pub(super) mod date;
pub(super) mod path;
mod range;
mod regex;

use crate::arena::Arena;
use crate::ast::{constants, node as ast};
use crate::error::runtime::RuntimeError;
use crate::eval::builtin::convert::Convert;
use crate::eval::env::{self, Env};
use crate::ident::all_symbols;
use crate::number::{self};
use crate::selector::Selector;
use crate::{Ident, Shared, SharedCell, Token, get_token, parse_markdown_input, parse_mdx_input};
use base64::Engine;
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};
use csv::ReaderBuilder;
use itertools::Itertools;
use quick_xml::XmlVersion;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use similar::{ChangeTag, TextDiff};
use smol_str::SmolStr;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io;
use std::process::exit;
use std::sync::LazyLock;
use thiserror::Error;

use self::range::{generate_char_range, generate_multi_char_range, generate_numeric_range};
use self::regex::{capture_re, is_match_re, match_re, replace_re, split_re};
use super::runtime_value::{self, RuntimeValue};
use mq_markdown;

/// Maximum number of elements allowed in a generated range
pub(super) const MAX_RANGE_SIZE: usize = 1_000_000;
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
#[mq_macros::mq_fn(name = "partial", params = Range(1, u8::MAX))]
fn partial_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    if args.is_empty() {
        return Err(Error::InvalidNumberOfArguments(ident.to_string(), 1, 0));
    }
    let fn_value = args.remove(0);
    let provided = args;

    match fn_value {
        RuntimeValue::Function(params, program, fn_env) => {
            if provided.len() >= params.len() {
                return Err(Error::InvalidNumberOfArguments(
                    ident.to_string(),
                    params.len() as u8,
                    provided.len() as u8 + 1,
                ));
            }
            let partial_env = Shared::new(SharedCell::new(Env::with_parent(Shared::downgrade(&fn_env))));
            let mut remaining = crate::ast::node::Params::new();
            for (i, param) in params.iter().enumerate() {
                if i < provided.len() {
                    #[cfg(not(feature = "sync"))]
                    partial_env.borrow_mut().define(param.ident.name, provided[i].clone());
                    #[cfg(feature = "sync")]
                    partial_env
                        .write()
                        .unwrap()
                        .define(param.ident.name, provided[i].clone());
                } else {
                    remaining.push(param.clone());
                }
            }
            Ok(RuntimeValue::Function(Box::new(remaining), program, partial_env))
        }
        other => Err(Error::InvalidTypes(ident.to_string(), vec![other])),
    }
}

#[mq_macros::mq_fn(name = "halt", params = Fixed(1))]
fn halt_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(exit_code)] => exit(exit_code.value() as i32),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("halt should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "error", params = Fixed(1))]
fn error_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(message)] => Err(Error::UserDefined(message.to_string())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("error should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "print", params = Fixed(1))]
fn print_impl(_: &Ident, current_value: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => {
            println!("{}", a);
            Ok(current_value.clone())
        }
        _ => unreachable!("print should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "stderr", params = Fixed(1))]
fn stderr_impl(_: &Ident, current_value: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => {
            eprintln!("{}", a);
            Ok(current_value.clone())
        }
        _ => unreachable!("stderr should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "type", params = Fixed(1))]
fn type_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.first() {
        Some(value) => Ok(value.name().to_string().into()),
        None => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "array", params = Range(0, u8::MAX))]
fn array_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Array(args))
}

#[mq_macros::mq_fn(name = "flatten", params = Fixed(1))]
fn flatten_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(arrays)] => Ok(convert::flatten(std::mem::take(arrays)).into()),
        [a] => Ok(std::mem::take(a)),
        _ => unreachable!("flatten should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "convert", params = Fixed(2))]
fn convert_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [input, convert_value] => Convert::try_from(convert_value).map(|convert| convert.convert(input)),
        _ => unreachable!("convert should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "from_date", params = Fixed(1))]
fn from_date_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(date_str)] => convert::from_date(date_str),
        [RuntimeValue::Markdown(node_value, _)] => convert::from_date(node_value.value().as_str()),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("from_date should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "to_date", params = Fixed(2))]
fn to_date_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(ms), RuntimeValue::String(format)] => convert::to_date(*ms, Some(format.as_str())),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("to_date should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "now", params = None)]
fn now_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Number(
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| Error::Runtime(format!("{}", e)))?
            .as_secs() as i64)
            .into(),
    ))
}

/// Array format: [year, month (0-11), day (1-31), hour (0-23), minute (0-59), second (0-60), weekday (0=Sun), day-of-year (0-365)]
fn broken_down_time_array<Tz: chrono::TimeZone>(dt: &chrono::DateTime<Tz>) -> RuntimeValue {
    RuntimeValue::Array(vec![
        RuntimeValue::Number(((dt.year()) as i64).into()),
        RuntimeValue::Number((dt.month0() as i64).into()),
        RuntimeValue::Number((dt.day() as i64).into()),
        RuntimeValue::Number((dt.hour() as i64).into()),
        RuntimeValue::Number((dt.minute() as i64).into()),
        RuntimeValue::Number((dt.second() as i64).into()),
        RuntimeValue::Number((dt.weekday().num_days_from_sunday() as i64).into()),
        RuntimeValue::Number((dt.ordinal0() as i64).into()),
    ])
}

fn broken_down_time_to_naive(caller: &str, arr: &[RuntimeValue]) -> Result<chrono::NaiveDateTime, Error> {
    let get_i64 = |v: &RuntimeValue| -> Result<i64, Error> {
        match v {
            RuntimeValue::Number(n) => Ok(n.value() as i64),
            _ => Err(Error::Runtime(format!("{caller}: array elements must be numbers"))),
        }
    };
    let year = get_i64(&arr[0])? as i32;
    let month = (get_i64(&arr[1])? + 1) as u32;
    let day = get_i64(&arr[2])? as u32;
    let hour = get_i64(&arr[3])? as u32;
    let minute = get_i64(&arr[4])? as u32;
    let second = get_i64(&arr[5])? as u32;
    NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|d| d.and_hms_opt(hour, minute, second))
        .ok_or_else(|| Error::Runtime(format!("{caller}: invalid date components")))
}

/// Converts Unix timestamp (seconds) to broken-down UTC time array:
/// [year, month (0-11), day, hour, minute, second, weekday (0=Sunday), day-of-year (0-365)]
#[mq_macros::mq_fn(name = "gmtime", params = Fixed(1))]
fn gmtime_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(secs)] => {
            let secs_val = secs.value() as i64;
            DateTime::from_timestamp(secs_val, 0)
                .map(|dt| broken_down_time_array(&dt))
                .ok_or_else(|| Error::Runtime(format!("Invalid timestamp: {}", secs_val)))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("gmtime should always receive exactly one argument"),
    }
}

/// Converts Unix timestamp (seconds) to broken-down local time array:
/// [year, month (0-11), day, hour, minute, second, weekday (0=Sunday), day-of-year (0-365)]
#[mq_macros::mq_fn(name = "localtime", params = Fixed(1))]
fn localtime_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(secs)] => {
            let secs_val = secs.value() as i64;
            DateTime::from_timestamp(secs_val, 0)
                .map(|dt| broken_down_time_array(&dt.with_timezone(&Local)))
                .ok_or_else(|| Error::Runtime(format!("Invalid timestamp: {}", secs_val)))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("localtime should always receive exactly one argument"),
    }
}

/// Converts broken-down UTC time array to Unix timestamp (seconds).
/// Input format: [year, month (0-11), day, hour, minute, second, weekday, day-of-year]
#[mq_macros::mq_fn(name = "mktime", params = Fixed(1))]
fn mktime_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(arr)] if arr.len() == 8 => {
            broken_down_time_to_naive("mktime", arr).map(|dt| RuntimeValue::Number(dt.and_utc().timestamp().into()))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("mktime should always receive exactly one argument"),
    }
}

/// Formats a Unix timestamp (seconds) as a date string using the given strftime format.
#[mq_macros::mq_fn(name = "strftime", params = Fixed(2))]
fn strftime_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(secs), RuntimeValue::String(fmt)] => {
            let secs_val = secs.value() as i64;
            DateTime::from_timestamp(secs_val, 0)
                .map(|dt| RuntimeValue::String(dt.format(fmt.as_str()).to_string()))
                .ok_or_else(|| Error::Runtime(format!("strftime: invalid timestamp: {}", secs_val)))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("strftime should always receive exactly two arguments"),
    }
}

/// Adds n units to a broken-down time array and returns a new broken-down array (UTC).
/// Input/output format: [year, month (0-11), day, hour, minute, second, weekday, day-of-year]
/// Units: "seconds", "minutes", "hours", "days", "weeks", "months", "years"
/// Month/year arithmetic is calendar-aware (e.g. Jan 31 + 1 month = Feb 28/29).
#[mq_macros::mq_fn(name = "date_add", params = Fixed(3))]
fn date_add_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [
            RuntimeValue::Array(arr),
            RuntimeValue::Number(n),
            RuntimeValue::String(unit),
        ] if arr.len() == 8 => {
            let amount = n.value() as i64;
            let dt = broken_down_time_to_naive("date_add", arr)?.and_utc();
            date::add(dt, amount, unit.as_str()).map(|dt| broken_down_time_array(&dt))
        }
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!("date_add should always receive exactly three arguments"),
    }
}

/// Returns the difference (array2 - array1) in the given unit.
/// Input format: [year, month (0-11), day, hour, minute, second, weekday, day-of-year]
/// Units: "seconds", "minutes", "hours", "days", "weeks"
#[mq_macros::mq_fn(name = "date_diff", params = Fixed(3))]
fn date_diff_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [
            RuntimeValue::Array(arr1),
            RuntimeValue::Array(arr2),
            RuntimeValue::String(unit),
        ] if arr1.len() == 8 && arr2.len() == 8 => {
            let dt1 = broken_down_time_to_naive("date_diff", arr1)?.and_utc();
            let dt2 = broken_down_time_to_naive("date_diff", arr2)?.and_utc();
            let duration = dt2.signed_duration_since(dt1);
            date::diff(duration, unit.as_str()).map(|n| RuntimeValue::Number(n.into()))
        }
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!("date_diff should always receive exactly three arguments"),
    }
}

#[mq_macros::mq_fn(name = "base64", params = Fixed(1))]
fn base64_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::base64(s),
        [RuntimeValue::Bytes(b)] => convert::base64_bytes(b),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::base64(md.value().as_str()).and_then(|b| match b {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("base64 should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "base64d", params = Fixed(1))]
fn base64d_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::base64d(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::base64d(md.value().as_str()).and_then(|o| match o {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("base64d should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "base64url", params = Fixed(1))]
fn base64url_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::base64url(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::base64url(md.value().as_str()).and_then(|b| match b {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("base64url should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "base64urld", params = Fixed(1))]
fn base64urld_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::base64urld(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::base64urld(md.value().as_str()).and_then(|o| match o {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("base64urld should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "md5", params = Fixed(1))]
fn md5_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::md5(s),
        [RuntimeValue::Bytes(b)] => convert::md5_bytes(b),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::md5(md.value().as_str()).and_then(|h| match h {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => convert::md5(&a.to_string()),
        _ => unreachable!("md5 should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "sha256", params = Fixed(1))]
fn sha256_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::sha256(s),
        [RuntimeValue::Bytes(b)] => convert::sha256_bytes(b),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::sha256(md.value().as_str()).and_then(|h| match h {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => convert::sha256(&a.to_string()),
        _ => unreachable!("sha256 should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "sha512", params = Fixed(1))]
fn sha512_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::sha512(s),
        [RuntimeValue::Bytes(b)] => convert::sha512_bytes(b),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::sha512(md.value().as_str()).and_then(|h| match h {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => convert::sha512(&a.to_string()),
        _ => unreachable!("sha512 should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "from_hex", params = Fixed(1))]
fn from_hex_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::from_hex(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| convert::from_hex(md.value().as_str()))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("from_hex should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "to_hex", params = Fixed(1))]
fn to_hex_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b)] => convert::to_hex(b),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("to_hex should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "utf8", params = Fixed(1))]
fn utf8_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b)] => convert::utf8(b),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("utf8 should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "xor", params = Fixed(2))]
fn xor_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => {
            if b1.len() != b2.len() {
                return Err(Error::Runtime(format!(
                    "xor: byte slices must have the same length ({} != {})",
                    b1.len(),
                    b2.len()
                )));
            }
            Ok(RuntimeValue::Bytes(
                b1.iter().zip(b2.iter()).map(|(a, b)| a ^ b).collect(),
            ))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("xor should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "band", params = Fixed(2))]
fn band_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => {
            if b1.len() != b2.len() {
                return Err(Error::Runtime(format!(
                    "band: byte slices must have the same length ({} != {})",
                    b1.len(),
                    b2.len()
                )));
            }
            Ok(RuntimeValue::Bytes(
                b1.iter().zip(b2.iter()).map(|(a, b)| a & b).collect(),
            ))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("band should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "bor", params = Fixed(2))]
fn bor_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => {
            if b1.len() != b2.len() {
                return Err(Error::Runtime(format!(
                    "bor: byte slices must have the same length ({} != {})",
                    b1.len(),
                    b2.len()
                )));
            }
            Ok(RuntimeValue::Bytes(
                b1.iter().zip(b2.iter()).map(|(a, b)| a | b).collect(),
            ))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("bor should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "bnot", params = Fixed(1))]
fn bnot_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b)] => Ok(RuntimeValue::Bytes(b.iter().map(|x| !x).collect())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("bnot should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "pack", params = Fixed(2))]
fn pack_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(fmt), RuntimeValue::Number(n)] => bytes::pack_number(fmt, n.value()),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("pack should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "unpack", params = Fixed(2))]
fn unpack_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(fmt), RuntimeValue::Bytes(b)] => bytes::unpack_bytes(fmt, b),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("unpack should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "min", params = Fixed(2))]
fn min_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok(std::cmp::min(*n1, *n2).into()),
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(std::mem::take(std::cmp::min(s1, s2)).into()),
        [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok(std::mem::take(std::cmp::min(s1, s2)).into()),
        [RuntimeValue::None, _] | [_, RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("min should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "max", params = Fixed(2))]
fn max_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok(std::cmp::max(*n1, *n2).into()),
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(std::mem::take(std::cmp::max(s1, s2)).into()),
        [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok(std::mem::take(std::cmp::max(s1, s2)).into()),
        [RuntimeValue::None, a] | [a, RuntimeValue::None] => Ok(std::mem::take(a)),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("max should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "from_html", params = Fixed(1))]
fn from_html_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            let markdown = mq_markdown::convert_html_to_markdown(s, mq_markdown::ConversionOptions::default())
                .map_err(|e| Error::Runtime(format!("Failed to convert HTML: {}", e)))?;
            Ok(RuntimeValue::Array(parse_markdown_input(&markdown).map_err(|e| {
                Error::Runtime(format!("Failed to parse converted markdown: {}", e))
            })?))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("from_html should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "to_html", params = Fixed(1))]
fn to_html_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [a] => convert::to_html(a).map_err(|_| Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("to_html should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "to_markdown_string", params = Fixed(1))]
fn to_markdown_string_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    convert::to_markdown_string(args)
}

#[mq_macros::mq_fn(name = "to_string", params = Fixed(1))]
fn to_string_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.first() {
        Some(value) => convert::to_string(value),
        None => unreachable!("to_string should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "to_number", params = Fixed(1))]
fn to_number_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    convert::to_number(&mut args[0])
}

#[mq_macros::mq_fn(name = "to_array", params = Fixed(1))]
fn to_array_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    convert::to_array(&mut args[0])
}

#[mq_macros::mq_fn(name = "to_bytes", params = Fixed(1))]
fn to_bytes_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Bytes(std::mem::take(s).into_bytes())),
        [RuntimeValue::Bytes(b)] => Ok(RuntimeValue::Bytes(std::mem::take(b))),
        [RuntimeValue::Array(arr)] => {
            let mut bytes = Vec::with_capacity(arr.len());
            for v in arr.iter() {
                match v {
                    RuntimeValue::Number(n) => {
                        let f = n.value();
                        if !f.is_finite() || !n.is_int() || !(0.0..=255.0).contains(&f) {
                            return Err(Error::InvalidTypes(ident.to_string(), vec![v.clone()]));
                        }
                        bytes.push(f as u8);
                    }
                    other => return Err(Error::InvalidTypes(ident.to_string(), vec![other.clone()])),
                }
            }
            Ok(RuntimeValue::Bytes(bytes))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("to_bytes should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "url_encode", params = Fixed(1))]
fn url_encode_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::url_encode(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::url_encode(md.value().as_str()).and_then(|o| match o {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [a] => convert::url_encode(&a.to_string()),
        _ => unreachable!("url_encode should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "to_text", params = Fixed(1))]
fn to_text_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.first() {
        Some(value) => convert::to_text(value),
        None => unreachable!("to_text should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "ends_with", params = Fixed(2))]
fn ends_with_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, env: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(md.value().ends_with(&*s).into()))
            .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.ends_with(&*s2).into()),
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => Ok(b1.ends_with(b2).into()),
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
        _ => unreachable!("ends_with should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "starts_with", params = Fixed(2))]
fn starts_with_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, env: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(md.value().starts_with(&*s).into()))
            .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.starts_with(&*s2).into()),
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => Ok(b1.starts_with(b2).into()),
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
        _ => unreachable!("starts_with should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "regex_match", params = Fixed(2))]
fn regex_match_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        _ => unreachable!("regex_match should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "is_regex_match", params = Fixed(2))]
fn is_regex_match_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s), RuntimeValue::String(pattern)] => is_match_re(s, pattern),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(pattern)] => node
            .markdown_node()
            .map(|md| is_match_re(&md.value(), pattern))
            .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("is_regex_match should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "is_not_regex_match", params = Fixed(2))]
fn is_not_regex_match_impl(_: &Ident, _: &RuntimeValue, args: Args, env: &SharedEnv) -> Result<RuntimeValue, Error> {
    eval_builtin(
        &RuntimeValue::NONE,
        &Ident::new(constants::builtins::IS_REGEX_MATCH),
        args,
        env,
    )
    .map(|result| result.negated())
}

#[mq_macros::mq_fn(name = "capture", params = Fixed(2))]
fn capture_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        _ => unreachable!("capture should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "downcase", params = Fixed(1))]
fn downcase_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().to_lowercase().as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::String(s)] => Ok(s.to_lowercase().into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "gsub", params = Fixed(3))]
fn gsub_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        _ => unreachable!("gsub should always receive exactly three arguments"),
    }
}

#[mq_macros::mq_fn(name = "replace", params = Fixed(3))]
fn replace_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        _ => unreachable!("replace should always receive exactly three arguments"),
    }
}

#[mq_macros::mq_fn(name = "repeat", params = Fixed(2))]
fn repeat_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [v, RuntimeValue::Number(n)] => repeat(v, n.value() as usize),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("repeat should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "explode", params = Fixed(1))]
fn explode_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("explode should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "implode", params = Fixed(1))]
fn implode_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("implode should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "trim", params = Fixed(1))]
fn trim_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(s.trim().to_string().into()),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.to_string().trim())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("trim should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "ltrim", params = Fixed(1))]
fn ltrim_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(s.trim_start().to_string().into()),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.to_string().trim_start())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("ltrim should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "rtrim", params = Fixed(1))]
fn rtrim_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(s.trim_end().to_string().into()),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.to_string().trim_end())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("rtrim should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "upcase", params = Fixed(1))]
fn upcase_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().to_uppercase().as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::String(s)] => Ok(s.to_uppercase().into()),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("upcase should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "update", params = Fixed(2))]
fn update_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
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
        _ => unreachable!("update should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "slice", params = Fixed(3))]
fn slice_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
            RuntimeValue::Bytes(b),
            RuntimeValue::Number(start),
            RuntimeValue::Number(end),
        ] => {
            let len = b.len();
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
                return Ok(RuntimeValue::Bytes(vec![]));
            }
            Ok(RuntimeValue::Bytes(b[real_start..real_end].to_vec()))
        }
        [RuntimeValue::None, RuntimeValue::Number(_), RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!("slice should always receive exactly three arguments"),
    }
}

#[mq_macros::mq_fn(name = "pow", params = Fixed(2))]
fn pow_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(base), RuntimeValue::Number(exp)] => {
            if exp.is_int() && exp.value() >= 0.0 {
                Ok(RuntimeValue::Number(
                    (base.value() as i64).pow(exp.value() as u32).into(),
                ))
            } else {
                Ok(RuntimeValue::Number(base.value().powf(exp.value()).into()))
            }
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("pow should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "ln", params = Fixed(1))]
fn ln_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ln().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("ln should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "log10", params = Fixed(1))]
fn log10_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().log10().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("log10 should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "sqrt", params = Fixed(1))]
fn sqrt_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().sqrt().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("sqrt should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "exp", params = Fixed(1))]
fn exp_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().exp().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("exp should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "index", params = Fixed(2))]
fn index_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        [RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(needle)] => {
            let pos = haystack
                .windows(needle.len().max(1))
                .position(|w| w == needle.as_slice())
                .map(|i| i as i64)
                .unwrap_or(-1);
            Ok(RuntimeValue::Number(pos.into()))
        }
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
        _ => unreachable!("index should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "len", params = Fixed(1))]
fn len_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.chars().count().into())),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(RuntimeValue::Number(md.value().chars().count().into())))
            .unwrap_or_else(|| Ok(RuntimeValue::Number(0.into()))),
        [a] => Ok(RuntimeValue::Number(a.len().into())),
        _ => unreachable!("len should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "utf8bytelen", params = Fixed(1))]
fn utf8bytelen_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => Ok(RuntimeValue::Number(a.len().into())),
        _ => unreachable!("utf8bytelen should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "rindex", params = Fixed(2))]
fn rindex_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(
            s1.rfind(&*s2).map(|v| v as isize).unwrap_or_else(|| -1).into(),
        )),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| {
                Ok(RuntimeValue::Number(
                    md.value().rfind(&*s).map(|v| v as isize).unwrap_or_else(|| -1).into(),
                ))
            })
            .unwrap_or_else(|| Ok(RuntimeValue::Number((-1_i64).into()))),
        [RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(needle)] => {
            let nlen = needle.len().max(1);
            let pos = haystack
                .windows(nlen)
                .rposition(|w| w == needle.as_slice())
                .map(|i| i as i64)
                .unwrap_or(-1);
            Ok(RuntimeValue::Number(pos.into()))
        }
        [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
            .iter()
            .rposition(|o| match o {
                RuntimeValue::String(s1) => s1 == s,
                _ => false,
            })
            .map(|i| RuntimeValue::Number(i.into()))
            .unwrap_or(RuntimeValue::Number((-1_i64).into()))),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::Number((-1_i64).into())),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("rindex should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "range", params = Range(1, 3))]
fn range_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
}

#[mq_macros::mq_fn(name = "del", params = Fixed(2))]
fn del_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        _ => unreachable!("del should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "join", params = Fixed(2))]
fn join_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array.iter().join(s).into()),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("join should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "reverse", params = Fixed(1))]
fn reverse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => {
            let mut vec = std::mem::take(array);
            vec.reverse();
            Ok(RuntimeValue::Array(vec))
        }
        [RuntimeValue::String(s)] => Ok(s.chars().rev().collect::<String>().into()),
        [RuntimeValue::Bytes(b)] => {
            let mut v = std::mem::take(b);
            v.reverse();
            Ok(RuntimeValue::Bytes(v))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("reverse should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "sort", params = Fixed(1))]
fn sort_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("sort should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_sort_by_impl", params = Fixed(1))]
fn _sort_by_impl_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => {
            let mut vec = std::mem::take(array);
            vec.sort_by(|a, b| match (a, b) {
                (RuntimeValue::Array(a1), RuntimeValue::Array(a2)) => a1
                    .first()
                    .unwrap()
                    .partial_cmp(a2.first().unwrap())
                    .unwrap_or(std::cmp::Ordering::Equal),
                _ => unreachable!("_sort_by_impl should only be called with an array of arrays"),
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
                    _ => unreachable!("_sort_by_impl should only be called with an array of arrays"),
                })
                .collect();

            Ok(RuntimeValue::Array(vec))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_sort_by_impl should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "compact", params = Fixed(1))]
fn compact_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(
            std::mem::take(array)
                .into_iter()
                .filter(|v| !v.is_none())
                .collect::<Vec<_>>(),
        )),
        [a] => Ok(std::mem::take(a)),
        _ => unreachable!("compact should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "split", params = Fixed(2))]
fn split_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        _ => unreachable!("split should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "uniq", params = Fixed(1))]
fn uniq_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => {
            let mut vec = std::mem::take(array);
            let mut seen = FxHashSet::default();
            vec.retain(|item| seen.insert(item.to_string()));
            Ok(RuntimeValue::Array(vec))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("uniq should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "ceil", params = Fixed(1))]
fn ceil_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ceil().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("ceil should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "floor", params = Fixed(1))]
fn floor_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().floor().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("floor should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "round", params = Fixed(1))]
fn round_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().round().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("round should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "trunc", params = Fixed(1))]
fn trunc_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().trunc().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("trunc should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "abs", params = Fixed(1))]
fn abs_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().abs().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("abs should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "eq", params = Fixed(2))]
fn eq_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a, b] => Ok((a == b).into()),
        _ => unreachable!("eq should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "ne", params = Fixed(2))]
fn ne_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a, b] => Ok((a != b).into()),
        _ => unreachable!("ne should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "gt", params = Fixed(2))]
fn gt_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 > s2).into()),
        [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok((s1 > s2).into()),
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 > n2).into()),
        [RuntimeValue::Boolean(b1), RuntimeValue::Boolean(b2)] => Ok((b1 > b2).into()),
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => Ok((b1 > b2).into()),
        [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => Ok((n1 > n2).into()),
        [_, _] => Ok(RuntimeValue::FALSE),
        _ => unreachable!("gt should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "gte", params = Fixed(2))]
fn gte_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 >= s2).into()),
        [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok((s1 >= s2).into()),
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 >= n2).into()),
        [RuntimeValue::Boolean(b1), RuntimeValue::Boolean(b2)] => Ok((b1 >= b2).into()),
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => Ok((b1 >= b2).into()),
        [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => Ok((n1 >= n2).into()),
        [_, _] => Ok(RuntimeValue::FALSE),
        _ => unreachable!("gte should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "lt", params = Fixed(2))]
fn lt_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 < s2).into()),
        [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok((s1 < s2).into()),
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 < n2).into()),
        [RuntimeValue::Boolean(b1), RuntimeValue::Boolean(b2)] => Ok((b1 < b2).into()),
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => Ok((b1 < b2).into()),
        [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => Ok((n1 < n2).into()),
        [_, _] => Ok(RuntimeValue::FALSE),
        _ => unreachable!("lt should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "lte", params = Fixed(2))]
fn lte_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok((s1 <= s2).into()),
        [RuntimeValue::Symbol(s1), RuntimeValue::Symbol(s2)] => Ok((s1 <= s2).into()),
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((n1 <= n2).into()),
        [RuntimeValue::Boolean(b1), RuntimeValue::Boolean(b2)] => Ok((b1 <= b2).into()),
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => Ok((b1 <= b2).into()),
        [RuntimeValue::Markdown(n1, _), RuntimeValue::Markdown(n2, _)] => Ok((n1 <= n2).into()),
        [_, _] => Ok(RuntimeValue::FALSE),
        _ => unreachable!("lte should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "add", params = Fixed(2))]
fn add_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => {
            let mut result = std::mem::take(b1);
            result.extend_from_slice(b2);
            Ok(RuntimeValue::Bytes(result))
        }
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
        [RuntimeValue::Dict(d1), RuntimeValue::Dict(d2)] => {
            let mut result = std::mem::take(d1);
            result.extend(std::mem::take(d2));
            Ok(RuntimeValue::Dict(result))
        }
        [a, RuntimeValue::None] | [RuntimeValue::None, a] => Ok(std::mem::take(a)),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("add should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "sub", params = Fixed(2))]
fn sub_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 - *n2).into()),
        [a, b] => match (convert::to_number(a)?, convert::to_number(b)?) {
            (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 - n2).into()),
            _ => Err(Error::InvalidTypes(
                "Both operands could not be converted to numbers: {:?}, {:?}".to_string(),
                vec![std::mem::take(a), std::mem::take(b)],
            )),
        },
        _ => unreachable!("sub should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "div", params = Fixed(2))]
fn div_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => {
            if n2.is_zero() {
                Err(Error::ZeroDivision)
            } else {
                Ok((*n1 / *n2).into())
            }
        }
        [a, b] => match (convert::to_number(a)?, convert::to_number(b)?) {
            (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 / n2).into()),
            (RuntimeValue::None, _) | (_, RuntimeValue::None) => Ok(RuntimeValue::NONE),
            _ => Err(Error::InvalidTypes(
                "Both operands could not be converted to numbers: {:?}, {:?}".to_string(),
                vec![std::mem::take(a), std::mem::take(b)],
            )),
        },
        _ => unreachable!("div should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "mul", params = Fixed(2))]
fn mul_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 * *n2).into()),
        [RuntimeValue::Array(array), RuntimeValue::Number(n)]
        | [RuntimeValue::Number(n), RuntimeValue::Array(array)] => {
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
                            [a, b] => match (convert::to_number(a)?, convert::to_number(b)?) {
                                (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 * n2).into()),
                                (RuntimeValue::None, _) | (_, RuntimeValue::None) => Ok(RuntimeValue::NONE),
                                _ => Err(Error::InvalidTypes(
                                    constants::builtins::MUL.to_string(),
                                    vec![std::mem::take(&mut args[0]), std::mem::take(&mut args[1])],
                                )),
                            },
                            _ => unreachable!("mul should always receive exactly two arguments"),
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
        [a, b] => match (convert::to_number(a)?, convert::to_number(b)?) {
            (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 * n2).into()),
            (RuntimeValue::None, _) | (_, RuntimeValue::None) => Ok(RuntimeValue::NONE),
            _ => Ok(RuntimeValue::Number(0.into())),
        },
        _ => unreachable!("mul should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "mod", params = Fixed(2))]
fn mod_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 % *n2).into()),
        [a, b] => match (convert::to_number(a)?, convert::to_number(b)?) {
            (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 % n2).into()),
            _ => Err(Error::InvalidTypes(
                "".to_string(),
                vec![std::mem::take(a), std::mem::take(b)],
            )),
        },
        _ => unreachable!("mod should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "and", params = Range(2, u8::MAX))]
fn and_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    let mut last_truthy = None;
    for arg in args {
        if !arg.is_truthy() {
            return Ok(RuntimeValue::Boolean(false));
        }
        let mut arg = arg;
        last_truthy = Some(std::mem::take(&mut arg));
    }
    Ok(last_truthy.unwrap_or(RuntimeValue::Boolean(true)))
}

#[mq_macros::mq_fn(name = "or", params = Range(2, u8::MAX))]
fn or_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    for arg in args {
        if arg.is_truthy() {
            let mut arg = arg;
            return Ok(std::mem::take(&mut arg));
        }
    }
    Ok(RuntimeValue::Boolean(false))
}

#[mq_macros::mq_fn(name = "not", params = Fixed(1))]
fn not_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => Ok((!a.is_truthy()).into()),
        _ => unreachable!("not should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "attr", params = Fixed(2))]
fn attr_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::String(attr)] => {
            Ok(node.attr(attr).map(Into::into).unwrap_or(RuntimeValue::NONE))
        }
        [RuntimeValue::Array(nodes), RuntimeValue::String(attr)] => Ok(nodes
            .iter_mut()
            .flat_map(|node| match node {
                RuntimeValue::Markdown(node, _) => {
                    let value = node.attr(attr).map(Into::into).unwrap_or(RuntimeValue::NONE);

                    match value {
                        RuntimeValue::Array(arr) => arr,
                        RuntimeValue::None => Vec::new(),
                        v => vec![v],
                    }
                }
                a => vec![std::mem::take(a)],
            })
            .collect::<Vec<_>>()
            .into()),
        [a, ..] => Ok(std::mem::take(a)),
        _ => unreachable!("attr should always receive at least two arguments"),
    }
}

#[mq_macros::mq_fn(name = "set_attr", params = Fixed(3))]
fn set_attr_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [
            RuntimeValue::Markdown(node, selector),
            RuntimeValue::String(attr),
            value,
        ] => {
            let mut new_node = std::mem::take(node);
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
                            RuntimeValue::Markdown(new_node, selector.take()),
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
        _ => unreachable!("set_attr should always receive at least three arguments"),
    }
}

#[mq_macros::mq_fn(name = "to_code", params = Fixed(2))]
fn to_code_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a, RuntimeValue::String(lang)] => Ok(mq_markdown::Node::Code(mq_markdown::Code {
            value: a.to_string(),
            lang: Some(lang.to_string()),
            position: None,
            meta: None,
            fence: true,
        })
        .into()),
        [a, RuntimeValue::None] if !a.is_none() => Ok(mq_markdown::Node::Code(mq_markdown::Code {
            value: a.to_string(),
            lang: None,
            position: None,
            meta: None,
            fence: true,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_code_inline", params = Fixed(1))]
fn to_code_inline_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] if !a.is_none() => Ok(mq_markdown::Node::CodeInline(mq_markdown::CodeInline {
            value: a.to_string().into(),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_h", params = Fixed(2))]
fn to_h_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::Number(depth)] => {
            Ok(mq_markdown::Node::Heading(mq_markdown::Heading {
                depth: (*depth).value() as u8,
                values: node.node_values(),
                position: None,
            })
            .into())
        }
        [a, RuntimeValue::Number(depth)] => Ok(mq_markdown::Node::Heading(mq_markdown::Heading {
            depth: (*depth).value() as u8,
            values: vec![a.to_string().into()],
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_hr", params = Fixed(0))]
fn to_hr_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }).into())
}

#[mq_macros::mq_fn(name = "to_link", params = Fixed(3))]
fn to_link_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
}

#[mq_macros::mq_fn(name = "to_image", params = Fixed(3))]
fn to_image_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
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
    }
}

#[mq_macros::mq_fn(name = "to_math", params = Fixed(1))]
fn to_math_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => Ok(mq_markdown::Node::Math(mq_markdown::Math {
            value: a.to_string(),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_math_inline", params = Fixed(1))]
fn to_math_inline_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => Ok(mq_markdown::Node::MathInline(mq_markdown::MathInline {
            value: a.to_string().into(),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_md_name", params = Fixed(1))]
fn to_md_name_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Markdown(node, _)] => Ok(node.name().to_string().into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "set_list_ordered", params = Fixed(2))]
fn set_list_ordered_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::Boolean(ordered)]
            if matches!(**node, mq_markdown::Node::List(_)) =>
        {
            let ordered = *ordered;
            if let mq_markdown::Node::List(list) = &mut **node {
                Ok(mq_markdown::Node::List(mq_markdown::List {
                    ordered,
                    ..std::mem::take(list)
                })
                .into())
            } else {
                unreachable!()
            }
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_strong", params = Fixed(1))]
fn to_strong_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Markdown(node, _)] => Ok(mq_markdown::Node::Strong(mq_markdown::Strong {
            values: node.node_values(),
            position: None,
        })
        .into()),
        [a] if !a.is_none() => Ok(mq_markdown::Node::Strong(mq_markdown::Strong {
            values: vec![a.to_string().into()],
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_em", params = Fixed(1))]
fn to_em_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Markdown(node, _)] => Ok(mq_markdown::Node::Emphasis(mq_markdown::Emphasis {
            values: node.node_values(),
            position: None,
        })
        .into()),
        [a] if !a.is_none() => Ok(mq_markdown::Node::Emphasis(mq_markdown::Emphasis {
            values: vec![a.to_string().into()],
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_md_text", params = Fixed(1))]
fn to_md_text_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] if !a.is_none() => Ok(mq_markdown::Node::Text(mq_markdown::Text {
            value: a.to_string(),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_md_list", params = Fixed(2))]
fn to_md_list_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
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
        [a, RuntimeValue::Number(level)] if !a.is_none() => Ok(mq_markdown::Node::List(mq_markdown::List {
            values: vec![a.to_string().into()],
            index: 0,
            ordered: false,
            level: level.value() as u8,
            checked: None,
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_md_table_row", params = Range(1, u8::MAX))]
fn to_md_table_row_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        Box::new(mq_markdown::Node::TableRow(mq_markdown::TableRow {
            values,
            position: None,
        })),
        None,
    ))
}

#[mq_macros::mq_fn(name = "to_md_table_cell", params = Fixed(3))]
fn to_md_table_cell_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [value, RuntimeValue::Number(row), RuntimeValue::Number(column)] => Ok(RuntimeValue::Markdown(
            Box::new(mq_markdown::Node::TableCell(mq_markdown::TableCell {
                row: row.value() as usize,
                column: column.value() as usize,
                values: vec![value.to_string().into()],
                position: None,
            })),
            None,
        )),
        [a, b, c] => Err(Error::InvalidTypes(
            "table_cell".to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!("to_md_table_cell should always receive exactly three arguments"),
    }
}

#[mq_macros::mq_fn(name = "get_title", params = Fixed(1))]
fn get_title_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _)]
            if matches!(**node, mq_markdown::Node::Definition(_) | mq_markdown::Node::Link(_)) =>
        {
            match &mut **node {
                mq_markdown::Node::Definition(mq_markdown::Definition { title, .. })
                | mq_markdown::Node::Link(mq_markdown::Link { title, .. }) => std::mem::take(title)
                    .map(|t| Ok(RuntimeValue::String(t.to_value())))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                _ => unreachable!(),
            }
        }
        [RuntimeValue::Markdown(node, _)] if matches!(**node, mq_markdown::Node::Image(_)) => {
            if let mq_markdown::Node::Image(mq_markdown::Image { title, .. }) = &mut **node {
                std::mem::take(title)
                    .map(|t| Ok(RuntimeValue::String(t)))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE))
            } else {
                unreachable!()
            }
        }
        [_] => Ok(RuntimeValue::NONE),
        _ => unreachable!("get_title should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "get_url", params = Fixed(1))]
fn get_url_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Markdown(node, _)] => match &**node {
            mq_markdown::Node::Definition(def) => Ok(def.url.as_str().into()),
            mq_markdown::Node::Link(link) => Ok(link.url.as_str().into()),
            mq_markdown::Node::Image(image) => Ok(image.url.to_owned().into()),
            _ => Ok(RuntimeValue::NONE),
        },
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "set_check", params = Fixed(2))]
fn set_check_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::Boolean(checked)]
            if matches!(**node, mq_markdown::Node::List(_)) =>
        {
            let checked = *checked;
            if let mq_markdown::Node::List(list) = &mut **node {
                Ok(mq_markdown::Node::List(mq_markdown::List {
                    checked: Some(checked),
                    ..std::mem::take(list)
                })
                .into())
            } else {
                unreachable!()
            }
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "set_ref", params = Fixed(2))]
fn set_ref_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, selector), RuntimeValue::String(s)] => {
            match &mut **node {
                mq_markdown::Node::Definition(def) => {
                    return Ok(mq_markdown::Node::Definition(mq_markdown::Definition {
                        label: Some(s.to_owned()),
                        ..std::mem::take(def)
                    })
                    .into());
                }
                mq_markdown::Node::ImageRef(image_ref) => {
                    return Ok(mq_markdown::Node::ImageRef(mq_markdown::ImageRef {
                        label: if s == &image_ref.ident {
                            None
                        } else {
                            Some(s.to_owned())
                        },
                        ..std::mem::take(image_ref)
                    })
                    .into());
                }
                mq_markdown::Node::LinkRef(link_ref) => {
                    return Ok(mq_markdown::Node::LinkRef(mq_markdown::LinkRef {
                        label: if s == &link_ref.ident { None } else { Some(s.to_owned()) },
                        ..std::mem::take(link_ref)
                    })
                    .into());
                }
                mq_markdown::Node::Footnote(footnote) => {
                    return Ok(mq_markdown::Node::Footnote(mq_markdown::Footnote {
                        ident: s.to_owned(),
                        ..std::mem::take(footnote)
                    })
                    .into());
                }
                mq_markdown::Node::FootnoteRef(footnote_ref) => {
                    return Ok(mq_markdown::Node::FootnoteRef(mq_markdown::FootnoteRef {
                        label: Some(s.to_owned()),
                        ..std::mem::take(footnote_ref)
                    })
                    .into());
                }
                _ => {}
            }

            Ok(RuntimeValue::Markdown(std::mem::take(node), std::mem::take(selector)))
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "set_code_block_lang", params = Fixed(2))]
fn set_code_block_lang_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::String(lang)]
            if matches!(**node, mq_markdown::Node::Code(_)) =>
        {
            if let mq_markdown::Node::Code(code) = &mut **node {
                let lang = std::mem::take(lang);
                let mut new_code = std::mem::take(code);
                new_code.lang = if lang.is_empty() { None } else { Some(lang) };
                Ok(mq_markdown::Node::Code(new_code).into())
            } else {
                unreachable!()
            }
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "dict", params = Range(0, u8::MAX))]
fn dict_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    if args.is_empty() {
        Ok(RuntimeValue::new_dict())
    } else {
        let mut dict = BTreeMap::default();
        let entries: Cow<'_, [RuntimeValue]> = match args.as_slice() {
            [RuntimeValue::Array(entries)] => match entries.as_slice() {
                [RuntimeValue::Array(_)] if args.len() == 1 => Cow::Borrowed(entries),
                [RuntimeValue::Array(inner)] => Cow::Borrowed(inner),
                [RuntimeValue::String(_), ..] | [RuntimeValue::Symbol(_), ..] => {
                    Cow::Owned(vec![RuntimeValue::Array(entries.clone())])
                }
                _ => Cow::Borrowed(entries),
            },
            _ => Cow::Borrowed(args.as_slice()),
        };

        for entry in entries.iter() {
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
}

#[mq_macros::mq_fn(name = "get", params = Fixed(2))]
fn get_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Dict(map), RuntimeValue::String(key)] => Ok(map
            .get_mut(&Ident::new(key))
            .map(std::mem::take)
            .unwrap_or(RuntimeValue::NONE)),
        [RuntimeValue::Dict(map), RuntimeValue::Symbol(key)] => {
            Ok(map.get_mut(key).map(std::mem::take).unwrap_or(RuntimeValue::NONE))
        }
        [RuntimeValue::Array(array), RuntimeValue::Number(index)] => {
            let len = array.len();
            let idx = index.value() as isize;
            let real_idx = if idx < 0 {
                (len as isize + idx).max(0) as usize
            } else {
                idx as usize
            };
            Ok(array
                .get_mut(real_idx)
                .map(std::mem::take)
                .unwrap_or(RuntimeValue::NONE))
        }
        [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
            let len = s.chars().count();
            let idx = n.value() as isize;
            let real_idx = if idx < 0 {
                (len as isize + idx).max(0) as usize
            } else {
                idx as usize
            };
            match s.chars().nth(real_idx) {
                Some(o) => Ok(o.to_string().into()),
                None => Ok(RuntimeValue::NONE),
            }
        }
        [RuntimeValue::Markdown(node, _), RuntimeValue::Number(i)] => {
            let idx = i.value() as isize;
            let real_idx = if idx < 0 {
                let len = node.value().chars().count();
                (len as isize + idx).max(0) as usize
            } else {
                idx as usize
            };
            Ok(RuntimeValue::Markdown(
                std::mem::take(node),
                Some(runtime_value::Selector::Index(real_idx)),
            ))
        }
        [RuntimeValue::None, _] | [_, RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("get should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "set", params = Fixed(3))]
fn set_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
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
        _ => unreachable!("set should always receive exactly three arguments"),
    }
}

#[mq_macros::mq_fn(name = "keys", params = Fixed(1))]
fn keys_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Dict(map)] => {
            let keys = map
                .keys()
                .map(|k| RuntimeValue::String(k.as_str()))
                .collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(keys))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("keys should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "values", params = Fixed(1))]
fn values_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Dict(map)] => {
            let values = map.values().cloned().collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(values))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("values should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "entries", params = Fixed(1))]
fn entries_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Dict(map)] => {
            let entries = map
                .iter()
                .map(|(k, v)| RuntimeValue::Array(vec![RuntimeValue::String(k.as_str()), v.to_owned()]))
                .collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(entries))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("entries should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "insert", params = Fixed(3))]
fn insert_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
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
        _ => unreachable!("insert should always receive exactly three arguments"),
    }
}

#[mq_macros::mq_fn(name = "negate", params = Fixed(1))]
fn negate_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(-(*n))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("negate should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "intern", params = Fixed(1))]
fn intern_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(Ident::new(s).as_str())),
        [a] => Ok(RuntimeValue::String(Ident::new(&a.to_string()).as_str())),
        _ => unreachable!("intern should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "nan", params = None)]
fn nan_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Number(number::NAN))
}

#[mq_macros::mq_fn(name = "is_nan", params = Fixed(1))]
fn is_nan_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Boolean(n.is_nan())),
        [_] => Ok(RuntimeValue::FALSE),
        _ => unreachable!("is_nan should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "infinite", params = None)]
fn infinite_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Number(number::INFINITE))
}

#[mq_macros::mq_fn(name = "coalesce", params = Fixed(2))]
fn coalesce_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [a, b] => {
            if a.is_none() {
                Ok(std::mem::take(b))
            } else {
                Ok(std::mem::take(a))
            }
        }
        _ => unreachable!("coalesce should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "input", params = None)]
fn input_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| Error::Runtime(format!("Failed to read from stdin: {}", e)))?;
    input.truncate(input.trim_end_matches(&['\n', '\r'][..]).len());

    Ok(RuntimeValue::String(input))
}

#[mq_macros::mq_fn(name = "all_symbols", params = None)]
fn all_symbols_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Array(
        all_symbols()
            .into_iter()
            .map(|symbol| RuntimeValue::Symbol(Ident::new(&symbol)))
            .collect(),
    ))
}

#[mq_macros::mq_fn(name = "to_markdown", params = Fixed(1))]
fn to_markdown_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            Ok(RuntimeValue::Array(parse_markdown_input(s).map_err(|e| {
                Error::Runtime(format!("Failed to parse markdown: {}", e))
            })?))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("to_markdown should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "to_mdx", params = Fixed(1))]
fn to_mdx_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(
            parse_mdx_input(s).map_err(|e| Error::Runtime(format!("Failed to parse mdx: {}", e)))?,
        )),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("to_mdx should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_get_markdown_position", params = Fixed(1))]
fn _get_markdown_position_impl(
    ident: &Ident,
    _: &RuntimeValue,
    mut args: Args,
    _: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _)] => node
            .position()
            .map(|pos| {
                Ok(vec![
                    ("start_line".to_string(), pos.start.line.into()),
                    ("start_column".to_string(), pos.start.column.into()),
                    ("end_line".to_string(), pos.end.line.into()),
                    ("end_column".to_string(), pos.end.column.into()),
                ]
                .into())
            })
            .unwrap_or(Ok(RuntimeValue::NONE)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_get_markdown_position should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_csv_parse", params = Range(1, 3))]
fn _csv_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    let (csv_str, delimiter, has_header) = match args.as_mut_slice() {
        [RuntimeValue::String(s)] => (std::mem::take(s), b',', false),
        [RuntimeValue::String(s), RuntimeValue::String(delim)] => {
            let ch = delim
                .chars()
                .next()
                .ok_or_else(|| Error::Runtime("Delimiter must be a non-empty string".to_string()))?;
            if !ch.is_ascii() {
                return Err(Error::Runtime("Delimiter must be an ASCII character".to_string()));
            }
            (std::mem::take(s), ch as u8, false)
        }
        [
            RuntimeValue::String(s),
            RuntimeValue::String(delim),
            RuntimeValue::Boolean(b),
        ] => {
            let ch = delim
                .chars()
                .next()
                .ok_or_else(|| Error::Runtime("Delimiter must be a non-empty string".to_string()))?;
            if !ch.is_ascii() {
                return Err(Error::Runtime("Delimiter must be an ASCII character".to_string()));
            }
            (std::mem::take(s), ch as u8, *b)
        }
        [a] => return Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        [a, b] => {
            return Err(Error::InvalidTypes(
                ident.to_string(),
                vec![std::mem::take(a), std::mem::take(b)],
            ));
        }
        [a, b, c] => {
            return Err(Error::InvalidTypes(
                ident.to_string(),
                vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
            ));
        }
        _ => unreachable!("_csv_parse should receive between 1 and 3 arguments"),
    };

    let mut reader = ReaderBuilder::new()
        .has_headers(has_header)
        .delimiter(delimiter)
        .from_reader(csv_str.as_bytes());

    if has_header {
        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| Error::Runtime(format!("Failed to parse CSV headers: {e}")))?
            .iter()
            .map(|s| s.to_string())
            .collect();

        let rows: Result<Vec<RuntimeValue>, Error> = reader
            .records()
            .map(|record| {
                let record = record.map_err(|e| Error::Runtime(format!("Failed to parse CSV record: {e}")))?;
                let map: BTreeMap<Ident, RuntimeValue> = headers
                    .iter()
                    .zip(record.iter())
                    .map(|(k, v)| (Ident::new(k), RuntimeValue::String(v.to_string())))
                    .collect();
                Ok(RuntimeValue::Dict(map))
            })
            .collect();

        Ok(RuntimeValue::Array(rows?))
    } else {
        let rows: Result<Vec<RuntimeValue>, Error> = reader
            .records()
            .map(|record| {
                let record = record.map_err(|e| Error::Runtime(format!("Failed to parse CSV record: {e}")))?;
                let arr: Vec<RuntimeValue> = record.iter().map(|v| RuntimeValue::String(v.to_string())).collect();
                Ok(RuntimeValue::Array(arr))
            })
            .collect();

        Ok(RuntimeValue::Array(rows?))
    }
}

#[mq_macros::mq_fn(name = "_json_parse", params = Fixed(1))]
fn _json_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            let value: serde_json::Value =
                serde_json::from_str(s).map_err(|e| Error::Runtime(format!("Failed to parse JSON: {}", e)))?;
            Ok(value.into())
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_json_parse should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_yaml_parse", params = Fixed(1))]
fn _yaml_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            let docs = yaml_rust2::YamlLoader::load_from_str(s)
                .map_err(|e| Error::Runtime(format!("Failed to parse YAML: {}", e)))?;
            match docs.into_iter().next() {
                Some(doc) => Ok(doc.into()),
                None => Ok(RuntimeValue::NONE),
            }
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_yaml_parse should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_toon_parse", params = Fixed(1))]
fn _toon_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(toon_format::decode::<serde_json::Value>(
            s,
            &toon_format::DecodeOptions::default(),
        )
        .map_err(|e| Error::Runtime(format!("Failed to parse TOON: {}", e)))?
        .into()),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_toon_parse should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_toml_parse", params = Fixed(1))]
fn _toml_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            let value: serde_json::Value =
                toml::from_str(s).map_err(|e| Error::Runtime(format!("Failed to parse TOML: {}", e)))?;
            Ok(value.into())
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_toml_parse should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_cbor_parse", params = Fixed(1))]
fn _cbor_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(s.as_bytes())
                .map_err(|e| Error::Runtime(format!("Failed to decode base64: {}", e)))?;
            let value: ciborium::Value = ciborium::from_reader(bytes.as_slice())
                .map_err(|e| Error::Runtime(format!("Failed to parse CBOR: {}", e)))?;
            Ok(value.into())
        }
        [RuntimeValue::Bytes(b)] => {
            let value: ciborium::Value = ciborium::from_reader(b.as_slice())
                .map_err(|e| Error::Runtime(format!("Failed to parse CBOR: {}", e)))?;
            Ok(value.into())
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_cbor_parse should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_hcl_parse", params = Fixed(1))]
fn _hcl_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            let value: serde_json::Value =
                hcl::from_str(s).map_err(|e| Error::Runtime(format!("Failed to parse HCL: {}", e)))?;
            Ok(value.into())
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_hcl_parse should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_hcl_stringify", params = Fixed(1))]
fn _hcl_stringify_impl(_ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [value] => {
            let json_value = std::mem::take(value).to_json_value();
            let s =
                hcl::to_string(&json_value).map_err(|e| Error::Runtime(format!("Failed to serialize HCL: {}", e)))?;
            Ok(RuntimeValue::String(s))
        }
        _ => unreachable!("_hcl_stringify should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_cbor_stringify", params = Fixed(1))]
fn _cbor_stringify_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [value] => {
            let cbor_value = std::mem::take(value).to_cbor_value();
            let mut buf = Vec::new();
            ciborium::into_writer(&cbor_value, &mut buf)
                .map_err(|e| Error::Runtime(format!("Failed to serialize CBOR: {}", e)))?;
            Ok(RuntimeValue::Bytes(buf))
        }
        _ => unreachable!("_cbor_stringify should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_xml_parse", params = Fixed(1))]
fn _xml_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(xml_str)] => {
            let mut reader = quick_xml::Reader::from_str(xml_str);
            reader.config_mut().trim_text(true);
            let mut buf = Vec::new();
            #[allow(clippy::type_complexity)]
            let mut stack: Vec<(String, BTreeMap<Ident, RuntimeValue>, Vec<RuntimeValue>, Option<String>)> = Vec::new();
            let mut root: Option<RuntimeValue> = None;

            let parse_attrs = |e: &quick_xml::events::BytesStart<'_>, reader: &quick_xml::Reader<&[u8]>| {
                let mut attrs = BTreeMap::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| Error::Runtime(format!("XML attribute error: {}", e)))?;
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let value = attr
                        .decoded_and_normalized_value(XmlVersion::default(), reader.decoder())
                        .map_err(|e| Error::Runtime(format!("XML attribute value error: {}", e)))?
                        .to_string();
                    attrs.insert(Ident::new(&key), RuntimeValue::String(value));
                }
                Ok::<_, Error>(attrs)
            };

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(quick_xml::events::Event::Start(e)) => {
                        let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        let attrs = parse_attrs(&e, &reader)?;
                        stack.push((tag, attrs, Vec::new(), None));
                    }
                    Ok(quick_xml::events::Event::End(e)) => {
                        let end_tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        let (tag, attrs, children, text) = stack.pop().ok_or_else(|| {
                            Error::Runtime(format!(
                                "XML parse error at position {}: unexpected closing tag </{}>",
                                reader.buffer_position(),
                                end_tag
                            ))
                        })?;

                        if tag != end_tag {
                            return Err(Error::Runtime(format!(
                                "XML parse error at position {}: mismatched closing tag: expected </{}> but found </{}>",
                                reader.buffer_position(),
                                tag,
                                end_tag
                            )));
                        }

                        let mut dict = BTreeMap::new();
                        dict.insert(Ident::new("tag"), RuntimeValue::String(tag));
                        dict.insert(Ident::new("attributes"), RuntimeValue::Dict(attrs));
                        dict.insert(Ident::new("children"), RuntimeValue::Array(children));
                        dict.insert(
                            Ident::new("text"),
                            text.map(RuntimeValue::String).unwrap_or(RuntimeValue::NONE),
                        );
                        let element = RuntimeValue::Dict(dict);

                        if let Some(parent) = stack.last_mut() {
                            parent.2.push(element);
                        } else {
                            root = Some(element);
                            break;
                        }
                    }
                    Ok(quick_xml::events::Event::Empty(e)) => {
                        let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        let attrs = parse_attrs(&e, &reader)?;
                        let mut dict = BTreeMap::new();
                        dict.insert(Ident::new("tag"), RuntimeValue::String(tag));
                        dict.insert(Ident::new("attributes"), RuntimeValue::Dict(attrs));
                        dict.insert(Ident::new("children"), RuntimeValue::EMPTY_ARRAY);
                        dict.insert(Ident::new("text"), RuntimeValue::NONE);
                        let element = RuntimeValue::Dict(dict);

                        if let Some(parent) = stack.last_mut() {
                            parent.2.push(element);
                        } else {
                            root = Some(element);
                            break;
                        }
                    }
                    Ok(quick_xml::events::Event::Text(e)) => {
                        if let Some(parent) = stack.last_mut() {
                            let text = reader
                                .decoder()
                                .decode(e.as_ref())
                                .map_err(|e| Error::Runtime(format!("XML text error: {}", e)))?
                                .to_string();

                            if !text.is_empty() {
                                match &mut parent.3 {
                                    Some(t) => t.push_str(&text),
                                    None => parent.3 = Some(text),
                                }
                            }
                        }
                    }
                    Ok(quick_xml::events::Event::CData(e)) => {
                        if let Some(parent) = stack.last_mut() {
                            let text = reader
                                .decoder()
                                .decode(e.as_ref())
                                .map_err(|e| Error::Runtime(format!("XML CDATA error: {}", e)))?
                                .to_string();
                            match &mut parent.3 {
                                Some(t) => t.push_str(&text),
                                None => parent.3 = Some(text),
                            }
                        }
                    }
                    Ok(quick_xml::events::Event::Eof) => break,
                    Err(e) => {
                        return Err(Error::Runtime(format!(
                            "XML parse error at position {}: {}",
                            reader.buffer_position(),
                            e
                        )));
                    }
                    _ => (),
                }
                buf.clear();
            }

            Ok(root.unwrap_or(RuntimeValue::NONE))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_xml_parse should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "set_variable", params = Fixed(2))]
fn set_variable_impl(
    ident: &Ident,
    value: &RuntimeValue,
    mut args: Args,
    env: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
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
        _ => unreachable!("set_variable should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "get_variable", params = Fixed(1))]
fn get_variable_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, env: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
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
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("get_variable should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "is_debug_mode", params = None)]
fn is_debug_mode_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    #[cfg(feature = "debugger")]
    {
        Ok(RuntimeValue::TRUE)
    }
    #[cfg(not(feature = "debugger"))]
    {
        Ok(RuntimeValue::FALSE)
    }
}

// AST related built-ins
#[mq_macros::mq_fn(name = "_ast_get_args", params = Fixed(1))]
fn _ast_get_args_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
}

#[mq_macros::mq_fn(name = "_ast_to_code", params = Fixed(1))]
fn _ast_to_code_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Ast(ast)] => Ok(ast.to_code().into()),
        [a] => Ok(a.to_string().into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "shift_left", params = Fixed(2))]
fn shift_left_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(v), RuntimeValue::Number(n)] => v
            .to_int()
            .checked_shl(n.value() as u32)
            .map(|result| RuntimeValue::Number(result.into()))
            .ok_or_else(|| Error::Runtime("Shift amount is too large".to_string())),
        [RuntimeValue::String(v), RuntimeValue::Number(n)] => {
            let shift_amount = n.to_int().max(0) as usize;
            let shifted: String = v.chars().skip(shift_amount).collect();
            Ok(RuntimeValue::String(shifted))
        }
        [RuntimeValue::Array(arr), v] => {
            arr.push(std::mem::take(v));
            Ok(RuntimeValue::Array(std::mem::take(arr)))
        }
        [RuntimeValue::Markdown(node, selector), RuntimeValue::Number(n)] => {
            if let mq_markdown::Node::Heading(heading) = &mut **node {
                let shift_amount = n.to_int().max(0).min(u8::MAX as i64) as u8;

                heading.depth = heading.depth.saturating_sub(shift_amount).max(1);
                Ok(mq_markdown::Node::Heading(std::mem::take(heading)).into())
            } else {
                Ok(RuntimeValue::Markdown(std::mem::take(node), selector.take()))
            }
        }
        [RuntimeValue::None, _] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            constants::builtins::SHIFT_LEFT.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("shift_left should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "shift_right", params = Fixed(2))]
fn shift_right_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(v), RuntimeValue::Number(n)] => v
            .to_int()
            .checked_shr(n.value() as u32)
            .map(|result| RuntimeValue::Number(result.into()))
            .ok_or_else(|| Error::Runtime("Shift amount is too large".to_string())),
        [RuntimeValue::String(v), RuntimeValue::Number(n)] => {
            let shift_amount = n.value() as usize;
            let char_len = v.chars().count();
            if shift_amount >= char_len {
                Ok(RuntimeValue::String(String::new()))
            } else {
                let keep = char_len - shift_amount;
                let result: String = v.chars().take(keep).collect();
                Ok(RuntimeValue::String(result))
            }
        }
        [v, RuntimeValue::Array(arr)] => {
            arr.insert(0, std::mem::take(v));
            Ok(RuntimeValue::Array(std::mem::take(arr)))
        }
        [RuntimeValue::Markdown(node, selector), RuntimeValue::Number(n)] => {
            if let mq_markdown::Node::Heading(heading) = &mut **node {
                let shift_amount = n.to_int().max(0).min(u8::MAX as i64) as u8;

                if heading.depth + shift_amount <= 6 {
                    heading.depth += shift_amount;
                }
                Ok(mq_markdown::Node::Heading(std::mem::take(heading)).into())
            } else {
                Ok(RuntimeValue::Markdown(std::mem::take(node), selector.take()))
            }
        }
        [RuntimeValue::None, _] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            constants::builtins::SHIFT_RIGHT.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("shift_right should always receive exactly two arguments"),
    }
}

fn build_char_inline_diff(s1: &str, s2: &str) -> (Vec<RuntimeValue>, Vec<RuntimeValue>) {
    let char_diff = TextDiff::from_chars(s1, s2);
    let mut del_inline: Vec<RuntimeValue> = Vec::new();
    let mut ins_inline: Vec<RuntimeValue> = Vec::new();
    for c in char_diff.iter_all_changes() {
        let val = RuntimeValue::String(c.value().to_string());
        match c.tag() {
            ChangeTag::Delete => {
                let mut m = BTreeMap::new();
                m.insert(Ident::new("tag"), RuntimeValue::String("delete".into()));
                m.insert(Ident::new("value"), val);
                del_inline.push(RuntimeValue::Dict(m));
            }
            ChangeTag::Insert => {
                let mut m = BTreeMap::new();
                m.insert(Ident::new("tag"), RuntimeValue::String("insert".into()));
                m.insert(Ident::new("value"), val);
                ins_inline.push(RuntimeValue::Dict(m));
            }
            ChangeTag::Equal => {
                for inline in [&mut del_inline, &mut ins_inline] {
                    let mut m = BTreeMap::new();
                    m.insert(Ident::new("tag"), RuntimeValue::String("equal".into()));
                    m.insert(Ident::new("value"), RuntimeValue::String(c.value().to_string()));
                    inline.push(RuntimeValue::Dict(m));
                }
            }
        }
    }
    (del_inline, ins_inline)
}

#[mq_macros::mq_fn(name = "_diff", params = Fixed(2))]
fn _diff_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(a1), RuntimeValue::Array(a2)] => {
            let a1_debug: Vec<String> = a1.iter().map(|v| format!("{:?}", v)).collect();
            let a2_debug: Vec<String> = a2.iter().map(|v| format!("{:?}", v)).collect();
            let a1_slices: Vec<&str> = a1_debug.iter().map(|s| s.as_str()).collect();
            let a2_slices: Vec<&str> = a2_debug.iter().map(|s| s.as_str()).collect();
            let diff = TextDiff::from_slices(&a1_slices, &a2_slices);
            let changes: Vec<_> = diff.iter_all_changes().collect();
            let mut result = Vec::new();
            let mut i = 0;
            while i < changes.len() {
                if changes[i].tag() == ChangeTag::Delete
                    && i + 1 < changes.len()
                    && changes[i + 1].tag() == ChangeTag::Insert
                {
                    let old_idx = changes[i].old_index().unwrap();
                    let new_idx = changes[i + 1].new_index().unwrap();
                    let old_val = &a1[old_idx];
                    let new_val = &a2[new_idx];
                    if let (RuntimeValue::String(s1), RuntimeValue::String(s2)) = (old_val, new_val) {
                        let (del_inline, ins_inline) = build_char_inline_diff(s1.as_str(), s2.as_str());
                        let mut del_map = BTreeMap::new();
                        del_map.insert(Ident::new("tag"), RuntimeValue::String("delete".into()));
                        del_map.insert(Ident::new("value"), old_val.clone());
                        del_map.insert(Ident::new("inline"), RuntimeValue::Array(del_inline));
                        result.push(RuntimeValue::Dict(del_map));
                        let mut ins_map = BTreeMap::new();
                        ins_map.insert(Ident::new("tag"), RuntimeValue::String("insert".into()));
                        ins_map.insert(Ident::new("value"), new_val.clone());
                        ins_map.insert(Ident::new("inline"), RuntimeValue::Array(ins_inline));
                        result.push(RuntimeValue::Dict(ins_map));
                    } else {
                        let mut del_map = BTreeMap::new();
                        del_map.insert(Ident::new("tag"), RuntimeValue::String("delete".into()));
                        del_map.insert(Ident::new("value"), old_val.clone());
                        result.push(RuntimeValue::Dict(del_map));
                        let mut ins_map = BTreeMap::new();
                        ins_map.insert(Ident::new("tag"), RuntimeValue::String("insert".into()));
                        ins_map.insert(Ident::new("value"), new_val.clone());
                        result.push(RuntimeValue::Dict(ins_map));
                    }
                    i += 2;
                } else {
                    let tag_str = match changes[i].tag() {
                        ChangeTag::Equal => "equal",
                        ChangeTag::Delete => "delete",
                        ChangeTag::Insert => "insert",
                    };
                    let value = match changes[i].tag() {
                        ChangeTag::Equal | ChangeTag::Delete => a1[changes[i].old_index().unwrap()].clone(),
                        ChangeTag::Insert => a2[changes[i].new_index().unwrap()].clone(),
                    };
                    let mut map = BTreeMap::new();
                    map.insert(Ident::new("tag"), RuntimeValue::String(tag_str.into()));
                    map.insert(Ident::new("value"), value);
                    result.push(RuntimeValue::Dict(map));
                    i += 1;
                }
            }
            Ok(RuntimeValue::Array(result))
        }
        [a1, a2] => {
            let s1 = a1.to_string();
            let s2 = a2.to_string();
            let line_diff = TextDiff::from_lines(&s1, &s2);
            let changes: Vec<_> = line_diff.iter_all_changes().collect();
            let mut result = Vec::new();
            let mut i = 0;
            while i < changes.len() {
                if changes[i].tag() == ChangeTag::Delete
                    && i + 1 < changes.len()
                    && changes[i + 1].tag() == ChangeTag::Insert
                {
                    let old_val = changes[i].value().trim_end_matches('\n');
                    let new_val = changes[i + 1].value().trim_end_matches('\n');
                    let (del_inline, ins_inline) = build_char_inline_diff(old_val, new_val);
                    let mut del_map = BTreeMap::new();
                    del_map.insert(Ident::new("tag"), RuntimeValue::String("delete".into()));
                    del_map.insert(Ident::new("value"), RuntimeValue::String(old_val.to_string()));
                    del_map.insert(Ident::new("inline"), RuntimeValue::Array(del_inline));
                    result.push(RuntimeValue::Dict(del_map));
                    let mut ins_map = BTreeMap::new();
                    ins_map.insert(Ident::new("tag"), RuntimeValue::String("insert".into()));
                    ins_map.insert(Ident::new("value"), RuntimeValue::String(new_val.to_string()));
                    ins_map.insert(Ident::new("inline"), RuntimeValue::Array(ins_inline));
                    result.push(RuntimeValue::Dict(ins_map));
                    i += 2;
                } else {
                    let tag_str = match changes[i].tag() {
                        ChangeTag::Equal => "equal",
                        ChangeTag::Delete => "delete",
                        ChangeTag::Insert => "insert",
                    };
                    let val = changes[i].value().trim_end_matches('\n').to_string();
                    let mut map = BTreeMap::new();
                    map.insert(Ident::new("tag"), RuntimeValue::String(tag_str.into()));
                    map.insert(Ident::new("value"), RuntimeValue::String(val));
                    result.push(RuntimeValue::Dict(map));
                    i += 1;
                }
            }
            Ok(RuntimeValue::Array(result))
        }
        _ => unreachable!("_diff should receive exactly two arguments, both arrays or both non-arrays"),
    }
}

#[mq_macros::mq_fn(name = "basename", params = Fixed(1))]
fn basename_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(path::basename(s))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("basename should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "dirname", params = Fixed(1))]
fn dirname_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(path::dirname(s))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("dirname should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "extname", params = Fixed(1))]
fn extname_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(path::extname(s))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("extname should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "stem", params = Fixed(1))]
fn stem_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(path::stem(s))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("stem should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "path_join", params = Fixed(2))]
fn path_join_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(base), RuntimeValue::String(component)] => {
            path::path_join(base, component).map(RuntimeValue::String)
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("path_join should always receive exactly two arguments"),
    }
}

#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "read_file", params = Fixed(1))]
fn read_file_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => match std::fs::read_to_string(&path) {
            Ok(content) => Ok(RuntimeValue::String(content)),
            Err(e) => Err(Error::Runtime(format!("Failed to read file {}: {}", path, e))),
        },
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("read_file should always receive exactly one argument"),
    }
}

#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "file_exists", params = Fixed(1))]
fn file_exists_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => Ok(std::path::Path::new(path).exists().into()),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("file_exists should always receive exactly one argument"),
    }
}

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

pub fn get_builtin_functions(name: &Ident) -> Option<&'static BuiltinFunction> {
    name.resolve_with(get_builtin_functions_by_str)
}

mq_macros::builtin_dispatch! {
    PARTIAL,
    HALT,
    ERROR,
    PRINT,
    STDERR,
    TYPE,
    ARRAY,
    FLATTEN,
    CONVERT,
    FROM_DATE,
    TO_DATE,
    NOW,
    GMTIME,
    LOCALTIME,
    MKTIME,
    STRFTIME,
    DATE_ADD,
    DATE_DIFF,
    BASE64,
    BASE64D,
    BASE64URL,
    BASE64URLD,
    MD5,
    SHA256,
    SHA512,
    MIN,
    MAX,
    FROM_HTML,
    TO_HTML,
    TO_MARKDOWN_STRING,
    TO_STRING,
    TO_NUMBER,
    TO_ARRAY,
    TO_BYTES,
    FROM_HEX,
    TO_HEX,
    UTF8,
    XOR,
    BAND,
    BOR,
    BNOT,
    PACK,
    UNPACK,
    URL_ENCODE,
    TO_TEXT,
    ENDS_WITH,
    STARTS_WITH,
    REGEX_MATCH,
    IS_REGEX_MATCH,
    IS_NOT_REGEX_MATCH,
    CAPTURE,
    DOWNCASE,
    GSUB,
    REPLACE,
    REPEAT,
    EXPLODE,
    IMPLODE,
    TRIM,
    LTRIM,
    RTRIM,
    UPCASE,
    UPDATE,
    SLICE,
    POW,
    LN,
    LOG10,
    SQRT,
    EXP,
    INDEX,
    LEN,
    UTF8BYTELEN,
    RINDEX,
    RANGE,
    DEL,
    JOIN,
    REVERSE,
    SORT,
    _SORT_BY_IMPL,
    COMPACT,
    SPLIT,
    UNIQ,
    CEIL,
    FLOOR,
    ROUND,
    TRUNC,
    ABS,
    EQ,
    NE,
    GT,
    GTE,
    LT,
    LTE,
    ADD,
    SUB,
    DIV,
    MUL,
    MOD,
    AND,
    OR,
    NOT,
    ATTR,
    SET_ATTR,
    TO_CODE,
    TO_CODE_INLINE,
    TO_H,
    TO_HR,
    TO_LINK,
    TO_IMAGE,
    TO_MATH,
    TO_MATH_INLINE,
    TO_MD_NAME,
    SET_LIST_ORDERED,
    TO_STRONG,
    TO_EM,
    TO_MD_TEXT,
    TO_MD_LIST,
    TO_MD_TABLE_ROW,
    TO_MD_TABLE_CELL,
    GET_TITLE,
    GET_URL,
    SET_CHECK,
    SET_REF,
    SET_CODE_BLOCK_LANG,
    DICT,
    GET,
    SET,
    KEYS,
    VALUES,
    ENTRIES,
    INSERT,
    NEGATE,
    INTERN,
    NAN,
    IS_NAN,
    INFINITE,
    COALESCE,
    INPUT,
    ALL_SYMBOLS,
    TO_MARKDOWN,
    TO_MDX,
    _GET_MARKDOWN_POSITION,
    _CSV_PARSE,
    _JSON_PARSE,
    _YAML_PARSE,
    _TOON_PARSE,
    _TOML_PARSE,
    _CBOR_PARSE,
    _CBOR_STRINGIFY,
    _HCL_PARSE,
    _HCL_STRINGIFY,
    _XML_PARSE,
    SET_VARIABLE,
    GET_VARIABLE,
    IS_DEBUG_MODE,
    _AST_GET_ARGS,
    _AST_TO_CODE,
    SHIFT_LEFT,
    SHIFT_RIGHT,
    _DIFF,
    BASENAME,
    DIRNAME,
    EXTNAME,
    STEM,
    PATH_JOIN,
    #[cfg(feature = "file-io")]
    READ_FILE,
    #[cfg(feature = "file-io")]
    FILE_EXISTS,
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

    map.insert(
        SmolStr::new(".task"),
        BuiltinSelectorDoc {
            description: "Selects a task list node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".todo"),
        BuiltinSelectorDoc {
            description: "Selects a todo item in the task list node.",
            params: &[],
        },
    );

    map.insert(
        SmolStr::new(".done"),
        BuiltinSelectorDoc {
            description: "Selects a done item in the task list node.",
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
    map.insert(
        SmolStr::new("_csv_parse"),
        BuiltinFunctionDoc {
            description: "Parses a CSV string into an array of arrays, using the specified delimiter and header options.",
            params: &["csv_string", "delimiter", "has_header"],
        },
    );
    map.insert(
        SmolStr::new("_xml_parse"),
        BuiltinFunctionDoc {
            description: "Parses an XML string and returns the corresponding data structure.",
            params: &["xml_string"],
        },
    );
    map.insert(
        SmolStr::new("_json_parse"),
        BuiltinFunctionDoc {
            description: "Parses a JSON string into a data structure.",
            params: &["json_string"],
        },
    );
    map.insert(
        SmolStr::new("_yaml_parse"),
        BuiltinFunctionDoc {
            description: "Parses a YAML string into a data structure.",
            params: &["yaml_string"],
        },
    );
    map.insert(
        SmolStr::new("_toon_parse"),
        BuiltinFunctionDoc {
            description: "Parses a TOON string into a data structure.",
            params: &["toon_string"],
        },
    );
    map.insert(
        SmolStr::new("_toml_parse"),
        BuiltinFunctionDoc {
            description: "Parses a TOML string into a data structure.",
            params: &["toml_string"],
        },
    );
    map.insert(
        SmolStr::new("_cbor_parse"),
        BuiltinFunctionDoc {
            description: "Parses a base64-encoded CBOR string or raw bytes into a data structure.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("_cbor_stringify"),
        BuiltinFunctionDoc {
            description: "Serializes a value to CBOR bytes.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("_hcl_parse"),
        BuiltinFunctionDoc {
            description: "Parses an HCL string into a data structure.",
            params: &["hcl_string"],
        },
    );
    map.insert(
        SmolStr::new("_hcl_stringify"),
        BuiltinFunctionDoc {
            description: "Serializes a value to an HCL string.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("_diff"),
        BuiltinFunctionDoc {
            description: "Internal function to compute the difference between two values, returning an array of changes.",
            params: &["value1", "value2"],
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
        SmolStr::new("exp"),
        BuiltinFunctionDoc {
            description: "Returns the exponential (e^x) of the given number.",
            params: &["number"],
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
        SmolStr::new("ln"),
        BuiltinFunctionDoc {
            description: "Returns the natural logarithm (base e) of the given number.",
            params: &["number"],
        },
    );
    map.insert(
        SmolStr::new("log10"),
        BuiltinFunctionDoc {
            description: "Returns the base-10 logarithm of the given number.",
            params: &["number"],
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
        SmolStr::new("gmtime"),
        BuiltinFunctionDoc {
            description: "Converts Unix timestamp (seconds since epoch) to broken-down UTC time array [year, mon (0-11), mday, hour, min, sec, wday (0=Sun), yday (0-365)].",
            params: &["timestamp"],
        },
    );
    map.insert(
        SmolStr::new("localtime"),
        BuiltinFunctionDoc {
            description: "Converts Unix timestamp (seconds since epoch) to broken-down local time array [year, mon (0-11), mday, hour, min, sec, wday (0=Sun), yday (0-365)].",
            params: &["timestamp"],
        },
    );
    map.insert(
        SmolStr::new("mktime"),
        BuiltinFunctionDoc {
            description: "Converts broken-down UTC time array [year, mon (0-11), mday, hour, min, sec, wday, yday] to Unix timestamp (seconds since epoch).",
            params: &["time_array"],
        },
    );
    map.insert(
        SmolStr::new("strftime"),
        BuiltinFunctionDoc {
            description: "Formats a Unix timestamp (seconds) as a date string using the given strftime format (e.g. \"%Y-%m-%d\").",
            params: &["timestamp", "format"],
        },
    );
    map.insert(
        SmolStr::new("date_add"),
        BuiltinFunctionDoc {
            description: "Adds n units to a broken-down time array and returns a new array. Units: \"seconds\", \"minutes\", \"hours\", \"days\", \"weeks\", \"months\", \"years\". Month/year arithmetic is calendar-aware.",
            params: &["array", "n", "unit"],
        },
    );
    map.insert(
        SmolStr::new("date_diff"),
        BuiltinFunctionDoc {
            description: "Returns the difference (array2 - array1) in the given unit. Units: \"seconds\", \"minutes\", \"hours\", \"days\", \"weeks\".",
            params: &["array1", "array2", "unit"],
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
        SmolStr::new("base64url"),
        BuiltinFunctionDoc {
            description: "Encodes the given string to URL-safe base64.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("base64urld"),
        BuiltinFunctionDoc {
            description: "Decodes the given URL-safe base64 string.",
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
        SmolStr::new("from_html"),
        BuiltinFunctionDoc {
            description: "Converts the given HTML string to Markdown.",
            params: &["html"],
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
        SmolStr::new("md5"),
        BuiltinFunctionDoc {
            description: "Computes the MD5 hash of a string or bytes and returns a lowercase hex string.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("sha256"),
        BuiltinFunctionDoc {
            description: "Computes the SHA-256 hash of a string or bytes and returns a lowercase hex string.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("sha512"),
        BuiltinFunctionDoc {
            description: "Computes the SHA-512 hash of a string or bytes and returns a lowercase hex string.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("to_bytes"),
        BuiltinFunctionDoc {
            description: "Converts a string (UTF-8), array of numbers, or bytes to raw bytes.",
            params: &["value"],
        },
    );
    map.insert(
        SmolStr::new("from_hex"),
        BuiltinFunctionDoc {
            description: "Parses a hex string into raw bytes.",
            params: &["hex_string"],
        },
    );
    map.insert(
        SmolStr::new("to_hex"),
        BuiltinFunctionDoc {
            description: "Encodes raw bytes as a lowercase hex string.",
            params: &["bytes"],
        },
    );
    map.insert(
        SmolStr::new("utf8"),
        BuiltinFunctionDoc {
            description: "Decodes bytes as a UTF-8 string, returning an error if the bytes are not valid UTF-8.",
            params: &["bytes"],
        },
    );
    map.insert(
        SmolStr::new("xor"),
        BuiltinFunctionDoc {
            description: "Computes the bitwise XOR of two byte arrays of equal length.",
            params: &["bytes1", "bytes2"],
        },
    );
    map.insert(
        SmolStr::new("band"),
        BuiltinFunctionDoc {
            description: "Computes the bitwise AND of two byte arrays of equal length.",
            params: &["bytes1", "bytes2"],
        },
    );
    map.insert(
        SmolStr::new("bor"),
        BuiltinFunctionDoc {
            description: "Computes the bitwise OR of two byte arrays of equal length.",
            params: &["bytes1", "bytes2"],
        },
    );
    map.insert(
        SmolStr::new("bnot"),
        BuiltinFunctionDoc {
            description: "Computes the bitwise NOT (complement) of a byte array.",
            params: &["bytes"],
        },
    );
    map.insert(
        SmolStr::new("pack"),
        BuiltinFunctionDoc {
            description: "Packs a number into bytes using the given format. Supported formats: u8, i8, u16be/le, i16be/le, u32be/le, i32be/le, u64be/le, i64be/le, f32be/le, f64be/le.",
            params: &["format", "value"],
        },
    );
    map.insert(
        SmolStr::new("unpack"),
        BuiltinFunctionDoc {
            description: "Unpacks a number from bytes using the given format. Supported formats: u8, i8, u16be/le, i16be/le, u32be/le, i32be/le, u64be/le, i64be/le, f32be/le, f64be/le.",
            params: &["format", "bytes"],
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
            description: "Checks if the given string or byte array ends with the specified suffix.",
            params: &["value", "suffix"],
        },
    );
    map.insert(
        SmolStr::new("starts_with"),
        BuiltinFunctionDoc {
            description: "Checks if the given string or byte array starts with the specified prefix.",
            params: &["value", "prefix"],
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
        SmolStr::new("is_regex_match"),
        BuiltinFunctionDoc {
            description: "Checks if the given pattern matches the string.",
            params: &["string", "pattern"],
        },
    );
    map.insert(
        SmolStr::new("is_not_regex_match"),
        BuiltinFunctionDoc {
            description: "Checks if the given pattern does not match the string.",
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
        SmolStr::new("ltrim"),
        BuiltinFunctionDoc {
            description: "Trims whitespace from the left end of the given string.",
            params: &["input"],
        },
    );
    map.insert(
        SmolStr::new("rtrim"),
        BuiltinFunctionDoc {
            description: "Trims whitespace from the right end of the given string.",
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
            description: "Finds the first occurrence of a substring or byte subsequence. Returns -1 if not found.",
            params: &["value", "needle"],
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
            description: "Finds the last occurrence of a substring or byte subsequence. Returns -1 if not found.",
            params: &["value", "needle"],
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
        SmolStr::new("convert"),
        BuiltinFunctionDoc {
            description: "Converts the input value to the specified format. Supported formats: base64, html, text, uri, heading (#, ##, etc.), blockquote (>), list item (-), or link (URL).",
            params: &["input", "format"],
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
        SmolStr::new("sqrt"),
        BuiltinFunctionDoc {
            description: "Returns the square root of the given number.",
            params: &["number"],
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

    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("read_file"),
        BuiltinFunctionDoc {
            description: "Reads the contents of a file at the given path and returns it as a string.",
            params: &["path"],
        },
    );
    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("file_exists"),
        BuiltinFunctionDoc {
            description: "Checks if a file exists at the given path.",
            params: &["path"],
        },
    );

    map.insert(
        SmolStr::new("basename"),
        BuiltinFunctionDoc {
            description: "Returns the final component of a path string (e.g. \"file.txt\" from \"/a/b/file.txt\").",
            params: &["path"],
        },
    );
    map.insert(
        SmolStr::new("dirname"),
        BuiltinFunctionDoc {
            description: "Returns the parent directory of a path string (e.g. \"/a/b\" from \"/a/b/file.txt\"). Returns \".\" if the path has no parent.",
            params: &["path"],
        },
    );
    map.insert(
        SmolStr::new("extname"),
        BuiltinFunctionDoc {
            description: "Returns the extension of a file path including the leading dot (e.g. \".txt\" from \"file.txt\"). Returns an empty string if there is no extension.",
            params: &["path"],
        },
    );
    map.insert(
        SmolStr::new("stem"),
        BuiltinFunctionDoc {
            description: "Returns the file name without the extension (e.g. \"file\" from \"/a/b/file.txt\").",
            params: &["path"],
        },
    );
    map.insert(
        SmolStr::new("path_join"),
        BuiltinFunctionDoc {
            description: "Joins a base path with a component path and returns the resulting path string (e.g. path_join(\"/a/b\", \"c.txt\") → \"/a/b/c.txt\").",
            params: &["base", "component"],
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
            description: "Captures named groups from the given string based on the specified regular expression pattern and returns them as a dictionary keyed by group names.",
            params: &["string", "pattern"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::SHIFT_LEFT),
        BuiltinFunctionDoc {
            description: "Performs a left shift operation on the given value: for numbers, this is a bitwise left shift by the specified number of positions; for strings, this removes characters from the start; for Markdown headings, this increases the heading level accordingly.",
            params: &["value", "shift_amount"],
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::SHIFT_RIGHT),
        BuiltinFunctionDoc {
            description: "Performs a bitwise right shift on numbers, slices characters from the end of strings, and adjusts Markdown heading levels when applied to headings, using the given shift amount.",
            params: &["value", "shift_amount"],
        },
    );
    map.insert(
        SmolStr::new("partial"),
        BuiltinFunctionDoc {
            description: "Creates a new function by partially applying the given arguments to the specified function.",
            params: &["function", "arg1", "arg2", "..."],
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
    #[error("")]
    InvalidConvert(String),
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
                args: args.iter().map(|o| o.name().into()).collect::<Vec<_>>(),
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
            Error::InvalidConvert(format) => {
                RuntimeError::InvalidConvert((*get_token(token_arena, node.token_id)).clone(), format.clone())
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
                let mut new_args = Args::with_capacity(args.len() + 1);
                new_args.push(runtime_value.clone());
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

fn collect_depth_values(args: &[RuntimeValue]) -> Vec<u8> {
    args.iter()
        .flat_map(|arg| match arg {
            RuntimeValue::Number(n) => vec![n.value() as u8],
            RuntimeValue::Array(arr) => arr
                .iter()
                .filter_map(|v| {
                    if let RuntimeValue::Number(n) = v {
                        Some(n.value() as u8)
                    } else {
                        None
                    }
                })
                .collect(),
            _ => vec![],
        })
        .collect()
}

fn collect_runtime_values(args: &[RuntimeValue]) -> Vec<RuntimeValue> {
    args.iter()
        .flat_map(|arg| match arg {
            RuntimeValue::Number(n) => vec![(*n).into()],
            RuntimeValue::Array(arr) => arr
                .iter()
                .filter_map(|v| {
                    if let RuntimeValue::Number(n) = v {
                        Some((*n).into())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => vec![],
        })
        .collect()
}

fn collect_string_values(args: &[RuntimeValue]) -> Vec<String> {
    args.iter()
        .flat_map(|arg| match arg {
            RuntimeValue::String(s) => vec![s.clone()],
            RuntimeValue::Array(arr) => arr
                .iter()
                .filter_map(|v| {
                    if let RuntimeValue::String(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => vec![],
        })
        .collect()
}

/// Evaluates a selector with runtime arguments against a markdown node.
///
/// Supports filtered matching for selectors that accept arguments:
/// - `Heading`: filters by depth using numeric or range args (e.g. `.h(1..2)`, `.h(1, 2)`)
/// - `Code`: filters by language using string args (e.g. `.code("rust")`)
/// - `List`: filters by list item index using a numeric arg (e.g. `.[v]` where `v` evaluates to an index)
/// - `Table`: filters table cells by positional args where `args[0]` is the row and `args[1]` is the
///   column; a `None`/[`RuntimeValue::None`] value in either position acts as a wildcard matching any
///   row or column respectively (e.g. `.[v][]` matches row `v` of any column, `.[][v]` matches column
///   `v` of any row)
/// - All other selectors fall back to [`eval_selector`].
pub fn eval_selector_with_args(node: &mq_markdown::Node, selector: &Selector, args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return eval_selector(node, selector);
    }

    let is_match = match selector {
        Selector::Heading(_) => {
            let depths = collect_depth_values(args);

            if depths.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::Heading(mq_markdown::Heading { depth, .. }) = node {
                depths.contains(depth)
            } else {
                false
            }
        }
        Selector::Code => {
            let langs = collect_string_values(args);

            if langs.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::Code(mq_markdown::Code { lang, .. }) = node {
                let node_lang = lang.as_deref().unwrap_or("");
                langs.iter().any(|l| l == node_lang)
            } else {
                false
            }
        }
        Selector::List(..) => {
            let indices = collect_runtime_values(args);

            if indices.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::List(mq_markdown::List { index: list_index, .. }) = node {
                indices.iter().any(|i| match i {
                    RuntimeValue::Number(n) => *list_index == n.value() as usize,
                    _ => false,
                })
            } else {
                false
            }
        }
        Selector::Table(..) => {
            if args.is_empty() {
                return eval_selector(node, selector);
            }

            match node {
                mq_markdown::Node::TableCell(mq_markdown::TableCell { column, row, .. }) => {
                    let matches_pos = |spec: Option<&RuntimeValue>, actual: usize| -> bool {
                        match spec {
                            None | Some(RuntimeValue::None) => true,
                            Some(RuntimeValue::Number(n)) => actual == n.value() as usize,
                            _ => false,
                        }
                    };
                    matches_pos(args.first(), *row) && matches_pos(args.get(1), *column)
                }
                _ => false,
            }
        }
        _ => return eval_selector(node, selector),
    };

    if is_match {
        RuntimeValue::new_markdown(node.clone())
    } else {
        RuntimeValue::NONE
    }
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
        Selector::Table(row, column) => match node {
            mq_markdown::Node::TableCell(mq_markdown::TableCell {
                column: column2,
                row: row2,
                ..
            }) => match (row, column) {
                (Some(r), Some(c)) => r == row2 && c == column2,
                (Some(r), None) => r == row2,
                (None, Some(c)) => c == column2,
                (None, None) => true,
            },
            mq_markdown::Node::TableAlign(_) if row.is_none() && column.is_none() => true,
            _ => false,
        },
        Selector::TableAlign => node.is_table_align(),
        Selector::Html => node.is_html(),
        Selector::Footnote => node.is_footnote(),
        Selector::MdxJsxFlowElement => node.is_mdx_jsx_flow_element(),
        Selector::List(index, checked) => match node {
            mq_markdown::Node::List(mq_markdown::List {
                index: list_index,
                checked: list_checked,
                ..
            }) => match index {
                Some(i) => i == list_index && checked == list_checked,
                None => true,
            },
            _ => false,
        },
        Selector::Task => matches!(
            node,
            mq_markdown::Node::List(mq_markdown::List { checked: Some(_), .. })
        ),
        Selector::Todo => matches!(
            node,
            mq_markdown::Node::List(mq_markdown::List {
                checked: Some(false),
                ..
            })
        ),
        Selector::Done => matches!(
            node,
            mq_markdown::Node::List(mq_markdown::List {
                checked: Some(true),
                ..
            })
        ),
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
        Selector::Property(_) => false,
    };

    if is_match {
        RuntimeValue::new_markdown(node.clone())
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
            .map(RuntimeValue::new_markdown)
            .collect(),
    )
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
        RuntimeValue::Bytes(b) => {
            if n == 0 {
                return Ok(RuntimeValue::Bytes(vec![]));
            }
            let total_size = b.len().saturating_mul(n);
            if total_size > MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "bytes repeat size {} exceeds maximum allowed size of {}",
                    total_size, MAX_RANGE_SIZE
                )));
            }
            let mut repeated = Vec::with_capacity(total_size);
            for _ in 0..n {
                repeated.extend_from_slice(b);
            }
            Ok(RuntimeValue::Bytes(repeated))
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
    #[case("add", vec![], Error::InvalidNumberOfArguments("add".to_string(), 2, 0))]
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
    fn test_gmtime_epoch() {
        // Unix epoch (0) → 1970-01-01T00:00:00 UTC (Thursday)
        // format: [year, mon(0-11), mday, hour, min, sec, wday(0=Sun), yday(0-365)]
        let ident = Ident::new("gmtime");
        let args = vec![RuntimeValue::Number(0.into())];
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            args,
            &Shared::new(SharedCell::new(Env::default())),
        )
        .unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(vec![
                RuntimeValue::Number(1970.into()), // year
                RuntimeValue::Number(0.into()),    // mon (Jan=0)
                RuntimeValue::Number(1.into()),    // mday
                RuntimeValue::Number(0.into()),    // hour
                RuntimeValue::Number(0.into()),    // min
                RuntimeValue::Number(0.into()),    // sec
                RuntimeValue::Number(4.into()),    // wday (Thu=4)
                RuntimeValue::Number(0.into()),    // yday
            ])
        );
    }

    #[test]
    fn test_gmtime_known_date() {
        // 2024-01-01T00:00:00 UTC = 1704067200 seconds
        let ident = Ident::new("gmtime");
        let args = vec![RuntimeValue::Number(1704067200_i64.into())];
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            args,
            &Shared::new(SharedCell::new(Env::default())),
        )
        .unwrap();
        assert_eq!(
            result,
            RuntimeValue::Array(vec![
                RuntimeValue::Number(2024.into()), // year
                RuntimeValue::Number(0.into()),    // mon (Jan=0)
                RuntimeValue::Number(1.into()),    // mday
                RuntimeValue::Number(0.into()),    // hour
                RuntimeValue::Number(0.into()),    // min
                RuntimeValue::Number(0.into()),    // sec
                RuntimeValue::Number(1.into()),    // wday (Mon=1)
                RuntimeValue::Number(0.into()),    // yday
            ])
        );
    }

    #[rstest]
    #[case(0, 0)]
    #[case(1704067200_i64, 1704067200_i64)]
    #[case(1718454645_i64, 1718454645_i64)]
    fn test_mktime_roundtrip(#[case] secs: i64, #[case] expected: i64) {
        let env = Shared::new(SharedCell::new(Env::default()));
        let gmtime_ident = Ident::new("gmtime");
        let mktime_ident = Ident::new("mktime");

        let arr = eval_builtin(
            &RuntimeValue::None,
            &gmtime_ident,
            vec![RuntimeValue::Number(secs.into())],
            &env,
        )
        .unwrap();
        let result = eval_builtin(&RuntimeValue::None, &mktime_ident, vec![arr], &env).unwrap();
        assert_eq!(result, RuntimeValue::Number(expected.into()));
    }

    #[rstest]
    #[case(1704067200_i64, "%Y-%m-%d", "2024-01-01")]
    #[case(0_i64, "%Y-%m-%dT%H:%M:%S", "1970-01-01T00:00:00")]
    #[case(1704067200_i64, "%Y", "2024")]
    fn test_strftime(#[case] ts: i64, #[case] fmt: &str, #[case] expected: &str) {
        let ident = Ident::new("strftime");
        let args = vec![RuntimeValue::Number(ts.into()), RuntimeValue::String(fmt.into())];
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            args,
            &Shared::new(SharedCell::new(Env::default())),
        )
        .unwrap();
        assert_eq!(result, RuntimeValue::String(expected.into()));
    }

    fn gmtime_array(secs: i64) -> RuntimeValue {
        let env = Shared::new(SharedCell::new(Env::default()));
        eval_builtin(
            &RuntimeValue::None,
            &Ident::new("gmtime"),
            vec![RuntimeValue::Number(secs.into())],
            &env,
        )
        .unwrap()
    }

    // date_add: simple durations
    #[rstest]
    #[case(1704067200_i64, 60, "seconds", 1704067260_i64)]
    #[case(1704067200_i64, 5, "minutes", 1704067500_i64)]
    #[case(1704067200_i64, 2, "hours", 1704074400_i64)]
    #[case(1704067200_i64, 1, "days", 1704153600_i64)]
    #[case(1704067200_i64, -1,  "days",    1703980800_i64)]
    #[case(1704067200_i64, 1, "weeks", 1704672000_i64)]
    fn test_date_add_duration(#[case] base: i64, #[case] n: i64, #[case] unit: &str, #[case] expected_secs: i64) {
        let env = Shared::new(SharedCell::new(Env::default()));
        let arr = gmtime_array(base);
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_add"),
            vec![arr, RuntimeValue::Number(n.into()), RuntimeValue::String(unit.into())],
            &env,
        )
        .unwrap();
        // convert result array back to timestamp via mktime and compare
        let ts = eval_builtin(&RuntimeValue::None, &Ident::new("mktime"), vec![result], &env).unwrap();
        assert_eq!(ts, RuntimeValue::Number(expected_secs.into()));
    }

    // date_add: calendar-aware month/year arithmetic
    #[test]
    fn test_date_add_months_end_of_month() {
        // 2024-01-31 + 1 month = 2024-02-29 (leap year)
        let env = Shared::new(SharedCell::new(Env::default()));
        let arr = gmtime_array(1706659200); // 2024-01-31T00:00:00Z
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_add"),
            vec![
                arr,
                RuntimeValue::Number(1.into()),
                RuntimeValue::String("months".into()),
            ],
            &env,
        )
        .unwrap();
        // 2024-02-29T00:00:00Z = 1709164800
        let ts = eval_builtin(&RuntimeValue::None, &Ident::new("mktime"), vec![result], &env).unwrap();
        assert_eq!(ts, RuntimeValue::Number(1709164800_i64.into()));
    }

    #[test]
    fn test_date_add_years() {
        // 2024-02-29 + 1 year = 2025-02-28 (non-leap year clamps)
        let env = Shared::new(SharedCell::new(Env::default()));
        let arr = gmtime_array(1709164800); // 2024-02-29T00:00:00Z
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_add"),
            vec![
                arr,
                RuntimeValue::Number(1.into()),
                RuntimeValue::String("years".into()),
            ],
            &env,
        )
        .unwrap();
        // 2025-02-28T00:00:00Z = 1740700800
        let ts = eval_builtin(&RuntimeValue::None, &Ident::new("mktime"), vec![result], &env).unwrap();
        assert_eq!(ts, RuntimeValue::Number(1740700800_i64.into()));
    }

    #[test]
    fn test_date_add_invalid_unit() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let arr = gmtime_array(0);
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_add"),
            vec![
                arr,
                RuntimeValue::Number(1.into()),
                RuntimeValue::String("centuries".into()),
            ],
            &env,
        );
        assert!(matches!(result, Err(Error::Runtime(_))));
    }

    // date_diff: difference in various units
    #[rstest]
    #[case(1704067200_i64, 1704153600_i64, "seconds", 86400_i64)]
    #[case(1704067200_i64, 1704153600_i64, "minutes", 1440_i64)]
    #[case(1704067200_i64, 1704153600_i64, "hours", 24_i64)]
    #[case(1704067200_i64, 1704153600_i64, "days", 1_i64)]
    #[case(1704067200_i64, 1704672000_i64, "weeks", 1_i64)]
    #[case(1704153600_i64, 1704067200_i64, "seconds", -86400_i64)]
    fn test_date_diff(#[case] base1: i64, #[case] base2: i64, #[case] unit: &str, #[case] expected: i64) {
        let env = Shared::new(SharedCell::new(Env::default()));
        let arr1 = gmtime_array(base1);
        let arr2 = gmtime_array(base2);
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_diff"),
            vec![arr1, arr2, RuntimeValue::String(unit.into())],
            &env,
        )
        .unwrap();
        assert_eq!(result, RuntimeValue::Number(expected.into()));
    }

    #[test]
    fn test_date_diff_invalid_unit() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let arr = gmtime_array(0);
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_diff"),
            vec![arr.clone(), arr, RuntimeValue::String("months".into())],
            &env,
        );
        assert!(matches!(result, Err(Error::Runtime(_))));
    }

    #[test]
    fn test_gmtime_invalid_type() {
        let ident = Ident::new("gmtime");
        let args = vec![RuntimeValue::String("not a number".into())];
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            args,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(matches!(result, Err(Error::InvalidTypes(_, _))));
    }

    #[test]
    fn test_mktime_invalid_input() {
        let ident = Ident::new("mktime");
        let args = vec![RuntimeValue::String("not an array".into())];
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            args,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(matches!(result, Err(Error::InvalidTypes(_, _))));
    }

    #[test]
    fn test_date_add_malformed_array_error_prefix() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let bad_arr = RuntimeValue::Array(vec![RuntimeValue::String("x".into()); 8]);
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_add"),
            vec![
                bad_arr,
                RuntimeValue::Number(1.into()),
                RuntimeValue::String("days".into()),
            ],
            &env,
        );
        match result {
            Err(Error::Runtime(msg)) => assert!(msg.starts_with("date_add:"), "expected date_add prefix, got: {msg}"),
            other => panic!("expected Runtime error, got: {other:?}"),
        }
    }

    #[test]
    fn test_date_diff_malformed_array_error_prefix() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let bad_arr = RuntimeValue::Array(vec![RuntimeValue::String("x".into()); 8]);
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_diff"),
            vec![bad_arr.clone(), bad_arr, RuntimeValue::String("days".into())],
            &env,
        );
        match result {
            Err(Error::Runtime(msg)) => assert!(msg.starts_with("date_diff:"), "expected date_diff prefix, got: {msg}"),
            other => panic!("expected Runtime error, got: {other:?}"),
        }
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
    #[case::task_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Task,
        true
    )]
    #[case::task_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Task,
        true
    )]
    #[case::task_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
        Selector::Task,
        false
    )]
    #[case::todo_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Todo,
        true
    )]
    #[case::todo_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Todo,
        false
    )]
    #[case::todo_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
        Selector::Todo,
        false
    )]
    #[case::done_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Done,
        true
    )]
    #[case::done_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Done,
        false
    )]
    #[case::done_list(
        Node::List(mq_markdown::List { values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
        Selector::Done,
        false
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
                    Box::new(Node::Text(mq_markdown::Text {
                        value: "hello".into(),
                        position: None,
                    })),
                    None
                ),
                RuntimeValue::Markdown(
                    Box::new(Node::Link(mq_markdown::Link {
                        url: mq_markdown::Url::new("url".into()),
                        title: None,
                        values: Vec::new(),
                        position: None,
                    })),
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
                RuntimeValue::new_markdown(inner_text),
                RuntimeValue::new_markdown(heading),
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
    #[rstest]
    #[case(
        BTreeMap::from([("a".into(), RuntimeValue::Number(1.0.into())), ("b".into(), RuntimeValue::Number(2.0.into()))]),
        BTreeMap::from([("c".into(), RuntimeValue::Number(3.0.into()))]),
        BTreeMap::from([("a".into(), RuntimeValue::Number(1.0.into())), ("b".into(), RuntimeValue::Number(2.0.into())), ("c".into(), RuntimeValue::Number(3.0.into()))]),
    )]
    #[case(
        BTreeMap::from([("a".into(), RuntimeValue::Number(1.0.into()))]),
        BTreeMap::from([("a".into(), RuntimeValue::Number(99.0.into())), ("b".into(), RuntimeValue::Number(2.0.into()))]),
        BTreeMap::from([("a".into(), RuntimeValue::Number(99.0.into())), ("b".into(), RuntimeValue::Number(2.0.into()))]),
    )]
    #[case(
        BTreeMap::new(),
        BTreeMap::from([("x".into(), RuntimeValue::String("hello".into()))]),
        BTreeMap::from([("x".into(), RuntimeValue::String("hello".into()))]),
    )]
    #[case(
        BTreeMap::from([("x".into(), RuntimeValue::String("hello".into()))]),
        BTreeMap::new(),
        BTreeMap::from([("x".into(), RuntimeValue::String("hello".into()))]),
    )]
    fn test_eval_builtin_add_dict(
        #[case] d1: BTreeMap<Ident, RuntimeValue>,
        #[case] d2: BTreeMap<Ident, RuntimeValue>,
        #[case] expected: BTreeMap<Ident, RuntimeValue>,
    ) {
        let ident = Ident::new("add");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Dict(d1), RuntimeValue::Dict(d2)],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::Dict(expected)));
    }

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

    #[rstest]
    #[case::simple_no_header(
        "a,b,c\n1,2,3\n4,5,6",
        Ok(RuntimeValue::Array(vec![
            RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ]),
            RuntimeValue::Array(vec![
                RuntimeValue::String("1".to_string()),
                RuntimeValue::String("2".to_string()),
                RuntimeValue::String("3".to_string()),
            ]),
            RuntimeValue::Array(vec![
                RuntimeValue::String("4".to_string()),
                RuntimeValue::String("5".to_string()),
                RuntimeValue::String("6".to_string()),
            ]),
        ]))
    )]
    #[case::single_row_no_header(
        "x,y",
        Ok(RuntimeValue::Array(vec![
            RuntimeValue::Array(vec![
                RuntimeValue::String("x".to_string()),
                RuntimeValue::String("y".to_string()),
            ]),
        ]))
    )]
    #[case::empty_no_header(
        "",
        Ok(RuntimeValue::Array(vec![]))
    )]
    fn test_csv_parse_no_header(#[case] csv: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_csv_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(csv.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::simple_with_header(
        "name,age\nAlice,30\nBob,25",
        {
            let mut alice = BTreeMap::new();
            alice.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            alice.insert(Ident::new("age"), RuntimeValue::String("30".to_string()));
            let mut bob = BTreeMap::new();
            bob.insert(Ident::new("name"), RuntimeValue::String("Bob".to_string()));
            bob.insert(Ident::new("age"), RuntimeValue::String("25".to_string()));
            Ok(RuntimeValue::Array(vec![
                RuntimeValue::Dict(alice),
                RuntimeValue::Dict(bob),
            ]))
        }
    )]
    #[case::single_row_with_header(
        "id,value\n1,hello",
        {
            let mut row = BTreeMap::new();
            row.insert(Ident::new("id"), RuntimeValue::String("1".to_string()));
            row.insert(Ident::new("value"), RuntimeValue::String("hello".to_string()));
            Ok(RuntimeValue::Array(vec![RuntimeValue::Dict(row)]))
        }
    )]
    #[case::quoted_fields_with_header(
        "name,note\n\"Doe, Jane\",\"says \"\"hi\"\"\"",
        {
            let mut row = BTreeMap::new();
            row.insert(Ident::new("name"), RuntimeValue::String("Doe, Jane".to_string()));
            row.insert(Ident::new("note"), RuntimeValue::String("says \"hi\"".to_string()));
            Ok(RuntimeValue::Array(vec![RuntimeValue::Dict(row)]))
        }
    )]
    fn test_csv_parse_with_header(#[case] csv: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_csv_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![
                RuntimeValue::String(csv.to_string()),
                RuntimeValue::String(",".to_string()),
                RuntimeValue::Boolean(true),
            ],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::tsv_no_header(
        "a\tb\tc\n1\t2\t3",
        "\t",
        false,
        Ok(RuntimeValue::Array(vec![
            RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ]),
            RuntimeValue::Array(vec![
                RuntimeValue::String("1".to_string()),
                RuntimeValue::String("2".to_string()),
                RuntimeValue::String("3".to_string()),
            ]),
        ]))
    )]
    #[case::tsv_with_header(
        "name\tage\nAlice\t30",
        "\t",
        true,
        {
            let mut row = BTreeMap::new();
            row.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            row.insert(Ident::new("age"), RuntimeValue::String("30".to_string()));
            Ok(RuntimeValue::Array(vec![RuntimeValue::Dict(row)]))
        }
    )]
    fn test_csv_parse_custom_delimiter(
        #[case] csv: &str,
        #[case] delimiter: &str,
        #[case] has_header: bool,
        #[case] expected: Result<RuntimeValue, Error>,
    ) {
        let ident = Ident::new("_csv_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![
                RuntimeValue::String(csv.to_string()),
                RuntimeValue::String(delimiter.to_string()),
                RuntimeValue::Boolean(has_header),
            ],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::invalid_type_number(RuntimeValue::Number(42.into()))]
    #[case::invalid_type_bool(RuntimeValue::Boolean(false))]
    fn test_csv_parse_invalid_arg_type(#[case] invalid_arg: RuntimeValue) {
        let ident = Ident::new("_csv_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![invalid_arg],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::simple_object(
        r#"{"key": "value"}"#,
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("key"), RuntimeValue::String("value".to_string()));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::array(
        r#"[1, 2, 3]"#,
        Ok(RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ]))
    )]
    #[case::nested(
        r#"{"a": [true, null], "b": {"c": 1.2}}"#,
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("a"), RuntimeValue::Array(vec![
                RuntimeValue::Boolean(true),
                RuntimeValue::NONE,
            ]));
            let mut inner = BTreeMap::new();
            inner.insert(Ident::new("c"), RuntimeValue::Number(1.2.into()));
            map.insert(Ident::new("b"), RuntimeValue::Dict(inner));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::string(r#""hello""#, Ok(RuntimeValue::String("hello".to_string())))]
    #[case::number(r#"42"#, Ok(RuntimeValue::Number(42.into())))]
    #[case::boolean(r#"false"#, Ok(RuntimeValue::Boolean(false)))]
    #[case::null(r#"null"#, Ok(RuntimeValue::NONE))]
    fn test_json_parse(#[case] json: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_json_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(json.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::invalid_json(r#"{"key": "value""#)]
    #[case::invalid_type(RuntimeValue::Number(1.into()))]
    fn test_json_parse_error(#[case] input: impl Into<RuntimeValue>) {
        let ident = Ident::new("_json_parse");
        let arg: RuntimeValue = match input.into() {
            RuntimeValue::Number(n) => RuntimeValue::Number(n),
            s => RuntimeValue::String(s.to_string()),
        };
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![arg],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::mapping(
        "key: value",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("key"), RuntimeValue::String("value".to_string()));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::sequence(
        "- 1\n- 2\n- 3",
        Ok(RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ]))
    )]
    #[case::nested(
        "a:\n  b: 42",
        {
            let mut inner = BTreeMap::new();
            inner.insert(Ident::new("b"), RuntimeValue::Number(42.into()));
            let mut map = BTreeMap::new();
            map.insert(Ident::new("a"), RuntimeValue::Dict(inner));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::boolean(
        "flag: true",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("flag"), RuntimeValue::Boolean(true));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::null(
        "value: null",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("value"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::float(
        "ratio: 1.5",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("ratio"), RuntimeValue::Number(1.5.into()));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    fn test_yaml_parse(#[case] yaml: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_yaml_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(yaml.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::invalid_type(RuntimeValue::Number(1.into()))]
    fn test_yaml_parse_error(#[case] input: impl Into<RuntimeValue>) {
        let ident = Ident::new("_yaml_parse");
        let arg: RuntimeValue = match input.into() {
            RuntimeValue::Number(n) => RuntimeValue::Number(n),
            s => RuntimeValue::String(s.to_string()),
        };
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![arg],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::simple_kv(
        "a: 1\nb: 2",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
            map.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::nested_indent(
        "parent:\n  child: value",
        {
            let mut child_map = BTreeMap::new();
            child_map.insert(Ident::new("child"), RuntimeValue::String("value".to_string()));
            let mut parent_map = BTreeMap::new();
            parent_map.insert(Ident::new("parent"), RuntimeValue::Dict(child_map));
            Ok(RuntimeValue::Dict(parent_map))
        }
    )]
    #[case::tabular_data(
        "hikes[2]{id,name}:\n  1,Blue Lake\n  2,Ridge Trail",
        {
            let mut row1 = BTreeMap::new();
            row1.insert(Ident::new("id"), RuntimeValue::Number(1.into()));
            row1.insert(Ident::new("name"), RuntimeValue::String("Blue Lake".to_string()));
            let mut row2 = BTreeMap::new();
            row2.insert(Ident::new("id"), RuntimeValue::Number(2.into()));
            row2.insert(Ident::new("name"), RuntimeValue::String("Ridge Trail".to_string()));
            let mut map = BTreeMap::new();
            map.insert(Ident::new("hikes"), RuntimeValue::Array(vec![RuntimeValue::Dict(row1), RuntimeValue::Dict(row2)]));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::inline_array(
        "items[3]: 1, 2, 3",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("items"), RuntimeValue::Array(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Number(3.into()),
            ]));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::expanded_array(
        "items[2]:\n  - 1\n  - 2",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("items"), RuntimeValue::Array(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
            ]));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::primitives(
        "s: \"string\"\nb: true\nn: null\nf: false",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("s"), RuntimeValue::String("string".to_string()));
            map.insert(Ident::new("b"), RuntimeValue::TRUE);
            map.insert(Ident::new("n"), RuntimeValue::NONE);
            map.insert(Ident::new("f"), RuntimeValue::FALSE);
            Ok(RuntimeValue::Dict(map))
        }
    )]
    fn test_toon_parse(#[case] toon: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_toon_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(toon.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::simple_kv(
        "name = \"Alice\"\nage = 30",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::boolean(
        "enabled = true\ndisabled = false",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("enabled"), RuntimeValue::Boolean(true));
            map.insert(Ident::new("disabled"), RuntimeValue::Boolean(false));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::nested_table(
        "[server]\nhost = \"localhost\"\nport = 8080",
        {
            let mut inner = BTreeMap::new();
            inner.insert(Ident::new("host"), RuntimeValue::String("localhost".to_string()));
            inner.insert(Ident::new("port"), RuntimeValue::Number(8080.into()));
            let mut map = BTreeMap::new();
            map.insert(Ident::new("server"), RuntimeValue::Dict(inner));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    #[case::array(
        "tags = [\"rust\", \"toml\"]",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("tags"), RuntimeValue::Array(vec![
                RuntimeValue::String("rust".to_string()),
                RuntimeValue::String("toml".to_string()),
            ]));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    fn test_toml_parse(#[case] toml: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_toml_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(toml.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::invalid_toml("name = ")]
    fn test_toml_parse_error(#[case] input: &str) {
        let ident = Ident::new("_toml_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(input.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::invalid_type(RuntimeValue::Number(1.into()))]
    fn test_toml_parse_invalid_type(#[case] input: RuntimeValue) {
        let ident = Ident::new("_toml_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![input],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::simple_map(
        // {"name": "Alice", "age": 30}
        "omRuYW1lZUFsaWNlY2FnZRge",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    fn test_cbor_parse(#[case] input: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_cbor_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(input.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::invalid_base64("not-valid-base64!!!")]
    #[case::invalid_cbor("aGVsbG8=")]
    fn test_cbor_parse_error(#[case] input: &str) {
        let ident = Ident::new("_cbor_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(input.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::invalid_type(RuntimeValue::Number(1.into()))]
    fn test_cbor_parse_invalid_type(#[case] input: RuntimeValue) {
        let ident = Ident::new("_cbor_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![input],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::simple_block(
        r#"resource "aws_instance" "example" { ami = "abc-123" }"#,
        {
            let mut instance = BTreeMap::new();
            instance.insert(Ident::new("ami"), RuntimeValue::String("abc-123".to_string()));
            let mut example = BTreeMap::new();
            example.insert(Ident::new("example"), RuntimeValue::Dict(instance));
            let mut resource = BTreeMap::new();
            resource.insert(Ident::new("aws_instance"), RuntimeValue::Dict(example));
            let mut map = BTreeMap::new();
            map.insert(Ident::new("resource"), RuntimeValue::Dict(resource));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    fn test_hcl_parse(#[case] input: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_hcl_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(input.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::invalid_type(RuntimeValue::Number(1.into()))]
    fn test_hcl_parse_invalid_type(#[case] input: RuntimeValue) {
        let ident = Ident::new("_hcl_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![input],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_hcl_stringify_dict() {
        let ident = Ident::new("_hcl_stringify");
        let mut map = BTreeMap::new();
        map.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
        map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
        let input = RuntimeValue::Dict(map);
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![input],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_ok());
        let s = result.unwrap().to_string();
        assert!(s.contains("name") && s.contains("Alice"));
        assert!(s.contains("age"));
    }

    #[rstest]
    #[case::simple_map(
        // {"name": "Alice", "age": 30} encoded as CBOR then base64
        "omRuYW1lZUFsaWNlY2FnZRge",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
            Ok(RuntimeValue::Dict(map))
        }
    )]
    fn test_cbor_stringify_roundtrip(#[case] base64_input: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let env = Shared::new(SharedCell::new(Env::default()));

        // parse
        let ident_parse = Ident::new("_cbor_parse");
        let parsed = eval_builtin(
            &RuntimeValue::None,
            &ident_parse,
            vec![RuntimeValue::String(base64_input.to_string())],
            &env,
        );
        assert!(parsed.is_ok());
        assert_eq!(parsed.as_ref().ok(), expected.as_ref().ok());

        // stringify
        let ident_stringify = Ident::new("_cbor_stringify");
        let bytes_result = eval_builtin(&RuntimeValue::None, &ident_stringify, vec![parsed.unwrap()], &env);
        assert!(bytes_result.is_ok());
        assert!(matches!(bytes_result.unwrap(), RuntimeValue::Bytes(_)));
    }

    #[test]
    fn test_cbor_parse_from_bytes() {
        // {"name": "Alice"} as raw CBOR bytes
        let cbor_bytes = base64::engine::general_purpose::STANDARD
            .decode("oWRuYW1lZUFsaWNl")
            .unwrap();
        let ident = Ident::new("_cbor_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(cbor_bytes)],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_ok());
        let mut expected = BTreeMap::new();
        expected.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
        assert_eq!(result.unwrap(), RuntimeValue::Dict(expected));
    }

    #[test]
    fn test_base64_bytes_input() {
        let ident = Ident::new("base64");
        let bytes = vec![0x48u8, 0x65, 0x6c, 0x6c, 0x6f]; // "Hello"
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(bytes)],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::String("SGVsbG8=".to_string())));
    }

    #[rstest]
    #[case::string(
        RuntimeValue::String("hello".to_string()),
        Ok(RuntimeValue::Bytes(vec![0x68, 0x65, 0x6c, 0x6c, 0x6f]))
    )]
    #[case::empty_string(
        RuntimeValue::String("".to_string()),
        Ok(RuntimeValue::Bytes(vec![]))
    )]
    #[case::utf8_string(
        RuntimeValue::String("あ".to_string()),
        Ok(RuntimeValue::Bytes(vec![0xe3, 0x81, 0x82]))
    )]
    #[case::array_of_numbers(
        RuntimeValue::Array(vec![
            RuntimeValue::Number(0.into()),
            RuntimeValue::Number(255.into()),
            RuntimeValue::Number(128.into()),
        ]),
        Ok(RuntimeValue::Bytes(vec![0, 255, 128]))
    )]
    #[case::bytes_identity(
        RuntimeValue::Bytes(vec![1, 2, 3]),
        Ok(RuntimeValue::Bytes(vec![1, 2, 3]))
    )]
    fn test_to_bytes(#[case] input: RuntimeValue, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("to_bytes");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![input],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::number(RuntimeValue::Number(42.into()))]
    #[case::array_with_non_number(RuntimeValue::Array(vec![RuntimeValue::String("x".to_string())]))]
    #[case::array_with_negative(RuntimeValue::Array(vec![RuntimeValue::Number((-1i64).into())]))]
    #[case::array_with_256(RuntimeValue::Array(vec![RuntimeValue::Number(256i64.into())]))]
    #[case::array_with_fractional(RuntimeValue::Array(vec![RuntimeValue::Number(1.5f64.into())]))]
    #[case::array_with_nan(RuntimeValue::Array(vec![RuntimeValue::Number(f64::NAN.into())]))]
    #[case::array_with_infinity(RuntimeValue::Array(vec![RuntimeValue::Number(f64::INFINITY.into())]))]
    fn test_to_bytes_invalid(#[case] input: RuntimeValue) {
        let ident = Ident::new("to_bytes");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![input],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_bytes_add() {
        let ident = Ident::new("add");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(vec![1, 2]), RuntimeValue::Bytes(vec![3, 4])],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::Bytes(vec![1, 2, 3, 4])));
    }

    #[test]
    fn test_bytes_reverse() {
        let ident = Ident::new("reverse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(vec![1, 2, 3])],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::Bytes(vec![3, 2, 1])));
    }

    #[test]
    fn test_bytes_slice() {
        let ident = Ident::new("slice");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![
                RuntimeValue::Bytes(vec![10, 20, 30, 40, 50]),
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(4.into()),
            ],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::Bytes(vec![20, 30, 40])));
    }

    #[test]
    fn test_md5_bytes_input() {
        let ident = Ident::new("md5");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(b"hello".to_vec())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result,
            Ok(RuntimeValue::String("5d41402abc4b2a76b9719d911017c592".to_string()))
        );
    }

    #[test]
    fn test_sha256_bytes_input() {
        let ident = Ident::new("sha256");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(b"hello".to_vec())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result,
            Ok(RuntimeValue::String(
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string()
            ))
        );
    }

    #[test]
    fn test_sha512_string_input() {
        let ident = Ident::new("sha512");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String("hello".to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result,
            Ok(RuntimeValue::String(
                "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043".to_string()
            ))
        );
    }

    #[test]
    fn test_sha512_bytes_input() {
        let ident = Ident::new("sha512");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(b"hello".to_vec())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result,
            Ok(RuntimeValue::String(
                "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043".to_string()
            ))
        );
    }

    #[rstest]
    #[case::lowercase("deadbeef", Ok(RuntimeValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])))]
    #[case::uppercase("DEADBEEF", Ok(RuntimeValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef])))]
    #[case::empty("", Ok(RuntimeValue::Bytes(vec![])))]
    fn test_from_hex(#[case] input: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("from_hex");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(input.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::odd_length("abc", true)]
    #[case::invalid_chars("zzzz", true)]
    fn test_from_hex_invalid(#[case] input: &str, #[case] is_err: bool) {
        let ident = Ident::new("from_hex");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(input.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result.is_err(), is_err);
    }

    #[rstest]
    #[case::basic(vec![0xde, 0xad, 0xbe, 0xef], Ok(RuntimeValue::String("deadbeef".to_string())))]
    #[case::empty(vec![], Ok(RuntimeValue::String("".to_string())))]
    #[case::zero_ff(vec![0x00, 0xff], Ok(RuntimeValue::String("00ff".to_string())))]
    #[case::all_zeros(vec![0x00, 0x00], Ok(RuntimeValue::String("0000".to_string())))]
    fn test_to_hex(#[case] input: Vec<u8>, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("to_hex");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(input)],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_to_hex_roundtrip() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let original = vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
        let hex = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("to_hex"),
            vec![RuntimeValue::Bytes(original.clone())],
            &env,
        )
        .unwrap();
        let roundtripped = eval_builtin(&RuntimeValue::None, &Ident::new("from_hex"), vec![hex], &env).unwrap();
        assert_eq!(roundtripped, RuntimeValue::Bytes(original));
    }

    #[rstest]
    #[case("gt",  vec![0x02], vec![0x01], true)]
    #[case("gt",  vec![0x01], vec![0x02], false)]
    #[case("gt",  vec![0x01], vec![0x01], false)]
    #[case("gt",  vec![0x01, 0x00], vec![0x01], true)]
    #[case("gte", vec![0x02], vec![0x01], true)]
    #[case("gte", vec![0x01], vec![0x01], true)]
    #[case("gte", vec![0x01], vec![0x02], false)]
    #[case("lt",  vec![0x01], vec![0x02], true)]
    #[case("lt",  vec![0x02], vec![0x01], false)]
    #[case("lt",  vec![0x01], vec![0x01], false)]
    #[case("lte", vec![0x01], vec![0x02], true)]
    #[case("lte", vec![0x01], vec![0x01], true)]
    #[case("lte", vec![0x02], vec![0x01], false)]
    fn test_bytes_comparison(#[case] op: &str, #[case] lhs: Vec<u8>, #[case] rhs: Vec<u8>, #[case] expected: bool) {
        let ident = Ident::new(op);
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(lhs), RuntimeValue::Bytes(rhs)],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::Boolean(expected)));
    }

    #[test]
    fn test_utf8_valid() {
        let ident = Ident::new("utf8");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(b"hello".to_vec())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::String("hello".to_string())));
    }

    #[test]
    fn test_utf8_invalid() {
        let ident = Ident::new("utf8");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(vec![0xff, 0xfe])],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_xor_basic() {
        let ident = Ident::new("xor");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![
                RuntimeValue::Bytes(vec![0xaa, 0xbb]),
                RuntimeValue::Bytes(vec![0x55, 0x44]),
            ],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::Bytes(vec![0xff, 0xff])));
    }

    #[test]
    fn test_xor_identity() {
        let ident = Ident::new("xor");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![
                RuntimeValue::Bytes(vec![0x01, 0x02, 0x03]),
                RuntimeValue::Bytes(vec![0x00, 0x00, 0x00]),
            ],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::Bytes(vec![0x01, 0x02, 0x03])));
    }

    #[test]
    fn test_xor_length_mismatch() {
        let ident = Ident::new("xor");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::Bytes(vec![0x01, 0x02]), RuntimeValue::Bytes(vec![0x01])],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
    }

    #[rstest]
    #[case::simple(
        "<root>hello</root>",
        {
            let mut root = BTreeMap::new();
            root.insert(Ident::new("tag"), RuntimeValue::String("root".to_string()));
            root.insert(Ident::new("attributes"), RuntimeValue::new_dict());
            root.insert(Ident::new("children"), RuntimeValue::EMPTY_ARRAY);
            root.insert(Ident::new("text"), RuntimeValue::String("hello".to_string()));
            Ok(RuntimeValue::Dict(root))
        }
    )]
    #[case::with_attributes(
        "<root id=\"1\" class=\"main\">hello</root>",
        {
            let mut root = BTreeMap::new();
            let mut attrs = BTreeMap::new();
            attrs.insert(Ident::new("id"), RuntimeValue::String("1".to_string()));
            attrs.insert(Ident::new("class"), RuntimeValue::String("main".to_string()));
            root.insert(Ident::new("tag"), RuntimeValue::String("root".to_string()));
            root.insert(Ident::new("attributes"), RuntimeValue::Dict(attrs));
            root.insert(Ident::new("children"), RuntimeValue::EMPTY_ARRAY);
            root.insert(Ident::new("text"), RuntimeValue::String("hello".to_string()));
            Ok(RuntimeValue::Dict(root))
        }
    )]
    #[case::nested(
        "<root><child id=\"1\">hello</child><child id=\"2\">world</child></root>",
        {
            let mut root = BTreeMap::new();
            let mut child1 = BTreeMap::new();
            let mut attrs1 = BTreeMap::new();
            attrs1.insert(Ident::new("id"), RuntimeValue::String("1".to_string()));
            child1.insert(Ident::new("tag"), RuntimeValue::String("child".to_string()));
            child1.insert(Ident::new("attributes"), RuntimeValue::Dict(attrs1));
            child1.insert(Ident::new("children"), RuntimeValue::EMPTY_ARRAY);
            child1.insert(Ident::new("text"), RuntimeValue::String("hello".to_string()));

            let mut child2 = BTreeMap::new();
            let mut attrs2 = BTreeMap::new();
            attrs2.insert(Ident::new("id"), RuntimeValue::String("2".to_string()));
            child2.insert(Ident::new("tag"), RuntimeValue::String("child".to_string()));
            child2.insert(Ident::new("attributes"), RuntimeValue::Dict(attrs2));
            child2.insert(Ident::new("children"), RuntimeValue::EMPTY_ARRAY);
            child2.insert(Ident::new("text"), RuntimeValue::String("world".to_string()));

            root.insert(Ident::new("tag"), RuntimeValue::String("root".to_string()));
            root.insert(Ident::new("attributes"), RuntimeValue::new_dict());
            root.insert(Ident::new("children"), RuntimeValue::Array(vec![
                RuntimeValue::Dict(child1),
                RuntimeValue::Dict(child2),
            ]));
            root.insert(Ident::new("text"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(root))
        }
    )]
    #[case::self_closing(
        "<root><child id=\"1\"/></root>",
        {
            let mut root = BTreeMap::new();
            let mut child = BTreeMap::new();
            let mut attrs = BTreeMap::new();
            attrs.insert(Ident::new("id"), RuntimeValue::String("1".to_string()));
            child.insert(Ident::new("tag"), RuntimeValue::String("child".to_string()));
            child.insert(Ident::new("attributes"), RuntimeValue::Dict(attrs));
            child.insert(Ident::new("children"), RuntimeValue::EMPTY_ARRAY);
            child.insert(Ident::new("text"), RuntimeValue::NONE);

            root.insert(Ident::new("tag"), RuntimeValue::String("root".to_string()));
            root.insert(Ident::new("attributes"), RuntimeValue::new_dict());
            root.insert(Ident::new("children"), RuntimeValue::Array(vec![
                RuntimeValue::Dict(child),
            ]));
            root.insert(Ident::new("text"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(root))
        }
    )]
    fn test_xml_parse(#[case] xml: &str, #[case] expected: Result<RuntimeValue, Error>) {
        let ident = Ident::new("_xml_parse");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String(xml.to_string())],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_diff_strings() {
        let ident = Ident::new("_diff");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![RuntimeValue::String("abc".into()), RuntimeValue::String("abc ".into())],
            &Shared::new(SharedCell::new(Env::default())),
        );

        assert!(result.is_ok());
        if let Ok(RuntimeValue::Array(changes)) = result {
            // line-level diff: delete "abc" + insert "abc " (replace pair)
            assert_eq!(changes.len(), 2);
            if let RuntimeValue::Dict(ref m) = changes[0] {
                assert_eq!(m.get(&Ident::new("tag")), Some(&RuntimeValue::String("delete".into())));
                assert_eq!(m.get(&Ident::new("value")), Some(&RuntimeValue::String("abc".into())));
                assert!(m.contains_key(&Ident::new("inline")));
            } else {
                panic!("Expected Dict change");
            }
            if let RuntimeValue::Dict(ref m) = changes[1] {
                assert_eq!(m.get(&Ident::new("tag")), Some(&RuntimeValue::String("insert".into())));
                assert_eq!(m.get(&Ident::new("value")), Some(&RuntimeValue::String("abc ".into())));
                // inline should show the trailing space as "insert"
                if let Some(RuntimeValue::Array(inline)) = m.get(&Ident::new("inline")) {
                    let last = inline.last().expect("inline should not be empty");
                    if let RuntimeValue::Dict(lm) = last {
                        assert_eq!(lm.get(&Ident::new("tag")), Some(&RuntimeValue::String("insert".into())));
                        assert_eq!(lm.get(&Ident::new("value")), Some(&RuntimeValue::String(" ".into())));
                    } else {
                        panic!("Expected Dict in inline");
                    }
                } else {
                    panic!("Expected inline Array");
                }
            } else {
                panic!("Expected Dict change");
            }
        } else {
            panic!("Expected Array result");
        }
    }

    #[test]
    fn test_diff_arrays() {
        let ident = Ident::new("_diff");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![
                RuntimeValue::Array(vec![RuntimeValue::Number(1.into())]),
                RuntimeValue::Array(vec![RuntimeValue::Number(2.into())]),
            ],
            &Shared::new(SharedCell::new(Env::default())),
        );

        assert!(result.is_ok());
        if let Ok(RuntimeValue::Array(changes)) = result {
            assert_eq!(changes.len(), 2); // delete 1, insert 2
            if let RuntimeValue::Dict(ref m) = changes[0] {
                assert_eq!(m.get(&Ident::new("tag")), Some(&RuntimeValue::String("delete".into())));
                assert_eq!(m.get(&Ident::new("value")), Some(&RuntimeValue::Number(1.into())));
                // non-string elements have no inline field
                assert!(!m.contains_key(&Ident::new("inline")));
            } else {
                panic!("Expected Dict change");
            }
            if let RuntimeValue::Dict(ref m) = changes[1] {
                assert_eq!(m.get(&Ident::new("tag")), Some(&RuntimeValue::String("insert".into())));
                assert_eq!(m.get(&Ident::new("value")), Some(&RuntimeValue::Number(2.into())));
                assert!(!m.contains_key(&Ident::new("inline")));
            } else {
                panic!("Expected Dict change");
            }
        } else {
            panic!("Expected Array result");
        }
    }

    #[rstest]
    #[case::single_number(vec![RuntimeValue::Number(1.into())], vec![1u8])]
    #[case::multiple_numbers(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())], vec![1u8, 2u8])]
    #[case::number_array(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])], vec![1u8, 2u8])]
    #[case::empty(vec![], vec![])]
    #[case::ignores_strings(vec![RuntimeValue::String("x".into())], vec![])]
    fn test_collect_depth_values(#[case] args: Vec<RuntimeValue>, #[case] expected: Vec<u8>) {
        assert_eq!(collect_depth_values(&args), expected);
    }

    #[rstest]
    #[case::single_string(vec![RuntimeValue::String("rust".into())], vec!["rust".to_string()])]
    #[case::multiple_strings(vec![RuntimeValue::String("rust".into()), RuntimeValue::String("go".into())], vec!["rust".to_string(), "go".to_string()])]
    #[case::string_array(vec![RuntimeValue::Array(vec![RuntimeValue::String("rust".into()), RuntimeValue::String("go".into())])], vec!["rust".to_string(), "go".to_string()])]
    #[case::empty(vec![], vec![])]
    #[case::ignores_numbers(vec![RuntimeValue::Number(1.into())], vec![])]
    fn test_collect_string_values(#[case] args: Vec<RuntimeValue>, #[case] expected: Vec<String>) {
        assert_eq!(collect_string_values(&args), expected);
    }

    #[rstest]
    #[case::heading_depth_match(
        Node::Heading(mq_markdown::Heading { depth: 1, values: vec![], position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into())],
        true
    )]
    #[case::heading_depth_no_match(
        Node::Heading(mq_markdown::Heading { depth: 2, values: vec![], position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
    #[case::heading_multi_depth_match(
        Node::Heading(mq_markdown::Heading { depth: 2, values: vec![], position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        true
    )]
    #[case::heading_no_args_fallback(
        Node::Heading(mq_markdown::Heading { depth: 1, values: vec![], position: None }),
        Selector::Heading(None),
        vec![],
        true
    )]
    #[case::code_lang_match(
        Node::Code(mq_markdown::Code { lang: Some("rust".to_string()), meta: None, value: "fn main() {}".to_string(), fence: true, position: None }),
        Selector::Code,
        vec![RuntimeValue::String("rust".into())],
        true
    )]
    #[case::code_lang_no_match(
        Node::Code(mq_markdown::Code { lang: Some("python".to_string()), meta: None, value: "pass".to_string(), fence: true, position: None }),
        Selector::Code,
        vec![RuntimeValue::String("rust".into())],
        false
    )]
    #[case::code_no_args_fallback(
        Node::Code(mq_markdown::Code { lang: None, meta: None, value: "".to_string(), fence: true, position: None }),
        Selector::Code,
        vec![],
        true
    )]
    #[case::non_heading_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
    #[case::list_index_match(
        Node::List(mq_markdown::List { index: 2, level: 0, checked: None, ordered: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(2.into())],
        true
    )]
    #[case::list_index_no_match(
        Node::List(mq_markdown::List { index: 0, level: 0, checked: None, ordered: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
    #[case::list_multi_index_match(
        Node::List(mq_markdown::List { index: 3, level: 0, checked: None, ordered: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into())],
        true
    )]
    #[case::list_no_args_fallback(
        Node::List(mq_markdown::List { index: 0, level: 0, checked: None, ordered: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![],
        true
    )]
    #[case::list_non_list_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(0.into())],
        false
    )]
    #[case::table_row_match(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 1, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(1.into())],
        true
    )]
    #[case::table_row_no_match(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 0, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
    #[case::table_row_and_col_match(
        Node::TableCell(mq_markdown::TableCell { column: 2, row: 1, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        true
    )]
    #[case::table_row_and_col_no_match(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 1, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        false
    )]
    #[case::table_no_args_fallback(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 0, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![],
        true
    )]
    #[case::table_non_table_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(0.into())],
        false
    )]
    fn test_eval_selector_with_args(
        #[case] node: Node,
        #[case] selector: Selector,
        #[case] args: Vec<RuntimeValue>,
        #[case] expected_match: bool,
    ) {
        let result = eval_selector_with_args(&node, &selector, &args);
        assert_eq!(!result.is_none(), expected_match);
    }

    fn env() -> Shared<SharedCell<Env>> {
        Shared::new(SharedCell::new(Env::default()))
    }

    fn call(name: &str, args: Vec<RuntimeValue>) -> Result<RuntimeValue, Error> {
        eval_builtin(&RuntimeValue::None, &Ident::new(name), args, &env())
    }

    // =========================================================================
    // band
    // =========================================================================

    #[rstest]
    #[case(vec![0xff, 0xff], vec![0xff, 0xff], vec![0xff, 0xff])]
    #[case(vec![0xf0, 0x0f], vec![0xff, 0xff], vec![0xf0, 0x0f])]
    #[case(vec![0xaa, 0x55], vec![0x55, 0xaa], vec![0x00, 0x00])]
    #[case(vec![0xff],       vec![0x00],       vec![0x00])]
    #[case(vec![],           vec![],           vec![])]
    fn test_band(#[case] b1: Vec<u8>, #[case] b2: Vec<u8>, #[case] expected: Vec<u8>) {
        assert_eq!(
            call("band", vec![RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)]),
            Ok(RuntimeValue::Bytes(expected))
        );
    }

    #[rstest]
    #[case(vec![0x01, 0x02], vec![0x01])]
    #[case(vec![],           vec![0x00])]
    fn test_band_length_mismatch(#[case] b1: Vec<u8>, #[case] b2: Vec<u8>) {
        assert!(call("band", vec![RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)]).is_err());
    }

    #[test]
    fn test_band_type_error() {
        assert!(
            call(
                "band",
                vec![RuntimeValue::String("a".into()), RuntimeValue::Bytes(vec![0x01])]
            )
            .is_err()
        );
    }

    // =========================================================================
    // bor
    // =========================================================================

    #[rstest]
    #[case(vec![0x00, 0x00], vec![0x00, 0x00], vec![0x00, 0x00])]
    #[case(vec![0xf0, 0x00], vec![0x0f, 0x00], vec![0xff, 0x00])]
    #[case(vec![0xaa, 0x55], vec![0x55, 0xaa], vec![0xff, 0xff])]
    #[case(vec![0x00],       vec![0xff],       vec![0xff])]
    #[case(vec![],           vec![],           vec![])]
    fn test_bor(#[case] b1: Vec<u8>, #[case] b2: Vec<u8>, #[case] expected: Vec<u8>) {
        assert_eq!(
            call("bor", vec![RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)]),
            Ok(RuntimeValue::Bytes(expected))
        );
    }

    #[rstest]
    #[case(vec![0x01, 0x02], vec![0x01])]
    #[case(vec![],           vec![0x00])]
    fn test_bor_length_mismatch(#[case] b1: Vec<u8>, #[case] b2: Vec<u8>) {
        assert!(call("bor", vec![RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)]).is_err());
    }

    #[test]
    fn test_bor_type_error() {
        assert!(
            call(
                "bor",
                vec![RuntimeValue::Number(1.into()), RuntimeValue::Bytes(vec![0x01])]
            )
            .is_err()
        );
    }

    // =========================================================================
    // bnot
    // =========================================================================

    #[rstest]
    #[case(vec![0x00],       vec![0xff])]
    #[case(vec![0xff],       vec![0x00])]
    #[case(vec![0xf0, 0x0f], vec![0x0f, 0xf0])]
    #[case(vec![0x55, 0xaa], vec![0xaa, 0x55])]
    #[case(vec![],           vec![])]
    fn test_bnot(#[case] input: Vec<u8>, #[case] expected: Vec<u8>) {
        assert_eq!(
            call("bnot", vec![RuntimeValue::Bytes(input)]),
            Ok(RuntimeValue::Bytes(expected))
        );
    }

    #[test]
    fn test_bnot_double_negation() {
        let original = vec![0xde, 0xad, 0xbe, 0xef];
        let once = call("bnot", vec![RuntimeValue::Bytes(original.clone())]).unwrap();
        let twice = call("bnot", vec![once]).unwrap();
        assert_eq!(twice, RuntimeValue::Bytes(original));
    }

    #[test]
    fn test_bnot_type_error() {
        assert!(call("bnot", vec![RuntimeValue::String("a".into())]).is_err());
    }

    // =========================================================================
    // starts_with / ends_with for bytes
    // =========================================================================

    #[rstest]
    #[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02],             true)]
    #[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02, 0x03],       true)]
    #[case(vec![0x01, 0x02, 0x03], vec![0x02, 0x03],             false)]
    #[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02, 0x03, 0x04], false)]
    #[case(vec![0x01],             vec![],                        true)]
    #[case(vec![],                 vec![],                        true)]
    fn test_bytes_starts_with(#[case] haystack: Vec<u8>, #[case] prefix: Vec<u8>, #[case] expected: bool) {
        assert_eq!(
            call(
                "starts_with",
                vec![RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(prefix)]
            ),
            Ok(RuntimeValue::Boolean(expected))
        );
    }

    #[rstest]
    #[case(vec![0x01, 0x02, 0x03], vec![0x02, 0x03],             true)]
    #[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02, 0x03],       true)]
    #[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02],             false)]
    #[case(vec![0x01, 0x02, 0x03], vec![0x00, 0x01, 0x02, 0x03], false)]
    #[case(vec![0x01],             vec![],                        true)]
    #[case(vec![],                 vec![],                        true)]
    fn test_bytes_ends_with(#[case] haystack: Vec<u8>, #[case] suffix: Vec<u8>, #[case] expected: bool) {
        assert_eq!(
            call(
                "ends_with",
                vec![RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(suffix)]
            ),
            Ok(RuntimeValue::Boolean(expected))
        );
    }

    // =========================================================================
    // index / rindex for bytes
    // =========================================================================

    #[rstest]
    #[case(vec![0x01, 0x02, 0x03, 0x02],     vec![0x02],       1)]
    #[case(vec![0x01, 0x02, 0x03],           vec![0x04],       -1)]
    #[case(vec![0x01, 0x02, 0x03],           vec![0x01, 0x02], 0)]
    #[case(vec![0x01, 0x02, 0x03],           vec![0x02, 0x03], 1)]
    #[case(vec![0x01, 0x02, 0x03],           vec![0x01, 0x02, 0x03], 0)]
    #[case(vec![0x01, 0x02, 0x03],           vec![0x01, 0x02, 0x03, 0x04], -1)]
    #[case(vec![],                           vec![0x01],       -1)]
    fn test_bytes_index(#[case] haystack: Vec<u8>, #[case] needle: Vec<u8>, #[case] expected: i64) {
        assert_eq!(
            call(
                "index",
                vec![RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(needle)]
            ),
            Ok(RuntimeValue::Number(expected.into()))
        );
    }

    #[rstest]
    #[case(vec![0x01, 0x02, 0x03, 0x02],     vec![0x02],       3)]
    #[case(vec![0x01, 0x02, 0x03],           vec![0x04],       -1)]
    #[case(vec![0x01, 0x02, 0x03, 0x01, 0x02], vec![0x01, 0x02], 3)]
    #[case(vec![0x01, 0x02, 0x03],           vec![0x01, 0x02, 0x03], 0)]
    #[case(vec![],                           vec![0x01],       -1)]
    fn test_bytes_rindex(#[case] haystack: Vec<u8>, #[case] needle: Vec<u8>, #[case] expected: i64) {
        assert_eq!(
            call(
                "rindex",
                vec![RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(needle)]
            ),
            Ok(RuntimeValue::Number(expected.into()))
        );
    }

    #[test]
    fn test_bytes_index_rindex_agree_single_occurrence() {
        let h = vec![0xaa, 0xbb, 0xcc];
        let n = vec![0xbb];
        let idx = call(
            "index",
            vec![RuntimeValue::Bytes(h.clone()), RuntimeValue::Bytes(n.clone())],
        )
        .unwrap();
        let ridx = call("rindex", vec![RuntimeValue::Bytes(h), RuntimeValue::Bytes(n)]).unwrap();
        assert_eq!(idx, ridx);
    }

    // =========================================================================
    // repeat for bytes
    // =========================================================================

    #[rstest]
    #[case(vec![0x01, 0x02], 0, vec![])]
    #[case(vec![0x01, 0x02], 1, vec![0x01, 0x02])]
    #[case(vec![0x01, 0x02], 3, vec![0x01, 0x02, 0x01, 0x02, 0x01, 0x02])]
    #[case(vec![0xff],       4, vec![0xff, 0xff, 0xff, 0xff])]
    #[case(vec![],           5, vec![])]
    fn test_bytes_repeat(#[case] input: Vec<u8>, #[case] n: u32, #[case] expected: Vec<u8>) {
        assert_eq!(
            call(
                "repeat",
                vec![RuntimeValue::Bytes(input), RuntimeValue::Number((n as f64).into())]
            ),
            Ok(RuntimeValue::Bytes(expected))
        );
    }

    // =========================================================================
    // pack
    // =========================================================================

    #[rstest]
    #[case("u8",    0.0,    vec![0x00])]
    #[case("u8",    255.0,  vec![0xff])]
    #[case("i8",    -1.0,   vec![0xff])]
    #[case("i8",    -128.0, vec![0x80])]
    #[case("i8",    127.0,  vec![0x7f])]
    #[case("u16be", 256.0,  vec![0x01, 0x00])]
    #[case("u16le", 256.0,  vec![0x00, 0x01])]
    #[case("i16be", -1.0,   vec![0xff, 0xff])]
    #[case("i16le", -1.0,   vec![0xff, 0xff])]
    #[case("u32be", 1.0,    vec![0x00, 0x00, 0x00, 0x01])]
    #[case("u32le", 1.0,    vec![0x01, 0x00, 0x00, 0x00])]
    #[case("i32be", -1.0,   vec![0xff, 0xff, 0xff, 0xff])]
    #[case("i32le", -1.0,   vec![0xff, 0xff, 0xff, 0xff])]
    #[case("u64be", 1.0,    vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01])]
    #[case("u64le", 1.0,    vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case("i64be", -1.0,   vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff])]
    #[case("f32be", 1.0,    vec![0x3f, 0x80, 0x00, 0x00])]
    #[case("f32le", 1.0,    vec![0x00, 0x00, 0x80, 0x3f])]
    #[case("f64be", 1.0,    vec![0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case("f64le", 1.0,    vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f])]
    fn test_pack(#[case] fmt: &str, #[case] value: f64, #[case] expected: Vec<u8>) {
        assert_eq!(
            call(
                "pack",
                vec![RuntimeValue::String(fmt.into()), RuntimeValue::Number(value.into())]
            ),
            Ok(RuntimeValue::Bytes(expected))
        );
    }

    #[rstest]
    #[case("z99")]
    #[case("u16")]
    #[case("")]
    fn test_pack_unknown_format(#[case] fmt: &str) {
        assert!(
            call(
                "pack",
                vec![RuntimeValue::String(fmt.into()), RuntimeValue::Number(0.0.into())]
            )
            .is_err()
        );
    }

    #[test]
    fn test_pack_type_error() {
        assert!(
            call(
                "pack",
                vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(0.0.into())]
            )
            .is_err()
        );
    }

    // =========================================================================
    // unpack
    // =========================================================================

    #[rstest]
    #[case("u8",    vec![0x2a],                                               42.0)]
    #[case("i8",    vec![0xff],                                               -1.0)]
    #[case("u16be", vec![0x01, 0x00],                                         256.0)]
    #[case("u16le", vec![0x00, 0x01],                                         256.0)]
    #[case("i16be", vec![0xff, 0xff],                                         -1.0)]
    #[case("i16le", vec![0xff, 0xff],                                         -1.0)]
    #[case("u32be", vec![0x00, 0x00, 0x00, 0x01],                             1.0)]
    #[case("u32le", vec![0x01, 0x00, 0x00, 0x00],                             1.0)]
    #[case("i32be", vec![0xff, 0xff, 0xff, 0xff],                             -1.0)]
    #[case("i32le", vec![0xff, 0xff, 0xff, 0xff],                             -1.0)]
    #[case("u64be", vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],    1.0)]
    #[case("u64le", vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],    1.0)]
    #[case("i64be", vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],    -1.0)]
    #[case("f32be", vec![0x3f, 0x80, 0x00, 0x00],                             1.0)]
    #[case("f32le", vec![0x00, 0x00, 0x80, 0x3f],                             1.0)]
    #[case("f64be", vec![0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],    1.0)]
    #[case("f64le", vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f],    1.0)]
    fn test_unpack(#[case] fmt: &str, #[case] bytes: Vec<u8>, #[case] expected: f64) {
        assert_eq!(
            call(
                "unpack",
                vec![RuntimeValue::String(fmt.into()), RuntimeValue::Bytes(bytes)]
            ),
            Ok(RuntimeValue::Number(expected.into()))
        );
    }

    #[rstest]
    #[case("u8",    vec![])]
    #[case("u16be", vec![0x00])]
    #[case("u32be", vec![0x00, 0x00, 0x00])]
    #[case("u64be", vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    #[case("f32be", vec![0x00, 0x00, 0x00])]
    #[case("f64be", vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
    fn test_unpack_too_short(#[case] fmt: &str, #[case] bytes: Vec<u8>) {
        assert!(
            call(
                "unpack",
                vec![RuntimeValue::String(fmt.into()), RuntimeValue::Bytes(bytes)]
            )
            .is_err()
        );
    }

    #[rstest]
    #[case("z99")]
    #[case("")]
    fn test_unpack_unknown_format(#[case] fmt: &str) {
        assert!(
            call(
                "unpack",
                vec![RuntimeValue::String(fmt.into()), RuntimeValue::Bytes(vec![0x00])]
            )
            .is_err()
        );
    }

    #[test]
    fn test_unpack_type_error() {
        assert!(
            call(
                "unpack",
                vec![RuntimeValue::Number(1.into()), RuntimeValue::Bytes(vec![0x00])]
            )
            .is_err()
        );
    }

    #[rstest]
    #[case("u8", 42.0)]
    #[case("i8",    -5.0)]
    #[case("u16be", 1234.0)]
    #[case("u16le", 1234.0)]
    #[case("i16be", -1000.0)]
    #[case("i16le", -1000.0)]
    #[case("u32be", 100000.0)]
    #[case("u32le", 100000.0)]
    #[case("i32be", -100000.0)]
    #[case("i32le", -100000.0)]
    #[case("u64be", 1000000.0)]
    #[case("u64le", 1000000.0)]
    #[case("i64be", -1000000.0)]
    #[case("i64le", -1000000.0)]
    #[case("f32be", 1.5)]
    #[case("f32le", 1.5)]
    #[case("f64be", 1.23456789)]
    #[case("f64le", 1.23456789)]
    fn test_pack_unpack_roundtrip(#[case] fmt: &str, #[case] value: f64) {
        let packed = call(
            "pack",
            vec![RuntimeValue::String(fmt.into()), RuntimeValue::Number(value.into())],
        )
        .unwrap();
        let result = call("unpack", vec![RuntimeValue::String(fmt.into()), packed]).unwrap();
        match result {
            RuntimeValue::Number(n) => assert!((n.value() - value).abs() < 1e-5),
            _ => panic!("expected Number"),
        }
    }

    #[cfg(feature = "file-io")]
    #[test]
    fn test_file_exists_with_existing_file() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
        tmp.write_all(b"hello").expect("failed to write");
        let path = tmp.path().to_string_lossy().to_string();
        assert_eq!(
            call("file_exists", vec![RuntimeValue::String(path)]),
            Ok(RuntimeValue::Boolean(true))
        );
    }

    #[cfg(feature = "file-io")]
    #[test]
    fn test_file_exists_with_nonexistent_file() {
        assert_eq!(
            call(
                "file_exists",
                vec![RuntimeValue::String("/nonexistent/path/no_such_file.md".into())]
            ),
            Ok(RuntimeValue::Boolean(false))
        );
    }

    #[cfg(feature = "file-io")]
    #[test]
    fn test_file_exists_invalid_type() {
        let result = call("file_exists", vec![RuntimeValue::Number(42.into())]);
        assert!(result.is_err());
    }
}
