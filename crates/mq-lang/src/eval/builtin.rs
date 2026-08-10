pub(super) mod bytes;
mod compress;
pub(super) mod convert;
#[cfg(feature = "css-selector")]
mod css;
pub(super) mod date;
mod gron;
#[cfg(feature = "http")]
mod http;
pub(crate) mod io_context;
pub(super) mod path;
mod random;
mod range;
mod regex;
pub(super) mod tokenizer;

use crate::arena::Arena;
use crate::ast::{constants, node as ast};
use crate::error::runtime::RuntimeError;
use crate::eval::builtin::convert::Convert;
use crate::eval::env::{self, Env};
use crate::ident::all_symbols;
#[cfg(feature = "http")]
use crate::io::HttpRequestSpec;
#[cfg(feature = "file-io")]
use crate::io::Io;
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
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use self::range::{generate_char_range, generate_multi_char_range, generate_numeric_range};
use self::regex::{capture_re, is_match_re, match_re, replace_re, scan_re, split_re};
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
            #[cfg(target_arch = "wasm32")]
            {
                web_sys::console::log_1(&a.to_string().into());
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                println!("{}", a);
            }
            Ok(current_value.clone())
        }
        _ => unreachable!("print should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "stderr", params = Fixed(1))]
fn stderr_impl(_: &Ident, current_value: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => {
            #[cfg(target_arch = "wasm32")]
            {
                web_sys::console::error_1(&a.to_string().into());
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                eprintln!("{}", a);
            }

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
    Ok(RuntimeValue::Array(Shared::new(args)))
}

#[mq_macros::mq_fn(name = "flatten", params = Fixed(1))]
fn flatten_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(arrays)] => Ok(convert::flatten(Shared::unwrap_or_clone(std::mem::take(arrays))).into()),
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
    RuntimeValue::Array(Shared::new(vec![
        RuntimeValue::Number(((dt.year()) as i64).into()),
        RuntimeValue::Number((dt.month0() as i64).into()),
        RuntimeValue::Number((dt.day() as i64).into()),
        RuntimeValue::Number((dt.hour() as i64).into()),
        RuntimeValue::Number((dt.minute() as i64).into()),
        RuntimeValue::Number((dt.second() as i64).into()),
        RuntimeValue::Number((dt.weekday().num_days_from_sunday() as i64).into()),
        RuntimeValue::Number((dt.ordinal0() as i64).into()),
    ]))
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

/// Parses a date string using the given strptime format and returns a Unix timestamp (seconds, UTC).
/// Formats without a time component (e.g. "%Y-%m-%d") default the time to midnight.
#[mq_macros::mq_fn(name = "strptime", params = Fixed(2))]
fn strptime_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(date_str), RuntimeValue::String(fmt)] => {
            let date_str = date_str.as_str();
            let fmt = fmt.as_str();

            chrono::NaiveDateTime::parse_from_str(date_str, fmt)
                .or_else(|e| {
                    chrono::NaiveDate::parse_from_str(date_str, fmt)
                        .map(|d| d.and_time(chrono::NaiveTime::MIN))
                        .map_err(|_| e)
                })
                .map(|dt| RuntimeValue::Number(dt.and_utc().timestamp().into()))
                .map_err(|e| Error::Runtime(format!("strptime: {}", e)))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("strptime should always receive exactly two arguments"),
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

/// Parses a natural-language relative date expression relative to a base Unix timestamp
/// (seconds) and returns the resulting Unix timestamp (seconds, UTC).
/// Supported forms: "now", "today", "yesterday", "tomorrow", "<n> <unit> ago",
/// "in <n> <unit>", "next <weekday>", "last <weekday>".
/// Units accept singular or plural: "second(s)", "minute(s)", "hour(s)", "day(s)", "week(s)", "month(s)", "year(s)".
/// The base timestamp comes first (like `to_date`/`strftime`) so it can flow through a pipe:
/// `now() | date_relative("3 days ago")`.
#[mq_macros::mq_fn(name = "date_relative", params = Fixed(2))]
fn date_relative_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(base), RuntimeValue::String(s)] => {
            let base_secs = base.value() as i64;
            let base_dt = DateTime::from_timestamp(base_secs, 0)
                .ok_or_else(|| Error::Runtime(format!("date_relative: invalid base timestamp: {}", base_secs)))?;
            date::parse_relative(s.as_str(), base_dt).map(|dt| RuntimeValue::Number(dt.timestamp().into()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("date_relative should always receive exactly two arguments"),
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

#[mq_macros::mq_fn(name = "uuid", params = None)]
fn uuid_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(uuid::Uuid::new_v4().to_string()))
}

#[mq_macros::mq_fn(name = "uuid_v7", params = None)]
fn uuid_v7_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(uuid::Uuid::now_v7().to_string()))
}

#[mq_macros::mq_fn(name = "uuid_v4", params = None)]
fn uuid_v4_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(uuid::Uuid::new_v4().to_string()))
}

/// Generates a pseudo-random `f64` in `[0, 1)`. Not cryptographically secure.
#[mq_macros::mq_fn(name = "rand", params = None)]
fn rand_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Number(random::next_f64().into()))
}

/// Generates a pseudo-random integer uniformly distributed in `[min, max]` (inclusive).
#[mq_macros::mq_fn(name = "rand_int", params = Fixed(2))]
fn rand_int_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(min), RuntimeValue::Number(max)] if min.is_int() && max.is_int() => {
            let (min_i, max_i) = (min.to_int(), max.to_int());
            random::next_range_i64(min_i, max_i)
                .map(|n| RuntimeValue::Number(n.into()))
                .ok_or_else(|| Error::Runtime(format!("rand_int: min ({min_i}) must be <= max ({max_i})")))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("rand_int should always receive exactly two arguments"),
    }
}

/// Returns a new array containing the same elements as `arr` in a uniformly random order.
#[mq_macros::mq_fn(name = "shuffle", params = Fixed(1))]
fn shuffle_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(arr)] => {
            let mut arr = std::mem::take(arr);
            random::shuffle(runtime_value::array_mut(&mut arr));
            Ok(RuntimeValue::Array(arr))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("shuffle should always receive exactly one argument"),
    }
}

/// Returns `n` elements sampled from `arr` without replacement, in random order.
#[mq_macros::mq_fn(name = "sample", params = Fixed(2))]
fn sample_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(arr), RuntimeValue::Number(n)] if n.is_int() && n.value() >= 0.0 => {
            let n = n.to_int() as usize;
            if n > arr.len() {
                return Err(Error::Runtime(format!(
                    "sample: n ({n}) must not exceed the array length ({})",
                    arr.len()
                )));
            }
            Ok(RuntimeValue::Array(Shared::new(random::sample(arr, n))))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("sample should always receive exactly two arguments"),
    }
}

/// Returns a random string of `len` characters, each independently chosen (with
/// replacement) from `charset`.
#[mq_macros::mq_fn(name = "random_string", params = Fixed(2))]
fn random_string_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(len), RuntimeValue::String(charset)] if len.is_int() && len.value() >= 0.0 => {
            let len = len.to_int() as usize;
            let charset: Vec<char> = charset.chars().collect();
            random::next_string(len, &charset)
                .map(RuntimeValue::String)
                .ok_or_else(|| Error::Runtime("random_string: charset must not be empty".to_string()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("random_string should always receive exactly two arguments"),
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
            Ok(RuntimeValue::Array(Shared::new(
                parse_markdown_input(&markdown)
                    .map_err(|e| Error::Runtime(format!("Failed to parse converted markdown: {}", e)))?,
            )))
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

#[mq_macros::mq_fn(name = "html_escape", params = Fixed(1))]
fn html_escape_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::html_escape(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::html_escape(md.value().as_str()).and_then(|o| match o {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("html_escape should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "html_unescape", params = Fixed(1))]
fn html_unescape_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::html_unescape(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::html_unescape(md.value().as_str()).and_then(|o| match o {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("html_unescape should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "sanitize_html", params = Fixed(1))]
fn sanitize_html_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::sanitize_html(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::sanitize_html(md.value().as_str()).and_then(|o| match o {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("sanitize_html should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "strip_tags", params = Fixed(1))]
fn strip_tags_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::strip_tags(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::strip_tags(md.value().as_str()).and_then(|o| match o {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("strip_tags should always receive exactly one argument"),
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

#[mq_macros::mq_fn(name = "to_boolean", params = Fixed(1))]
fn to_boolean_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    convert::to_boolean(&args[0])
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

#[mq_macros::mq_fn(name = "url_decode", params = Fixed(1))]
fn url_decode_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::url_decode(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| {
                convert::url_decode(md.value().as_str()).and_then(|o| match o {
                    RuntimeValue::String(s) => Ok(node.update_markdown_value(&s)),
                    a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                })
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [a] => convert::url_decode(&a.to_string()),
        _ => unreachable!("url_decode should always receive exactly one argument"),
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
            .unwrap_or_else(|| Ok(RuntimeValue::empty_array())),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::empty_array()),
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

#[mq_macros::mq_fn(name = "scan", params = Fixed(2))]
fn scan_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s), RuntimeValue::String(pattern)] => scan_re(s, pattern),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(pattern)] => node
            .markdown_node()
            .map(|md| scan_re(&md.value(), pattern))
            .unwrap_or_else(|| Ok(RuntimeValue::empty_array())),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::empty_array()),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("scan should always receive exactly two arguments"),
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

#[mq_macros::mq_fn(name = "ascii_downcase", params = Fixed(1))]
fn ascii_downcase_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().to_ascii_lowercase().as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::String(s)] => Ok(s.to_ascii_lowercase().into()),
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

#[mq_macros::mq_fn(name = "word_wrap", params = Fixed(2))]
fn word_wrap_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s), RuntimeValue::Number(width)] => Ok(word_wrap(s, width.value() as usize).into()),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::Number(width)] => {
            let width = width.value() as usize;
            node.markdown_node()
                .map(|md| Ok(node.update_markdown_value(&word_wrap(&md.value(), width))))
                .unwrap_or_else(|| Ok(RuntimeValue::NONE))
        }
        [RuntimeValue::None, RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("word_wrap should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "truncate", params = Fixed(3))]
fn truncate_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [
            RuntimeValue::String(s),
            RuntimeValue::Number(len),
            RuntimeValue::String(ellipsis),
        ] => Ok(truncate_str(s, len.value() as usize, ellipsis).into()),
        [
            node @ RuntimeValue::Markdown(_, _),
            RuntimeValue::Number(len),
            RuntimeValue::String(ellipsis),
        ] => {
            let len = len.value() as usize;
            node.markdown_node()
                .map(|md| Ok(node.update_markdown_value(&truncate_str(&md.value(), len, ellipsis))))
                .unwrap_or_else(|| Ok(RuntimeValue::NONE))
        }
        [RuntimeValue::None, RuntimeValue::Number(_), RuntimeValue::String(_)] => Ok(RuntimeValue::NONE),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!("truncate should always receive exactly three arguments"),
    }
}

#[mq_macros::mq_fn(name = "explode", params = Fixed(1))]
fn explode_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(Shared::new(
            s.chars()
                .map(|c| RuntimeValue::Number((c as u32).into()))
                .collect::<Vec<_>>(),
        ))),
        [node @ RuntimeValue::Markdown(_, _)] => Ok(RuntimeValue::Array(Shared::new(
            node.markdown_node()
                .map(|md| {
                    md.value()
                        .chars()
                        .map(|c| RuntimeValue::Number((c as u32).into()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        ))),
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

#[mq_macros::mq_fn(name = "ascii_upcase", params = Fixed(1))]
fn ascii_upcase_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().to_ascii_uppercase().as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::String(s)] => Ok(s.to_ascii_uppercase().into()),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("ascii_upcase should always receive exactly one argument"),
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
                return Ok(RuntimeValue::empty_array());
            }

            Ok(RuntimeValue::Array(Shared::new(arrays[real_start..real_end].to_vec())))
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

/// Counts (or, with the `tiktoken` Cargo feature, exactly counts) the LLM tokens `text` would
/// consume. `model` is optional: without it, `text` is always run through the heuristic
/// estimate, regardless of the `tiktoken` feature. See [`tokenizer`] for the heuristic/exact
/// two-tier design.
#[mq_macros::mq_fn(name = "token_count", params = Range(1, 2))]
fn token_count_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(text)] => Ok(RuntimeValue::Number(tokenizer::token_count_estimate(text).into())),
        [node @ RuntimeValue::Markdown(_, _)] => Ok(RuntimeValue::Number(
            node.markdown_node()
                .map(|md| tokenizer::token_count_estimate(md.value().as_str()))
                .unwrap_or(0)
                .into(),
        )),
        [RuntimeValue::None] => Ok(RuntimeValue::Number(0.into())),
        [RuntimeValue::String(text), RuntimeValue::String(model)] => {
            tokenizer::token_count(text, model).map(|n| RuntimeValue::Number(n.into()))
        }
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(model)] => node
            .markdown_node()
            .map(|md| tokenizer::token_count(md.value().as_str(), model).map(|n| RuntimeValue::Number(n.into())))
            .unwrap_or(Ok(RuntimeValue::Number(0.into()))),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::Number(0.into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("token_count should always receive one or two arguments"),
    }
}

/// Reduces an array of Markdown nodes to fit within `budget` tokens. See
/// [`compress::token_compress`] for the staged algorithm; `model` is optional and behaves like
/// in `token_count`.
#[mq_macros::mq_fn(name = "token_compress", params = Range(2, 3))]
fn token_compress_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    fn compress_nodes(
        nodes: &mut Shared<Vec<RuntimeValue>>,
        budget: &number::Number,
        model: Option<&str>,
    ) -> Result<RuntimeValue, Error> {
        let vec = std::mem::take(nodes);
        let markdown_nodes: Vec<mq_markdown::Node> = Shared::unwrap_or_clone(vec)
            .into_iter()
            .filter_map(|v| v.markdown_node())
            .collect();
        let budget = budget.value().max(0.0) as usize;
        let counter = tokenizer::counter(model)?;
        let compressed = compress::token_compress(markdown_nodes, budget, counter.as_ref());

        Ok(RuntimeValue::Array(Shared::new(
            compressed
                .into_iter()
                .map(|node| RuntimeValue::Markdown(Box::new(node), None))
                .collect(),
        )))
    }

    match args.as_mut_slice() {
        [RuntimeValue::Array(nodes), RuntimeValue::Number(budget)] => compress_nodes(nodes, budget, None),
        [
            RuntimeValue::Array(nodes),
            RuntimeValue::Number(budget),
            RuntimeValue::String(model),
        ] => compress_nodes(nodes, budget, Some(model)),
        [RuntimeValue::None, RuntimeValue::Number(_)] => Ok(RuntimeValue::Array(Shared::new(Vec::new()))),
        [RuntimeValue::None, RuntimeValue::Number(_), RuntimeValue::String(_)] => {
            Ok(RuntimeValue::Array(Shared::new(Vec::new())))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => unreachable!("token_compress should always receive two or three arguments"),
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
            generate_numeric_range(0, end_val, 1).map(|v| RuntimeValue::Array(Shared::new(v)))
        }
        // Numeric range: range(start, end)
        [RuntimeValue::Number(start), RuntimeValue::Number(end)] => {
            let start_val = start.value() as isize;
            let end_val = end.value() as isize;
            let step = if start_val <= end_val { 1 } else { -1 };
            generate_numeric_range(start_val, end_val, step).map(|v| RuntimeValue::Array(Shared::new(v)))
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
            generate_numeric_range(start_val, end_val, step_val).map(|v| RuntimeValue::Array(Shared::new(v)))
        }
        // String range: range("a", "z") or range("A", "Z") or range("aa", "zz")
        [RuntimeValue::String(start), RuntimeValue::String(end)] => {
            let start_chars: Vec<char> = start.chars().collect();
            let end_chars: Vec<char> = end.chars().collect();

            if start_chars.len() == 1 && end_chars.len() == 1 {
                generate_char_range(start_chars[0], end_chars[0], None).map(|v| RuntimeValue::Array(Shared::new(v)))
            } else {
                generate_multi_char_range(start, end).map(|v| RuntimeValue::Array(Shared::new(v)))
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
                    .map(|v| RuntimeValue::Array(Shared::new(v)))
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
            runtime_value::array_mut(&mut array).remove(n.value() as usize);
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
            runtime_value::dict_mut(&mut dict).remove(&Ident::new(key));
            Ok(RuntimeValue::Dict(dict))
        }
        [RuntimeValue::Dict(dict), RuntimeValue::Symbol(key)] => {
            let mut dict = std::mem::take(dict);
            runtime_value::dict_mut(&mut dict).remove(key);
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
            runtime_value::array_mut(&mut vec).reverse();
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
            runtime_value::array_mut(&mut vec).sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let vec = Shared::unwrap_or_clone(vec)
                .into_iter()
                .map(|v| match v {
                    RuntimeValue::Markdown(mut node, s) => {
                        node.set_position(None);
                        RuntimeValue::Markdown(node, s)
                    }
                    _ => v,
                })
                .collect();
            Ok(RuntimeValue::Array(Shared::new(vec)))
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
            runtime_value::array_mut(&mut vec).sort_by(|a, b| match (a, b) {
                (RuntimeValue::Array(a1), RuntimeValue::Array(a2)) => a1
                    .first()
                    .unwrap()
                    .partial_cmp(a2.first().unwrap())
                    .unwrap_or(std::cmp::Ordering::Equal),
                _ => unreachable!("_sort_by_impl should only be called with an array of arrays"),
            });
            let vec = Shared::unwrap_or_clone(vec)
                .into_iter()
                .map(|v| match v {
                    RuntimeValue::Array(mut arr) if arr.len() >= 2 => {
                        if let RuntimeValue::Markdown(node, s) = &arr[1] {
                            let mut new_node = node.clone();
                            new_node.set_position(None);

                            runtime_value::array_mut(&mut arr)[1] = RuntimeValue::Markdown(new_node, s.clone());
                            RuntimeValue::Array(arr)
                        } else {
                            RuntimeValue::Array(arr)
                        }
                    }
                    _ => unreachable!("_sort_by_impl should only be called with an array of arrays"),
                })
                .collect();

            Ok(RuntimeValue::Array(Shared::new(vec)))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_sort_by_impl should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "compact", params = Fixed(1))]
fn compact_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(Shared::new(
            Shared::unwrap_or_clone(std::mem::take(array))
                .into_iter()
                .filter(|v| !v.is_none())
                .collect::<Vec<_>>(),
        ))),
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
                return Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::empty_array()])));
            }

            let mut positions = Vec::new();
            for (i, a) in array.iter().enumerate() {
                if a == v {
                    positions.push(i);
                }
            }

            if positions.is_empty() {
                return Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Array(
                    std::mem::take(array),
                )])));
            }

            let mut result = Vec::with_capacity(positions.len() + 1);
            let mut start = 0;

            for pos in positions {
                result.push(RuntimeValue::Array(Shared::new(array[start..pos].to_vec())));
                start = pos + 1;
            }

            if start < array.len() {
                result.push(RuntimeValue::Array(Shared::new(array[start..].to_vec())));
            }

            Ok(RuntimeValue::Array(Shared::new(result)))
        }
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::empty_array()),
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
            runtime_value::array_mut(&mut vec).retain(|item| seen.insert(item.to_string()));
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
            let a_mut = runtime_value::array_mut(&mut a);
            a_mut.reserve(a2.len());
            a_mut.extend_from_slice(a2);
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
            let a_mut = runtime_value::array_mut(&mut a);
            a_mut.reserve(1);
            a_mut.push(std::mem::take(a2));
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
            arr.extend(Shared::unwrap_or_clone(std::mem::take(a)));

            Ok(RuntimeValue::Array(Shared::new(arr)))
        }
        [RuntimeValue::Dict(d1), RuntimeValue::Dict(d2)] => {
            let mut result = std::mem::take(d1);
            runtime_value::dict_mut(&mut result).extend(Shared::unwrap_or_clone(std::mem::take(d2)));
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
                let result: Result<Vec<RuntimeValue>, Error> = Shared::unwrap_or_clone(std::mem::take(array))
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
                result.map(|v| RuntimeValue::Array(Shared::new(v)))
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
        [RuntimeValue::Array(nodes), RuntimeValue::String(attr)] => Ok(runtime_value::array_mut(nodes)
            .iter_mut()
            .flat_map(|node| match node {
                RuntimeValue::Markdown(node, _) => {
                    let value = node.attr(attr).map(Into::into).unwrap_or(RuntimeValue::NONE);

                    match value {
                        RuntimeValue::Array(arr) => Shared::unwrap_or_clone(arr),
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

#[mq_macros::mq_fn(name = "set_children", params = Fixed(2))]
fn set_children_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, selector), RuntimeValue::Array(children)] => {
            let mut new_node = std::mem::take(node);
            let children = runtime_value::array_mut(children)
                .iter_mut()
                .map(|child| match child {
                    RuntimeValue::Markdown(node, _) => (**node).clone(),
                    value => std::mem::take(value).to_string().into(),
                })
                .collect();
            new_node.set_children(children);
            Ok(RuntimeValue::Markdown(new_node, selector.take()))
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => unreachable!("set_children should always receive at least two arguments"),
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

#[mq_macros::mq_fn(name = "to_blockquote", params = Fixed(1))]
fn to_blockquote_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Markdown(node, _)] => Ok(mq_markdown::Node::Blockquote(mq_markdown::Blockquote {
            values: node.node_values(),
            position: None,
        })
        .into()),
        [a] if !a.is_none() => Ok(mq_markdown::Node::Blockquote(mq_markdown::Blockquote {
            values: vec![a.to_string().into()],
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_delete", params = Fixed(1))]
fn to_delete_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Markdown(node, _)] => Ok(mq_markdown::Node::Delete(mq_markdown::Delete {
            values: node.node_values(),
            position: None,
        })
        .into()),
        [a] if !a.is_none() => Ok(mq_markdown::Node::Delete(mq_markdown::Delete {
            values: vec![a.to_string().into()],
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_callout", params = Fixed(3))]
fn to_callout_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [
            RuntimeValue::Markdown(node, _),
            RuntimeValue::String(kind),
            RuntimeValue::String(title),
        ] => Ok(mq_markdown::Node::Callout(mq_markdown::Callout {
            kind: kind.to_uppercase(),
            title: if title.is_empty() {
                None
            } else {
                Some(title.to_string())
            },
            values: node.node_values(),
            position: None,
        })
        .into()),
        [a, RuntimeValue::String(kind), RuntimeValue::String(title)] if !a.is_none() => {
            Ok(mq_markdown::Node::Callout(mq_markdown::Callout {
                kind: kind.to_uppercase(),
                title: if title.is_empty() {
                    None
                } else {
                    Some(title.to_string())
                },
                values: vec![a.to_string().into()],
                position: None,
            })
            .into())
        }
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
                start: None,
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
            start: None,
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

#[mq_macros::mq_fn(name = "to_md_table_align", params = Fixed(1))]
fn to_md_table_align_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Array(values)] => Ok(mq_markdown::Node::TableAlign(mq_markdown::TableAlign {
            align: values.iter().map(|v| v.to_string().as_str().into()).collect(),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

fn node_from_runtime_value(value: &RuntimeValue) -> mq_markdown::Node {
    match value {
        RuntimeValue::Markdown(node, _) => (**node).clone(),
        _ => mq_markdown::Node::Text(mq_markdown::Text {
            value: value.to_string(),
            position: None,
        }),
    }
}

fn flatten_into_nodes(value: &RuntimeValue, out: &mut Vec<mq_markdown::Node>) {
    match value {
        RuntimeValue::Array(values) => {
            for value in values.iter() {
                flatten_into_nodes(value, out);
            }
        }
        _ => out.push(node_from_runtime_value(value)),
    }
}

#[mq_macros::mq_fn(name = "to_md_fragment", params = Fixed(1))]
fn to_md_fragment_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a @ (RuntimeValue::Array(_) | RuntimeValue::Markdown(_, _))] => {
            let mut values = Vec::new();
            flatten_into_nodes(a, &mut values);
            Ok(mq_markdown::Node::Fragment(mq_markdown::Fragment { values }).into())
        }
        _ => Ok(RuntimeValue::NONE),
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
        [RuntimeValue::Dict(map), RuntimeValue::String(key)] => Ok(runtime_value::dict_mut(map)
            .get_mut(&Ident::new(key))
            .map(std::mem::take)
            .unwrap_or(RuntimeValue::NONE)),
        [RuntimeValue::Dict(map), RuntimeValue::Symbol(key)] => Ok(runtime_value::dict_mut(map)
            .get_mut(key)
            .map(std::mem::take)
            .unwrap_or(RuntimeValue::NONE)),
        [RuntimeValue::Array(array), RuntimeValue::Number(index)] => {
            let len = array.len();
            let idx = index.value() as isize;
            let real_idx = if idx < 0 {
                (len as isize + idx).max(0) as usize
            } else {
                idx as usize
            };
            Ok(runtime_value::array_mut(array)
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
            runtime_value::dict_mut(&mut new_dict).insert(Ident::new(key_val), std::mem::take(value_val));
            Ok(RuntimeValue::Dict(new_dict))
        }
        [RuntimeValue::Dict(map_val), RuntimeValue::Symbol(key_val), value_val] => {
            let mut new_dict = std::mem::take(map_val);
            runtime_value::dict_mut(&mut new_dict).insert(*key_val, std::mem::take(value_val));
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
                Shared::unwrap_or_clone(std::mem::take(array_val))
            };

            // Set value at specified index
            new_array[index] = std::mem::take(value_val);
            Ok(RuntimeValue::Array(Shared::new(new_array)))
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
            Ok(RuntimeValue::Array(Shared::new(keys)))
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
            Ok(RuntimeValue::Array(Shared::new(values)))
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
                .map(|(k, v)| RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(k.as_str()), v.to_owned()])))
                .collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(Shared::new(entries)))
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
            let array_mut = runtime_value::array_mut(&mut new_array);
            if idx > array_mut.len() {
                array_mut.resize(idx, RuntimeValue::NONE);
            }
            array_mut.insert(idx, std::mem::take(value));
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
            runtime_value::dict_mut(&mut new_dict).insert(Ident::new(key_val), std::mem::take(value_val));
            Ok(RuntimeValue::Dict(new_dict))
        }
        [RuntimeValue::Dict(map_val), RuntimeValue::Symbol(key_val), value_val] => {
            let mut new_dict = std::mem::take(map_val);
            runtime_value::dict_mut(&mut new_dict).insert(*key_val, std::mem::take(value_val));
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
    Ok(RuntimeValue::Array(Shared::new(
        all_symbols()
            .into_iter()
            .map(|symbol| RuntimeValue::Symbol(Ident::new(&symbol)))
            .collect(),
    )))
}

#[mq_macros::mq_fn(name = "to_markdown", params = Fixed(1))]
fn to_markdown_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(Shared::new(
            parse_markdown_input(s).map_err(|e| Error::Runtime(format!("Failed to parse markdown: {}", e)))?,
        ))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("to_markdown should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "to_mdx", params = Fixed(1))]
fn to_mdx_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(Shared::new(
            parse_mdx_input(s).map_err(|e| Error::Runtime(format!("Failed to parse mdx: {}", e)))?,
        ))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("to_mdx should always receive exactly one argument"),
    }
}

#[mq_macros::mq_fn(name = "_get_markdown_position", params = Fixed(1))]
fn _get_markdown_position_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
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
        // Matches get_title/get_url/to_md_name: non-markdown input (including the None
        // a filtered-out selector like `.h` produces) resolves to None instead of erroring.
        _ => Ok(RuntimeValue::NONE),
    }
}

/// Public, documented entry point for `_get_markdown_position` (issue #1358).
#[mq_macros::mq_fn(name = "get_location", params = Fixed(1))]
fn get_location_impl(
    ident: &Ident,
    current_value: &RuntimeValue,
    args: Args,
    env: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    _get_markdown_position_impl(ident, current_value, args, env)
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
        .flexible(true)
        .from_reader(csv_str.as_bytes());

    if has_header {
        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| Error::Runtime(format!("Failed to parse CSV headers: {e}")))?
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Rows with fewer fields than the header are padded with empty strings;
        // fields beyond the header count are dropped, since there is no key for them.
        let rows: Result<Vec<RuntimeValue>, Error> = reader
            .records()
            .map(|record| {
                let record = record.map_err(|e| Error::Runtime(format!("Failed to parse CSV record: {e}")))?;
                let map: BTreeMap<Ident, RuntimeValue> = headers
                    .iter()
                    .enumerate()
                    .map(|(i, k)| {
                        (
                            Ident::new(k),
                            RuntimeValue::String(record.get(i).unwrap_or("").to_string()),
                        )
                    })
                    .collect();
                Ok(RuntimeValue::Dict(Shared::new(map)))
            })
            .collect();

        Ok(RuntimeValue::Array(Shared::new(rows?)))
    } else {
        let rows: Result<Vec<RuntimeValue>, Error> = reader
            .records()
            .map(|record| {
                let record = record.map_err(|e| Error::Runtime(format!("Failed to parse CSV record: {e}")))?;
                let arr: Vec<RuntimeValue> = record.iter().map(|v| RuntimeValue::String(v.to_string())).collect();
                Ok(RuntimeValue::Array(Shared::new(arr)))
            })
            .collect();

        Ok(RuntimeValue::Array(Shared::new(rows?)))
    }
}

#[mq_macros::mq_fn(name = "_levenshtein_distance", params = Fixed(2))]
fn _levenshtein_distance_impl(
    ident: &Ident,
    _: &RuntimeValue,
    mut args: Args,
    _: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
            Ok(RuntimeValue::Number((strsim::levenshtein(s1, s2) as i64).into()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("_levenshtein_distance should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "_jaro_distance", params = Fixed(2))]
fn _jaro_distance_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(strsim::jaro(s1, s2).into())),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("_jaro_distance should always receive exactly two arguments"),
    }
}

#[mq_macros::mq_fn(name = "_jaro_winkler_distance", params = Fixed(2))]
fn _jaro_winkler_distance_impl(
    ident: &Ident,
    _: &RuntimeValue,
    mut args: Args,
    _: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
            Ok(RuntimeValue::Number(strsim::jaro_winkler(s1, s2).into()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("_jaro_winkler_distance should always receive exactly two arguments"),
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
            let mut docs = yaml_rust2::YamlLoader::load_from_str(s)
                .map_err(|e| Error::Runtime(format!("Failed to parse YAML: {}", e)))?
                .into_iter();

            match (docs.next(), docs.next()) {
                (None, _) => Ok(RuntimeValue::NONE),
                // A single `---`-separated document parses the same as before.
                (Some(doc), None) => Ok(doc.into()),
                // Multiple `---`-separated documents are returned as an array
                // instead of silently discarding everything past the first one.
                (Some(first), Some(second)) => Ok(RuntimeValue::Array(Shared::new(
                    std::iter::once(first)
                        .chain(std::iter::once(second))
                        .chain(docs)
                        .map(RuntimeValue::from)
                        .collect(),
                ))),
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

#[mq_macros::mq_fn(name = "_toon_stringify", params = Fixed(1))]
fn _toon_stringify_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [value] => {
            let json_value = std::mem::take(value).to_json_value();
            let toon_str = toon_format::encode_default(&json_value)
                .map_err(|e| Error::Runtime(format!("Failed to encode TOON: {}", e)))?;
            Ok(RuntimeValue::String(toon_str))
        }
        _ => unreachable!("_toon_stringify should always receive exactly one argument"),
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

#[mq_macros::mq_fn(name = "_gron_parse", params = Fixed(1))]
fn _gron_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            let value = self::gron::parse(s).map_err(|e| Error::Runtime(format!("Failed to parse gron: {}", e)))?;
            Ok(value.into())
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("_gron_parse should always receive exactly one argument"),
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
                        dict.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs)));
                        dict.insert(Ident::new("children"), RuntimeValue::Array(Shared::new(children)));
                        dict.insert(
                            Ident::new("text"),
                            text.map(RuntimeValue::String).unwrap_or(RuntimeValue::NONE),
                        );
                        let element = RuntimeValue::Dict(Shared::new(dict));

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
                        dict.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs)));
                        dict.insert(Ident::new("children"), RuntimeValue::empty_array());
                        dict.insert(Ident::new("text"), RuntimeValue::NONE);
                        let element = RuntimeValue::Dict(Shared::new(dict));

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
            runtime_value::array_mut(arr).push(std::mem::take(v));
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
            runtime_value::array_mut(arr).insert(0, std::mem::take(v));
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
                del_inline.push(RuntimeValue::Dict(Shared::new(m)));
            }
            ChangeTag::Insert => {
                let mut m = BTreeMap::new();
                m.insert(Ident::new("tag"), RuntimeValue::String("insert".into()));
                m.insert(Ident::new("value"), val);
                ins_inline.push(RuntimeValue::Dict(Shared::new(m)));
            }
            ChangeTag::Equal => {
                for inline in [&mut del_inline, &mut ins_inline] {
                    let mut m = BTreeMap::new();
                    m.insert(Ident::new("tag"), RuntimeValue::String("equal".into()));
                    m.insert(Ident::new("value"), RuntimeValue::String(c.value().to_string()));
                    inline.push(RuntimeValue::Dict(Shared::new(m)));
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
                        del_map.insert(Ident::new("inline"), RuntimeValue::Array(Shared::new(del_inline)));
                        result.push(RuntimeValue::Dict(Shared::new(del_map)));
                        let mut ins_map = BTreeMap::new();
                        ins_map.insert(Ident::new("tag"), RuntimeValue::String("insert".into()));
                        ins_map.insert(Ident::new("value"), new_val.clone());
                        ins_map.insert(Ident::new("inline"), RuntimeValue::Array(Shared::new(ins_inline)));
                        result.push(RuntimeValue::Dict(Shared::new(ins_map)));
                    } else {
                        let mut del_map = BTreeMap::new();
                        del_map.insert(Ident::new("tag"), RuntimeValue::String("delete".into()));
                        del_map.insert(Ident::new("value"), old_val.clone());
                        result.push(RuntimeValue::Dict(Shared::new(del_map)));
                        let mut ins_map = BTreeMap::new();
                        ins_map.insert(Ident::new("tag"), RuntimeValue::String("insert".into()));
                        ins_map.insert(Ident::new("value"), new_val.clone());
                        result.push(RuntimeValue::Dict(Shared::new(ins_map)));
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
                    result.push(RuntimeValue::Dict(Shared::new(map)));
                    i += 1;
                }
            }
            Ok(RuntimeValue::Array(Shared::new(result)))
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
                    del_map.insert(Ident::new("inline"), RuntimeValue::Array(Shared::new(del_inline)));
                    result.push(RuntimeValue::Dict(Shared::new(del_map)));
                    let mut ins_map = BTreeMap::new();
                    ins_map.insert(Ident::new("tag"), RuntimeValue::String("insert".into()));
                    ins_map.insert(Ident::new("value"), RuntimeValue::String(new_val.to_string()));
                    ins_map.insert(Ident::new("inline"), RuntimeValue::Array(Shared::new(ins_inline)));
                    result.push(RuntimeValue::Dict(Shared::new(ins_map)));
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
                    result.push(RuntimeValue::Dict(Shared::new(map)));
                    i += 1;
                }
            }
            Ok(RuntimeValue::Array(Shared::new(result)))
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

/// Returns whether `path` matches the glob `pattern` (e.g. `*.md`, `docs/**/*.rs`).
#[mq_macros::mq_fn(name = "glob_match", params = Fixed(2))]
fn glob_match_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(pattern), RuntimeValue::String(path)] => {
            path::glob_match(pattern, path).map(RuntimeValue::Boolean)
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("glob_match should always receive exactly two arguments"),
    }
}

/// Reads the contents of `path` as a string. Requires the ambient [`Io`]'s read
/// permission (see [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "read_file", params = Fixed(1))]
fn read_file_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => io_context::current()
            .read_to_string(std::path::Path::new(path.as_str()))
            .map(RuntimeValue::String)
            .map_err(|e| Error::Runtime(format!("Failed to read file {}: {}", path, e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("read_file should always receive exactly one argument"),
    }
}

/// Checks whether `path` exists on the filesystem. Requires the ambient [`Io`]'s read
/// permission (see [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "file_exists", params = Fixed(1))]
fn file_exists_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => io_context::current()
            .exists(std::path::Path::new(path.as_str()))
            .map(RuntimeValue::Boolean)
            .map_err(|e| Error::Runtime(format!("Failed to check {}: {}", path, e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("file_exists should always receive exactly one argument"),
    }
}

/// Returns the size, in bytes, of the file at `path`. Requires the ambient [`Io`]'s read
/// permission (see [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "file_size", params = Fixed(1))]
fn file_size_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => io_context::current()
            .file_size(std::path::Path::new(path.as_str()))
            .map(|size| RuntimeValue::Number((size as usize).into()))
            .map_err(|e| Error::Runtime(format!("Failed to get size of {}: {}", path, e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("file_size should always receive exactly one argument"),
    }
}

/// Reads the contents of `path` as raw bytes. Requires the ambient [`Io`]'s read
/// permission (see [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "read_file_bytes", params = Fixed(1))]
fn read_file_bytes_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => io_context::current()
            .read_bytes(std::path::Path::new(path.as_str()))
            .map(RuntimeValue::Bytes)
            .map_err(|e| Error::Runtime(format!("Failed to read file {}: {}", path, e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => unreachable!("read_file_bytes should always receive exactly one argument"),
    }
}

/// Writes `content` (string or bytes) to `path`, creating or truncating the file.
/// Requires the ambient [`Io`]'s write permission (see [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "write_file", params = Fixed(2))]
fn write_file_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    fn write(path: &str, content: &[u8]) -> Result<RuntimeValue, Error> {
        io_context::current()
            .write(std::path::Path::new(path), content)
            .map(|()| RuntimeValue::NONE)
            .map_err(|e| Error::Runtime(format!("Failed to write file {}: {}", path, e)))
    }

    match args.as_mut_slice() {
        [RuntimeValue::String(path), RuntimeValue::String(content)] => write(path, content.as_bytes()),
        [RuntimeValue::String(path), RuntimeValue::Bytes(content)] => write(path, content),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("write_file should always receive exactly two arguments"),
    }
}

#[cfg(feature = "file-io")]
fn is_embeddable_image_url(url: &str) -> bool {
    !url.starts_with("data:") && !url.contains("://")
}

#[cfg(feature = "file-io")]
fn guess_image_mime_type(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "tif" | "tiff" => "image/tiff",
        _ => return None,
    })
}

#[cfg(feature = "file-io")]
fn image_extension_for_mime(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
        "image/avif" => "avif",
        "image/tiff" => "tiff",
        _ => return None,
    })
}

/// Reads the local image file referenced by an `.image` node's `url` and inlines it as a
/// `data:` URI, base64-encoding the bytes and inferring the MIME type from the file
/// extension. The path is resolved relative to `base_dir` (default `"."`). URLs that are
/// already `data:` URIs, or that contain a `://` scheme (e.g. `https://`), are left
/// unchanged, as are non-image nodes. Requires the ambient [`Io`]'s read permission (see
/// [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "embed_images", params = Range(1, 2))]
fn embed_images_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [a @ RuntimeValue::Markdown(_, _)] => embed_image(a, "."),
        [a @ RuntimeValue::Markdown(_, _), RuntimeValue::String(base_dir)] => {
            let base_dir = std::mem::take(base_dir);
            embed_image(a, &base_dir)
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => unreachable!("embed_images should always receive one or two arguments"),
    }
}

#[cfg(feature = "file-io")]
fn embed_image(arg: &mut RuntimeValue, base_dir: &str) -> Result<RuntimeValue, Error> {
    let RuntimeValue::Markdown(node, _) = arg else {
        unreachable!()
    };
    let mq_markdown::Node::Image(image) = &mut **node else {
        return Ok(std::mem::take(arg));
    };

    if !is_embeddable_image_url(&image.url) {
        return Ok(std::mem::take(arg));
    }

    let path = std::path::Path::new(base_dir).join(&image.url);
    let bytes = io_context::current()
        .read_bytes(&path)
        .map_err(|e| Error::Runtime(format!("Failed to read image {}: {}", path.display(), e)))?;
    let mime = guess_image_mime_type(&path)
        .ok_or_else(|| Error::Runtime(format!("Unsupported image extension for {}", path.display())))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    image.url = format!("data:{mime};base64,{encoded}");

    Ok(std::mem::take(arg))
}

/// Decodes an `.image` node's `data:` URI and writes the bytes to a file under `dir`,
/// named by the content's MD5 hash with an extension inferred from the MIME type, then
/// replaces `url` with that file's path. Nodes whose `url` is not a base64-encoded `data:`
/// URI (including non-image nodes) are left unchanged. Requires the ambient [`Io`]'s write
/// permission (see [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "extract_images", params = Fixed(2))]
fn extract_images_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [a @ RuntimeValue::Markdown(_, _), RuntimeValue::String(dir)] => {
            let dir = std::mem::take(dir);
            extract_image(a, &dir)
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => unreachable!("extract_images should always receive exactly two arguments"),
    }
}

#[cfg(feature = "file-io")]
fn extract_image(arg: &mut RuntimeValue, dir: &str) -> Result<RuntimeValue, Error> {
    let RuntimeValue::Markdown(node, _) = arg else {
        unreachable!()
    };
    let mq_markdown::Node::Image(image) = &mut **node else {
        return Ok(std::mem::take(arg));
    };

    let Some(data) = image.url.strip_prefix("data:") else {
        return Ok(std::mem::take(arg));
    };
    let Some((meta, payload)) = data.split_once(',') else {
        return Err(Error::Runtime(format!("Malformed data URI: {}", image.url)));
    };
    let Some(mime) = meta.strip_suffix(";base64") else {
        return Err(Error::Runtime(format!(
            "Only base64-encoded data URIs can be extracted: {}",
            image.url
        )));
    };
    let ext =
        image_extension_for_mime(mime).ok_or_else(|| Error::Runtime(format!("Unsupported image MIME type: {mime}")))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload)?;
    let hash = match convert::md5_bytes(&bytes)? {
        RuntimeValue::String(s) => s,
        _ => unreachable!(),
    };
    let path = std::path::Path::new(dir).join(format!("{hash}.{ext}"));

    io_context::current()
        .write(&path, &bytes)
        .map_err(|e| Error::Runtime(format!("Failed to write image {}: {}", path.display(), e)))?;

    image.url = path.to_string_lossy().into_owned();

    Ok(std::mem::take(arg))
}

/// Performs an HTTPS request with the given method (`"get"`/`:get`, `"post"`/`:post`, etc.) and
/// returns the response body as a string. `body`, when given, is sent regardless of method.
/// `headers`, a dict of string to string, is applied to the request when given.
/// Requires the ambient [`Io`]'s net permission (see [`io_context`]).
#[cfg(feature = "http")]
#[mq_macros::mq_fn(name = "http", params = Range(2, 4))]
fn http_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [
            method @ (RuntimeValue::Symbol(_) | RuntimeValue::String(_)),
            RuntimeValue::String(url),
        ] => http::request(method, url, None, None),
        [
            method @ (RuntimeValue::Symbol(_) | RuntimeValue::String(_)),
            RuntimeValue::String(url),
            RuntimeValue::String(body),
        ] => http::request(method, url, Some(body), None),
        [
            method @ (RuntimeValue::Symbol(_) | RuntimeValue::String(_)),
            RuntimeValue::String(url),
            RuntimeValue::Dict(headers),
        ] => http::request(method, url, None, Some(headers)),
        [
            method @ (RuntimeValue::Symbol(_) | RuntimeValue::String(_)),
            RuntimeValue::String(url),
            RuntimeValue::String(body),
            RuntimeValue::Dict(headers),
        ] => http::request(method, url, Some(body), Some(headers)),
        args => Err(Error::InvalidTypes(
            ident.to_string(),
            args.iter_mut().map(std::mem::take).collect(),
        )),
    }
}

#[cfg(feature = "http")]
#[mq_macros::mq_fn(name = "http_all", params = Fixed(1))]
fn http_all_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(requests)] => {
            let specs = requests.iter().map(parse_http_request).collect::<Result<Vec<_>, _>>()?;
            let io = io_context::current();
            let bodies = io
                .http_request_all(&specs)
                .map_err(|e| Error::Runtime(format!("http_all: {e}")))?;
            Ok(RuntimeValue::Array(Shared::new(
                bodies.into_iter().map(RuntimeValue::String).collect(),
            )))
        }
        args => Err(Error::InvalidTypes(
            ident.to_string(),
            args.iter_mut().map(std::mem::take).collect(),
        )),
    }
}

#[cfg(feature = "http")]
fn parse_http_request(value: &RuntimeValue) -> Result<HttpRequestSpec, Error> {
    let RuntimeValue::Dict(fields) = value else {
        return Err(Error::Runtime(
            "http_all: each request must be a dict with `url` (and optional `method`, `body`, `headers`)".to_string(),
        ));
    };
    let url = fields
        .get(&Ident::from("url"))
        .ok_or_else(|| Error::Runtime("http_all: each request dict needs a `url` string".to_string()))
        .and_then(|value| match value {
            RuntimeValue::String(url) => Ok(url.clone()),
            other => Err(Error::Runtime(format!("http_all: `url` must be a string, got {other}"))),
        })?;
    let method = match fields.get(&Ident::from("method")) {
        Some(RuntimeValue::Symbol(method)) => method.as_str().to_string(),
        Some(RuntimeValue::String(method)) => method.clone(),
        Some(other) => {
            return Err(Error::Runtime(format!(
                "http_all: `method` must be a string or symbol, got {other}"
            )));
        }
        None => "GET".to_string(),
    };
    let method = method
        .parse::<ureq::http::Method>()
        .map_err(|_| Error::Runtime(format!("http_all: invalid HTTP method {method:?}")))?
        .to_string();
    let body = match fields.get(&Ident::from("body")) {
        Some(RuntimeValue::String(body)) => Some(body.clone()),
        Some(other) => {
            return Err(Error::Runtime(format!(
                "http_all: `body` must be a string, got {other}"
            )));
        }
        None => None,
    };
    let headers = match fields.get(&Ident::from("headers")) {
        Some(RuntimeValue::Dict(headers)) => headers
            .iter()
            .map(|(name, value)| match value {
                RuntimeValue::String(value) => Ok((name.as_str().to_string(), value.clone())),
                other => Err(Error::Runtime(format!(
                    "http_all: header {name:?} must be a string, got {other}"
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(other) => {
            return Err(Error::Runtime(format!(
                "http_all: `headers` must be a dict, got {other}"
            )));
        }
        None => Vec::new(),
    };
    Ok(HttpRequestSpec {
        method,
        url,
        body,
        headers,
    })
}

#[cfg(all(feature = "http", feature = "mock-io"))]
#[mq_macros::mq_fn(name = "mock_fetch", params = Fixed(2))]
fn mock_fetch_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(url), RuntimeValue::String(body)] => io_context::current()
            .set_fetch_response(url, body)
            .map(|()| RuntimeValue::NONE)
            .map_err(|e| Error::Runtime(format!("Failed to mock fetch response for {}: {}", url, e))),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("mock_fetch should always receive exactly two arguments"),
    }
}

#[cfg(feature = "process-io")]
fn runtime_args_to_strings(command: &str, arr: &[RuntimeValue]) -> Result<Vec<String>, Error> {
    arr.iter()
        .map(|v| match v {
            RuntimeValue::String(s) => Ok(s.clone()),
            other => Err(Error::Runtime(format!(
                "system: `{command}` arguments must be strings, got {other}"
            ))),
        })
        .collect()
}

#[cfg(feature = "process-io")]
#[mq_macros::mq_fn(name = "system", params = Range(1, 2))]
fn system_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(command)] => io_context::current()
            .execute(command, &[])
            .map(RuntimeValue::String)
            .map_err(|e| Error::Runtime(format!("Failed to execute {}: {}", command, e))),
        [RuntimeValue::String(command), RuntimeValue::Array(arr)] => {
            let cmd_args = runtime_args_to_strings(command, arr)?;
            io_context::current()
                .execute(command, &cmd_args)
                .map(RuntimeValue::String)
                .map_err(|e| Error::Runtime(format!("Failed to execute {}: {}", command, e)))
        }
        args => Err(Error::InvalidTypes(
            ident.to_string(),
            args.iter_mut().map(std::mem::take).collect(),
        )),
    }
}

#[cfg(feature = "css-selector")]
#[mq_macros::mq_fn(name = "css", params = Fixed(2))]
fn css_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(html), RuntimeValue::String(selector)] => Ok(RuntimeValue::Array(Shared::new(
            css::select_html(html, selector)?
                .into_iter()
                .map(RuntimeValue::from)
                .collect(),
        ))),
        args => Err(Error::InvalidTypes(
            ident.to_string(),
            args.iter_mut().map(std::mem::take).collect(),
        )),
    }
}

/// Returns the text content of every element in `html` matching the CSS `selector`, as an array
/// of strings.
#[cfg(feature = "css-selector")]
#[mq_macros::mq_fn(name = "css_text", params = Fixed(2))]
fn css_text_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(html), RuntimeValue::String(selector)] => Ok(RuntimeValue::Array(Shared::new(
            css::select_text(html, selector)?
                .into_iter()
                .map(RuntimeValue::from)
                .collect(),
        ))),
        args => Err(Error::InvalidTypes(
            ident.to_string(),
            args.iter_mut().map(std::mem::take).collect(),
        )),
    }
}

/// Returns the value of attribute `name` for every element in `html` matching the CSS
/// `selector`, as an array; elements without that attribute produce `None` in the array.
#[cfg(feature = "css-selector")]
#[mq_macros::mq_fn(name = "css_attr", params = Fixed(3))]
fn css_attr_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [
            RuntimeValue::String(html),
            RuntimeValue::String(selector),
            RuntimeValue::String(name),
        ] => Ok(RuntimeValue::Array(Shared::new(
            css::select_attr(html, selector, name)?
                .into_iter()
                .map(|attr| attr.map(RuntimeValue::from).unwrap_or(RuntimeValue::NONE))
                .collect(),
        ))),
        args => Err(Error::InvalidTypes(
            ident.to_string(),
            args.iter_mut().map(std::mem::take).collect(),
        )),
    }
}

#[cfg(feature = "file-io")]
fn collection_record(path: String, raw: &str) -> Result<RuntimeValue, Error> {
    let nodes = mq_markdown::Markdown::from_markdown_str(raw)
        .map_err(|e| Error::Runtime(format!("Failed to parse markdown {}: {}", path, e)))?
        .nodes;

    let (frontmatter, body_nodes) = match nodes.first() {
        Some(mq_markdown::Node::Yaml(yaml)) => {
            let frontmatter = yaml_rust2::YamlLoader::load_from_str(&yaml.value)
                .map_err(|e| Error::Runtime(format!("Failed to parse YAML frontmatter in {}: {}", path, e)))?
                .into_iter()
                .next()
                .map(RuntimeValue::from)
                .unwrap_or(RuntimeValue::NONE);
            (frontmatter, &nodes[1..])
        }
        Some(mq_markdown::Node::Toml(toml_node)) => {
            let value: serde_json::Value = toml::from_str(&toml_node.value)
                .map_err(|e| Error::Runtime(format!("Failed to parse TOML frontmatter in {}: {}", path, e)))?;
            (value.into(), &nodes[1..])
        }
        _ => (RuntimeValue::NONE, &nodes[..]),
    };

    let title = nodes
        .iter()
        .find(|node| matches!(node, mq_markdown::Node::Heading(_)))
        .map(|node| RuntimeValue::String(node.value()))
        .unwrap_or(RuntimeValue::NONE);

    let content = RuntimeValue::Array(Shared::new(
        body_nodes.iter().cloned().map(RuntimeValue::from).collect(),
    ));

    let mut record = BTreeMap::new();
    record.insert(Ident::new("path"), RuntimeValue::String(path));
    record.insert(Ident::new("title"), title);
    record.insert(Ident::new("frontmatter"), frontmatter);
    record.insert(Ident::new("content"), content);

    Ok(RuntimeValue::Dict(Shared::new(record)))
}

/// Checks matchers closest-directory-first, so a deeper `.gitignore` overrides a shallower one.
#[cfg(feature = "file-io")]
fn is_gitignored(stack: &[ignore::gitignore::Gitignore], path: &std::path::Path, is_dir: bool) -> bool {
    for gi in stack.iter().rev() {
        match gi.matched(path, is_dir) {
            ignore::Match::Ignore(_) => return true,
            ignore::Match::Whitelist(_) => return false,
            ignore::Match::None => continue,
        }
    }
    false
}

/// Reads `dir`'s `.gitignore` (if any) through the ambient [`Io`] rather than the real
/// filesystem, so it stays subject to the same sandboxed read permission as the rest of the walk.
#[cfg(feature = "file-io")]
fn load_gitignore(io: &dyn Io, dir: &std::path::Path) -> Option<ignore::gitignore::Gitignore> {
    let gitignore_path = dir.join(".gitignore");
    if !io.exists(&gitignore_path).unwrap_or(false) {
        return None;
    }
    let contents = io.read_to_string(&gitignore_path).ok()?;
    let mut builder = ignore::gitignore::GitignoreBuilder::new(dir);
    for line in contents.lines() {
        let _ = builder.add_line(None, line);
    }
    builder.build().ok()
}

#[cfg(feature = "file-io")]
fn collect_markdown_files(
    io: &dyn Io,
    dir: &std::path::Path,
    ancestors: &mut FxHashSet<std::path::PathBuf>,
    gitignore_stack: &mut Vec<ignore::gitignore::Gitignore>,
    respect_gitignore: bool,
) -> Result<Vec<std::path::PathBuf>, Error> {
    let canonical = io.canonicalize(dir);
    if !ancestors.insert(canonical.clone()) {
        return Ok(Vec::new());
    }

    let pushed_gitignore = respect_gitignore && load_gitignore(io, dir).map(|gi| gitignore_stack.push(gi)).is_some();

    let entries = io
        .read_dir(dir)
        .map_err(|e| Error::Runtime(format!("Failed to read directory {}: {}", dir.display(), e)))?;

    let mut paths = Vec::new();

    for (path, is_dir) in entries {
        if respect_gitignore {
            let is_hidden = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'));
            if is_hidden || is_gitignored(gitignore_stack, &path, is_dir) {
                continue;
            }
        }

        if is_dir {
            paths.extend(collect_markdown_files(
                io,
                &path,
                ancestors,
                gitignore_stack,
                respect_gitignore,
            )?);
        } else if io.exists(&path).unwrap_or(false)
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        {
            paths.push(path);
        }
    }

    if pushed_gitignore {
        gitignore_stack.pop();
    }
    ancestors.remove(&canonical);
    Ok(paths)
}

#[cfg(feature = "file-io")]
fn collection_impl_inner(dir: &str, respect_gitignore: bool) -> Result<RuntimeValue, Error> {
    let io = io_context::current();
    let mut ancestors = FxHashSet::default();
    let mut gitignore_stack = Vec::new();
    let mut paths = collect_markdown_files(
        io.as_ref(),
        std::path::Path::new(dir),
        &mut ancestors,
        &mut gitignore_stack,
        respect_gitignore,
    )?;
    paths.sort();

    let records = paths
        .into_iter()
        .map(|path| {
            let raw = io
                .read_to_string(&path)
                .map_err(|e| Error::Runtime(format!("Failed to read file {}: {}", path.display(), e)))?;
            collection_record(path.to_string_lossy().into_owned(), &raw)
        })
        .collect::<Result<Vec<RuntimeValue>, Error>>()?;

    Ok(RuntimeValue::Array(Shared::new(records)))
}

/// Recursively reads every Markdown file under `dir`. Requires the ambient [`Io`]'s read
/// permission (see [`io_context`]).
///
/// `respect_gitignore` (default `false`, keeping prior behavior) skips dotfiles/dot-directories
/// and any path matched by a `.gitignore` found in `dir` or one of its subdirectories, with
/// closer `.gitignore` files taking precedence over farther ones, same as `git`.
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "collection", params = Range(1, 2))]
fn collection_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(dir)] => collection_impl_inner(dir.as_str(), false),
        [RuntimeValue::String(dir), RuntimeValue::Boolean(respect_gitignore)] => {
            collection_impl_inner(dir.as_str(), *respect_gitignore)
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => unreachable!("collection should always receive one or two arguments"),
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
    STRPTIME,
    DATE_ADD,
    DATE_DIFF,
    DATE_RELATIVE,
    BASE64,
    BASE64D,
    BASE64URL,
    BASE64URLD,
    MD5,
    SHA256,
    SHA512,
    UUID,
    UUID_V4,
    UUID_V7,
    RAND,
    RAND_INT,
    RANDOM_STRING,
    SHUFFLE,
    SAMPLE,
    MIN,
    MAX,
    FROM_HTML,
    TO_HTML,
    HTML_ESCAPE,
    HTML_UNESCAPE,
    STRIP_TAGS,
    SANITIZE_HTML,
    TO_MARKDOWN_STRING,
    TO_STRING,
    TO_NUMBER,
    TO_BOOLEAN,
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
    URL_DECODE,
    TO_TEXT,
    ENDS_WITH,
    STARTS_WITH,
    REGEX_MATCH,
    IS_REGEX_MATCH,
    IS_NOT_REGEX_MATCH,
    CAPTURE,
    SCAN,
    DOWNCASE,
    ASCII_DOWNCASE,
    GSUB,
    REPLACE,
    REPEAT,
    WORD_WRAP,
    TRUNCATE,
    EXPLODE,
    IMPLODE,
    TRIM,
    LTRIM,
    RTRIM,
    UPCASE,
    ASCII_UPCASE,
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
    TOKEN_COUNT,
    TOKEN_COMPRESS,
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
    SET_CHILDREN,
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
    TO_BLOCKQUOTE,
    TO_DELETE,
    TO_CALLOUT,
    TO_MD_TEXT,
    TO_MD_LIST,
    TO_MD_TABLE_ROW,
    TO_MD_TABLE_CELL,
    TO_MD_TABLE_ALIGN,
    TO_MD_FRAGMENT,
    GET_TITLE,
    GET_URL,
    GET_LOCATION,
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
    _LEVENSHTEIN_DISTANCE,
    _JARO_DISTANCE,
    _JARO_WINKLER_DISTANCE,
    _JSON_PARSE,
    _GRON_PARSE,
    _YAML_PARSE,
    _TOON_PARSE,
    _TOON_STRINGIFY,
    _TOML_PARSE,
    _CBOR_PARSE,
    _CBOR_STRINGIFY,
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
    GLOB_MATCH,
    #[cfg(feature = "file-io")]
    READ_FILE,
    #[cfg(feature = "file-io")]
    FILE_EXISTS,
    #[cfg(feature = "file-io")]
    FILE_SIZE,
    #[cfg(feature = "file-io")]
    READ_FILE_BYTES,
    #[cfg(feature = "file-io")]
    COLLECTION,
    #[cfg(feature = "file-io")]
    WRITE_FILE,
    #[cfg(feature = "file-io")]
    EMBED_IMAGES,
    #[cfg(feature = "file-io")]
    EXTRACT_IMAGES,
    #[cfg(feature = "http")]
    HTTP,
    #[cfg(feature = "http")]
    HTTP_ALL,
    #[cfg(all(feature = "http", feature = "mock-io"))]
    MOCK_FETCH,
    #[cfg(feature = "process-io")]
    SYSTEM,
    #[cfg(feature = "css-selector")]
    CSS,
    #[cfg(feature = "css-selector")]
    CSS_TEXT,
    #[cfg(feature = "css-selector")]
    CSS_ATTR,
}

/// A single runnable, verified example shown by `mq help`.
///
/// `expected` is checked against the real evaluation result of `code` by a test
/// (see `doc_examples` tests), so examples cannot silently rot.
#[derive(Clone, Debug)]
pub struct BuiltinExample {
    pub code: &'static str,
    pub expected: &'static str,
}

#[derive(Clone, Debug)]
pub struct BuiltinSelectorDoc {
    pub description: &'static str,
    pub params: &'static [&'static str],
    /// Parallel to `params`; a type name (e.g. "string", "number") or "dynamic" per param.
    pub param_types: &'static [&'static str],
    /// Type name of the value this selector matches/produces (e.g. "markdown", "bool").
    pub returns: &'static str,
    pub examples: &'static [BuiltinExample],
    /// Cargo feature flag required to use this selector, if any (e.g. "css-selector").
    pub capability: Option<&'static str>,
}

pub static BUILTIN_SELECTOR_DOC: LazyLock<FxHashMap<SmolStr, BuiltinSelectorDoc>> = LazyLock::new(|| {
    let mut map = FxHashMap::with_capacity_and_hasher(100, FxBuildHasher);

    map.insert(
        SmolStr::new(".h"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the specified depth.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 3) | .h"#,
                expected: r#"### Title"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".text"),
        BuiltinSelectorDoc {
            description: "Selects a text node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_text("Hello") | .text"#,
                expected: r#"Hello"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".h1"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 1 depth.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 1) | .h1"#,
                expected: r#"# Title"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".h2"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 2 depth.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 2) | .h2"#,
                expected: r#"## Title"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".h3"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 3 depth.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 3) | .h3"#,
                expected: r#"### Title"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".h4"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 4 depth.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 4) | .h4"#,
                expected: r#"#### Title"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".h5"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 5 depth.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 5) | .h5"#,
                expected: r#"##### Title"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".h6"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the 6 depth.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 6) | .h6"#,
                expected: r#"###### Title"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".code"),
        BuiltinSelectorDoc {
            description: "Selects a code block node with the specified language.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_code("x = 1", "python") | .code"#,
                expected: r#"```python
x = 1
```"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".code_inline"),
        BuiltinSelectorDoc {
            description: "Selects an inline code node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_code_inline("x") | .code_inline"#,
                expected: r#"`x`"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".inline_math"),
        BuiltinSelectorDoc {
            description: "Selects an inline math node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_math_inline("x^2") | .inline_math"#,
                expected: r#"$x^2$"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".strong"),
        BuiltinSelectorDoc {
            description: "Selects a strong (bold) node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_strong("Bold") | .strong"#,
                expected: r#"**Bold**"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".emphasis"),
        BuiltinSelectorDoc {
            description: "Selects an emphasis (italic) node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_em("Italic") | .emphasis"#,
                expected: r#"*Italic*"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".delete"),
        BuiltinSelectorDoc {
            description: "Selects a delete (strikethrough) node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_delete("Old") | .delete"#,
                expected: r#"~~Old~~"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".link"),
        BuiltinSelectorDoc {
            description: "Selects a link node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_link("https://example.com", "Example", "") | .link"#,
                expected: r#"[Example](https://example.com)"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".link_ref"),
        BuiltinSelectorDoc {
            description: "Selects a link reference node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("[text][ref]\n\n[ref]: https://example.com")[0] | .link_ref"#,
                expected: r#"[text][ref]"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".image"),
        BuiltinSelectorDoc {
            description: "Selects an image node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_image("https://example.com/a.png", "Alt", "") | .image"#,
                expected: r#"![Alt](https://example.com/a.png "")"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".heading"),
        BuiltinSelectorDoc {
            description: "Selects a heading node with the specified depth.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 2) | .heading"#,
                expected: r#"## Title"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".horizontal_rule"),
        BuiltinSelectorDoc {
            description: "Selects a horizontal rule node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_hr() | .horizontal_rule"#,
                expected: r#"***"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".blockquote"),
        BuiltinSelectorDoc {
            description: "Selects a blockquote node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_blockquote("Quote") | .blockquote"#,
                expected: r#"> Quote"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".[][]"),
        BuiltinSelectorDoc {
            description: "Selects a table cell node with the specified row and column.",
            params: &["row", "column"],
            param_types: &["number", "number"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_table_cell("A1", 0, 0) | .[][]"#,
                expected: r#"A1"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".table"),
        BuiltinSelectorDoc {
            description: "Selects a table cell node with the specified row and column.",
            params: &["row", "column"],
            param_types: &["number", "number"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_table_cell("A1", 0, 0) | .table"#,
                expected: r#"A1"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".table_align"),
        BuiltinSelectorDoc {
            description: "Selects a table align node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_table_align(["left", "right"]) | .table_align"#,
                expected: r#"|:---|---:|"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".html"),
        BuiltinSelectorDoc {
            description: "Selects an HTML node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("<div>hi</div>")[0] | .html"#,
                expected: r#"<div>hi</div>"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".<>"),
        BuiltinSelectorDoc {
            description: "Selects an HTML node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".footnote"),
        BuiltinSelectorDoc {
            description: "Selects a footnote node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("Text[^1]\n\n[^1]: Note")[2] | .footnote"#,
                expected: r#"[^1]: Note"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".mdx_jsx_flow_element"),
        BuiltinSelectorDoc {
            description: "Selects an MDX JSX flow element node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_mdx("<Foo />")[0] | .mdx_jsx_flow_element"#,
                expected: r#"<Foo />"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".list"),
        BuiltinSelectorDoc {
            description: "Selects a list node with the specified index and checked state.",
            params: &["indent", "checked"],
            param_types: &["number", "bool"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_list("Item", 0) | .list"#,
                expected: r#"- Item"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".[]"),
        BuiltinSelectorDoc {
            description: "Selects a list node with the specified index and checked state.",
            params: &["indent", "checked"],
            param_types: &["number", "bool"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_list("Item", 0) | .[]"#,
                expected: r#"- Item"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".mdx_js_esm"),
        BuiltinSelectorDoc {
            description: "Selects an MDX JS ESM node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".toml"),
        BuiltinSelectorDoc {
            description: "Selects a TOML node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("+++\nkey = 1\n+++\n\nBody")[0] | .toml"#,
                expected: r#"+++
key = 1
+++"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".yaml"),
        BuiltinSelectorDoc {
            description: "Selects a YAML node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("---\nkey: 1\n---\n\nBody")[0] | .yaml"#,
                expected: r#"---
key: 1
---"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".break"),
        BuiltinSelectorDoc {
            description: "Selects a break node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("Line1  \nLine2")[1] | .break"#,
                expected: "\\\n",
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".mdx_text_expression"),
        BuiltinSelectorDoc {
            description: "Selects an MDX text expression node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_mdx("Value is {1 + 1}.")[1] | .mdx_text_expression"#,
                expected: r#"{1 + 1}"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".footnote_ref"),
        BuiltinSelectorDoc {
            description: "Selects a footnote reference node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("Text[^1]\n\n[^1]: Note")[1] | .footnote_ref"#,
                expected: r#"[^1]"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".image_ref"),
        BuiltinSelectorDoc {
            description: "Selects an image reference node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("![alt][ref]\n\n[ref]: https://example.com/a.png")[0] | .image_ref"#,
                expected: r#"![alt][ref]"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".mdx_jsx_text_element"),
        BuiltinSelectorDoc {
            description: "Selects an MDX JSX text element node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_mdx("Hello <b>world</b>.")[1] | .mdx_jsx_text_element"#,
                expected: r#"<b>world</b>"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".math"),
        BuiltinSelectorDoc {
            description: "Selects a math node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_math("x^2") | .math"#,
                expected: r#"$$
x^2
$$"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".math_inline"),
        BuiltinSelectorDoc {
            description: "Selects a math inline node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_math_inline("x^2") | .math_inline"#,
                expected: r#"$x^2$"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".mdx_flow_expression"),
        BuiltinSelectorDoc {
            description: "Selects an MDX flow expression node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_mdx("{1 + 1}")[0] | .mdx_flow_expression"#,
                expected: r#"{1 + 1}"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".definition"),
        BuiltinSelectorDoc {
            description: "Selects a definition node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("[ref]: https://example.com")[0] | .definition"#,
                expected: r#"[ref]: https://example.com"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".task"),
        BuiltinSelectorDoc {
            description: "Selects a task list node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("- [ ] Todo\n- [x] Done")[0] | .task"#,
                expected: r#"- [ ] Todo"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".todo"),
        BuiltinSelectorDoc {
            description: "Selects a todo item in the task list node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("- [ ] Todo\n- [x] Done")[0] | .todo"#,
                expected: r#"- [ ] Todo"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new(".done"),
        BuiltinSelectorDoc {
            description: "Selects a done item in the task list node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_markdown("- [ ] Todo\n- [x] Done")[1] | .done"#,
                expected: r#"- [x] Done"#,
            }],
            capability: None,
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
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
            },
        );
    map.insert(
            SmolStr::new("_get_markdown_position"),
            BuiltinFunctionDoc {
            description: "Internal function to get the position information of a markdown node, returning row and column data if available.",
            params: &["markdown_node"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
            },
        );
    map.insert(
        SmolStr::new("is_debug_mode"),
        BuiltinFunctionDoc {
            description: "Checks if the runtime is currently in debug mode, returning true if a debugger is attached.",
            params: &[],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_ast_get_args"),
        BuiltinFunctionDoc {
            description: "Internal function to extract arguments from an AST call expression, returning an array of arguments to their AST nodes.",
            params: &["ast_node"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_ast_to_code"),
        BuiltinFunctionDoc {
            description: "Internal function to convert an AST node back to its source code representation as a string.",
            params: &["ast_node"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_csv_parse"),
        BuiltinFunctionDoc {
            description: "Parses a CSV string into an array of arrays, using the specified delimiter and header options.",
            params: &["csv_string", "delimiter", "has_header"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_levenshtein_distance"),
        BuiltinFunctionDoc {
            description: "Calculates the Levenshtein edit distance between two strings.",
            params: &["s1", "s2"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_jaro_distance"),
        BuiltinFunctionDoc {
            description: "Calculates the Jaro distance between two strings (0.0 to 1.0, where 1.0 is an exact match).",
            params: &["s1", "s2"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_jaro_winkler_distance"),
        BuiltinFunctionDoc {
            description: "Calculates the Jaro-Winkler distance between two strings, boosting scores for matching prefixes.",
            params: &["s1", "s2"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_xml_parse"),
        BuiltinFunctionDoc {
            description: "Parses an XML string and returns the corresponding data structure.",
            params: &["xml_string"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_json_parse"),
        BuiltinFunctionDoc {
            description: "Parses a JSON string into a data structure.",
            params: &["json_string"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_yaml_parse"),
        BuiltinFunctionDoc {
            description: "Parses a YAML string into a data structure.",
            params: &["yaml_string"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_toon_parse"),
        BuiltinFunctionDoc {
            description: "Parses a TOON string into a data structure.",
            params: &["toon_string"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_toon_stringify"),
        BuiltinFunctionDoc {
            description: "Converts a data structure into a TOON string.",
            params: &["data"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_toml_parse"),
        BuiltinFunctionDoc {
            description: "Parses a TOML string into a data structure.",
            params: &["toml_string"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_gron_parse"),
        BuiltinFunctionDoc {
            description: "Parses gron-style `path = value;` assignment statements into a data structure.",
            params: &["gron_string"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_cbor_parse"),
        BuiltinFunctionDoc {
            description: "Parses a base64-encoded CBOR string or raw bytes into a data structure.",
            params: &["input"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_cbor_stringify"),
        BuiltinFunctionDoc {
            description: "Serializes a value to CBOR bytes.",
            params: &["value"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("_diff"),
        BuiltinFunctionDoc {
            description: "Internal function to compute the difference between two values, returning an array of changes.",
            params: &["value1", "value2"],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );

    map
});

#[derive(Clone, Debug)]
pub struct BuiltinFunctionDoc {
    pub description: &'static str,
    pub params: &'static [&'static str],
    /// Parallel to `params`; a type name (e.g. "string", "number") or "dynamic" per param.
    pub param_types: &'static [&'static str],
    /// Type name of the returned value (e.g. "array", "bool", "dynamic").
    pub returns: &'static str,
    pub examples: &'static [BuiltinExample],
    /// Cargo feature flag required to use this function, if any (e.g. "file-io").
    pub capability: Option<&'static str>,
}

pub static BUILTIN_FUNCTION_DOC: LazyLock<FxHashMap<SmolStr, BuiltinFunctionDoc>> = LazyLock::new(|| {
    let mut map = FxHashMap::with_capacity_and_hasher(112, FxBuildHasher);

    map.insert(
        SmolStr::new("halt"),
        BuiltinFunctionDoc {
            description: "Terminates the program with the given exit code.",
            params: &["exit_code"],
            param_types: &["number"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("error"),
        BuiltinFunctionDoc {
            description: "Raises a user-defined error with the specified message.",
            params: &["message"],
            param_types: &["string"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("exp"),
        BuiltinFunctionDoc {
            description: "Returns the exponential (e^x) of the given number.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"exp(0)"#,
                expected: r#"1"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("print"),
        BuiltinFunctionDoc {
            description: "Prints a message to standard output and returns the current value.",
            params: &["message"],
            param_types: &["string"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("stderr"),
        BuiltinFunctionDoc {
            description: "Prints a message to standard error and returns the current value.",
            params: &["message"],
            param_types: &["string"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("type"),
        BuiltinFunctionDoc {
            description: "Returns the type of the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"type(1)"#,
                expected: r#"number"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("ln"),
        BuiltinFunctionDoc {
            description: "Returns the natural logarithm (base e) of the given number.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"ln(1)"#,
                expected: r#"0"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("log10"),
        BuiltinFunctionDoc {
            description: "Returns the base-10 logarithm of the given number.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"log10(100)"#,
                expected: r#"2"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::ARRAY),
        BuiltinFunctionDoc {
            description: "Creates an array from the given values.",
            params: &["values"],
            param_types: &["dynamic"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"array(1, 2, 3)"#,
                expected: r#"[1, 2, 3]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("flatten"),
        BuiltinFunctionDoc {
            description: "Flattens a nested array into a single level array.",
            params: &["array"],
            param_types: &["array"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"flatten([[1, 2], [3]])"#,
                expected: r#"[1, 2, 3]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("from_date"),
        BuiltinFunctionDoc {
            description: "Converts a date string to a timestamp.",
            params: &["date_str"],
            param_types: &["string"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"from_date("1970-01-01T00:00:00Z")"#,
                expected: r#"0"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_date"),
        BuiltinFunctionDoc {
            description: "Converts a timestamp to a date string with the given format.",
            params: &["timestamp", "format"],
            param_types: &["number", "string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"to_date(0, "%Y-%m-%d")"#,
                expected: r#"1970-01-01"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("now"),
        BuiltinFunctionDoc {
            description: "Returns the current timestamp.",
            params: &[],
            param_types: &[],
            returns: "number",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("gmtime"),
        BuiltinFunctionDoc {
            description: "Converts Unix timestamp (seconds since epoch) to broken-down UTC time array [year, mon (0-11), mday, hour, min, sec, wday (0=Sun), yday (0-365)].",
            params: &["timestamp"],
            param_types: &["number"],
            returns: "array",
            examples: &[BuiltinExample { code: r#"gmtime(0)"#, expected: r#"[1970, 0, 1, 0, 0, 0, 4, 0]"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("localtime"),
        BuiltinFunctionDoc {
            description: "Converts Unix timestamp (seconds since epoch) to broken-down local time array [year, mon (0-11), mday, hour, min, sec, wday (0=Sun), yday (0-365)].",
            params: &["timestamp"],
            param_types: &["number"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("mktime"),
        BuiltinFunctionDoc {
            description: "Converts broken-down UTC time array [year, mon (0-11), mday, hour, min, sec, wday, yday] to Unix timestamp (seconds since epoch).",
            params: &["time_array"],
            param_types: &["array"],
            returns: "number",
            examples: &[BuiltinExample { code: r#"mktime(gmtime(0))"#, expected: r#"0"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("strftime"),
        BuiltinFunctionDoc {
            description: "Formats a Unix timestamp (seconds) as a date string using the given strftime format (e.g. \"%Y-%m-%d\").",
            params: &["timestamp", "format"],
            param_types: &["number", "string"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"strftime(0, "%Y-%m-%d")"#, expected: r#"1970-01-01"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("strptime"),
        BuiltinFunctionDoc {
            description: "Parses a date string using the given strptime format (e.g. \"%Y-%m-%d\") and returns a Unix timestamp (seconds, UTC).",
            params: &["date_str", "format"],
            param_types: &["string", "string"],
            returns: "number",
            examples: &[BuiltinExample { code: r#"strptime("1970-01-01", "%Y-%m-%d")"#, expected: r#"0"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("date_add"),
        BuiltinFunctionDoc {
            description: "Adds n units to a broken-down time array and returns a new array. Units: \"seconds\", \"minutes\", \"hours\", \"days\", \"weeks\", \"months\", \"years\". Month/year arithmetic is calendar-aware.",
            params: &["array", "n", "unit"],
            param_types: &["array", "number", "string"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("date_diff"),
        BuiltinFunctionDoc {
            description: "Returns the difference (array2 - array1) in the given unit. Units: \"seconds\", \"minutes\", \"hours\", \"days\", \"weeks\".",
            params: &["array1", "array2", "unit"],
            param_types: &["array", "array", "string"],
            returns: "number",
            examples: &[BuiltinExample { code: r#"date_diff(gmtime(0), gmtime(86400), "days")"#, expected: r#"1"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("date_relative"),
        BuiltinFunctionDoc {
            description: "Parses a natural-language relative date expression (e.g. \"3 days ago\", \"yesterday\", \"tomorrow\", \"next monday\", \"in 2 weeks\") relative to a base Unix timestamp and returns the resulting Unix timestamp (seconds, UTC).",
            params: &["base_timestamp", "date_str"],
            param_types: &["number", "string"],
            returns: "number",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("base64"),
        BuiltinFunctionDoc {
            description: "Encodes the given string to base64.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"base64("hi")"#,
                expected: r#"aGk="#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("base64d"),
        BuiltinFunctionDoc {
            description: "Decodes the given base64 string.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"base64d("aGk=")"#,
                expected: r#"hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("base64url"),
        BuiltinFunctionDoc {
            description: "Encodes the given string to URL-safe base64.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"base64url("hi")"#,
                expected: r#"aGk"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("base64urld"),
        BuiltinFunctionDoc {
            description: "Decodes the given URL-safe base64 string.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"base64urld(base64url("hi"))"#,
                expected: r#"hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("min"),
        BuiltinFunctionDoc {
            description: "Returns the minimum of two values.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"min(1, 2)"#,
                expected: r#"1"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("max"),
        BuiltinFunctionDoc {
            description: "Returns the maximum of two values.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"max(1, 2)"#,
                expected: r#"2"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("from_html"),
        BuiltinFunctionDoc {
            description: "Converts the given HTML string to Markdown.",
            params: &["html"],
            param_types: &["string"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_html"),
        BuiltinFunctionDoc {
            description: "Converts the given markdown string to HTML.",
            params: &["markdown"],
            param_types: &["string"],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("html_escape"),
        BuiltinFunctionDoc {
            description: "Escapes `&`, `<`, `>`, `\"`, and `'` in the given string as HTML entities.",
            params: &["string"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"html_escape("<a>")"#,
                expected: r#"&lt;a&gt;"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("html_unescape"),
        BuiltinFunctionDoc {
            description: "Decodes named and numeric HTML entities in the given string into their corresponding characters.",
            params: &["string"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"html_unescape("&lt;a&gt;")"#, expected: r#"<a>"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("strip_tags"),
        BuiltinFunctionDoc {
            description: "Removes HTML tags from the given string, keeping the surrounding text content.",
            params: &["string"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"strip_tags("<b>hi</b>")"#,
                expected: r#"hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("sanitize_html"),
        BuiltinFunctionDoc {
            description: "Sanitizes the given HTML string using an allowlist of safe tags and attributes, removing scripts and other XSS vectors.",
            params: &["html"],
            param_types: &["string"],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_string"),
        BuiltinFunctionDoc {
            description: "Converts the given value to a string.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"to_string(1)"#,
                expected: r#"1"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_markdown_string"),
        BuiltinFunctionDoc {
            description: "Converts the given value(s) to a markdown string representation.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_number"),
        BuiltinFunctionDoc {
            description: "Converts the given value to a number.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"to_number("42")"#,
                expected: r#"42"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_boolean"),
        BuiltinFunctionDoc {
            description: "Converts the given value to a boolean. Booleans are returned unchanged, the strings \"true\" and \"false\" are converted to their boolean equivalent, and all other input results in an error.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "bool",
            examples: &[BuiltinExample { code: r#"to_boolean("true")"#, expected: r#"true"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_array"),
        BuiltinFunctionDoc {
            description: "Converts the given value to an array.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"to_array(1)"#,
                expected: r#"[1]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("md5"),
        BuiltinFunctionDoc {
            description: "Computes the MD5 hash of a string or bytes and returns a lowercase hex string.",
            params: &["input"],
            param_types: &["dynamic"],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("sha256"),
        BuiltinFunctionDoc {
            description: "Computes the SHA-256 hash of a string or bytes and returns a lowercase hex string.",
            params: &["input"],
            param_types: &["dynamic"],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("sha512"),
        BuiltinFunctionDoc {
            description: "Computes the SHA-512 hash of a string or bytes and returns a lowercase hex string.",
            params: &["input"],
            param_types: &["dynamic"],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_bytes"),
        BuiltinFunctionDoc {
            description: "Converts a string (UTF-8), array of numbers, or bytes to raw bytes.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "bytes",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("from_hex"),
        BuiltinFunctionDoc {
            description: "Parses a hex string into raw bytes.",
            params: &["hex_string"],
            param_types: &["string"],
            returns: "bytes",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_hex"),
        BuiltinFunctionDoc {
            description: "Encodes raw bytes as a lowercase hex string.",
            params: &["bytes"],
            param_types: &["bytes"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"to_hex(from_hex("6869"))"#,
                expected: r#"6869"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("utf8"),
        BuiltinFunctionDoc {
            description: "Decodes bytes as a UTF-8 string, returning an error if the bytes are not valid UTF-8.",
            params: &["bytes"],
            param_types: &["bytes"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"utf8(to_bytes("hi"))"#,
                expected: r#"hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("xor"),
        BuiltinFunctionDoc {
            description: "Computes the bitwise XOR of two byte arrays of equal length.",
            params: &["bytes1", "bytes2"],
            param_types: &["bytes", "bytes"],
            returns: "bytes",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("band"),
        BuiltinFunctionDoc {
            description: "Computes the bitwise AND of two byte arrays of equal length.",
            params: &["bytes1", "bytes2"],
            param_types: &["bytes", "bytes"],
            returns: "bytes",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("bor"),
        BuiltinFunctionDoc {
            description: "Computes the bitwise OR of two byte arrays of equal length.",
            params: &["bytes1", "bytes2"],
            param_types: &["bytes", "bytes"],
            returns: "bytes",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("bnot"),
        BuiltinFunctionDoc {
            description: "Computes the bitwise NOT (complement) of a byte array.",
            params: &["bytes"],
            param_types: &["bytes"],
            returns: "bytes",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("pack"),
        BuiltinFunctionDoc {
            description: "Packs a number into bytes using the given format. Supported formats: u8, i8, u16be/le, i16be/le, u32be/le, i32be/le, u64be/le, i64be/le, f32be/le, f64be/le.",
            params: &["format", "value"],
            param_types: &["string", "number"],
            returns: "bytes",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("unpack"),
        BuiltinFunctionDoc {
            description: "Unpacks a number from bytes using the given format. Supported formats: u8, i8, u16be/le, i16be/le, u32be/le, i32be/le, u64be/le, i64be/le, f32be/le, f64be/le.",
            params: &["format", "bytes"],
            param_types: &["string", "bytes"],
            returns: "number",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("url_encode"),
        BuiltinFunctionDoc {
            description: "URL-encodes the given string.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"url_encode("a b")"#,
                expected: r#"a%20b"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("url_decode"),
        BuiltinFunctionDoc {
            description: "URL-decodes the given string.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"url_decode("a%20b")"#,
                expected: r#"a b"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("uuid"),
        BuiltinFunctionDoc {
            description: "Generates a random (version 4, RFC 4122) UUID string.",
            params: &[],
            param_types: &[],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("uuid_v4"),
        BuiltinFunctionDoc {
            description: "Generates a random (version 4, RFC 4122) UUID string. Alias of `uuid`.",
            params: &[],
            param_types: &[],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("uuid_v7"),
        BuiltinFunctionDoc {
            description: "Generates a time-ordered (version 7, RFC 9562) UUID string: a millisecond Unix timestamp followed by random bits, so values sort by creation time. The timestamp is plaintext, so prefer uuid/uuid_v4 for unguessable IDs.",
            params: &[],
            param_types: &[],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("rand"),
        BuiltinFunctionDoc {
            description: "Generates a pseudo-random number in the range [0, 1). Not cryptographically secure.",
            params: &[],
            param_types: &[],
            returns: "number",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("rand_int"),
        BuiltinFunctionDoc {
            description: "Generates a pseudo-random integer uniformly distributed in [min, max] (inclusive). Not cryptographically secure.",
            params: &["min", "max"],
            param_types: &["number", "number"],
            returns: "number",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("random_string"),
        BuiltinFunctionDoc {
            description: "Generates a random string of `len` characters, each independently chosen (with replacement) from `charset`. Not cryptographically secure.",
            params: &["len", "charset"],
            param_types: &["number", "string"],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("shuffle"),
        BuiltinFunctionDoc {
            description: "Returns a new array containing the same elements as the input, in a uniformly random order.",
            params: &["array"],
            param_types: &["array"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("sample"),
        BuiltinFunctionDoc {
            description: "Returns n elements sampled from the array without replacement, in random order. Errors if n exceeds the array length.",
            params: &["array", "n"],
            param_types: &["array", "number"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_text"),
        BuiltinFunctionDoc {
            description: "Converts the given markdown node to plain text.",
            params: &["markdown"],
            param_types: &["markdown"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"to_text(to_strong("hi"))"#,
                expected: r#"hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("ends_with"),
        BuiltinFunctionDoc {
            description: "Checks if the given string or byte array ends with the specified suffix.",
            params: &["value", "suffix"],
            param_types: &["dynamic", "dynamic"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"ends_with("hello", "lo")"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("starts_with"),
        BuiltinFunctionDoc {
            description: "Checks if the given string or byte array starts with the specified prefix.",
            params: &["value", "prefix"],
            param_types: &["dynamic", "dynamic"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"starts_with("hello", "he")"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("regex_match"),
        BuiltinFunctionDoc {
            description: "Finds all matches of the given pattern in the string.",
            params: &["string", "pattern"],
            param_types: &["string", "string"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"regex_match("abc123", "[0-9]+")"#,
                expected: r#"["123"]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("is_regex_match"),
        BuiltinFunctionDoc {
            description: "Checks if the given pattern matches the string.",
            params: &["string", "pattern"],
            param_types: &["string", "string"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"is_regex_match("abc", "a.c")"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("is_not_regex_match"),
        BuiltinFunctionDoc {
            description: "Checks if the given pattern does not match the string.",
            params: &["string", "pattern"],
            param_types: &["string", "string"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"is_not_regex_match("abc", "x")"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("scan"),
        BuiltinFunctionDoc {
            description: "Finds all matches of a regular expression pattern in the string. For each match, returns the captured groups as an array if the pattern has capture groups, otherwise returns the whole match as a string.",
            params: &["string", "pattern"],
            param_types: &["string", "string"],
            returns: "array",
            examples: &[BuiltinExample { code: r#"scan("a1b2", "[0-9]")"#, expected: r#"["1", "2"]"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("downcase"),
        BuiltinFunctionDoc {
            description: "Converts the given string to lowercase.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"downcase("ABC")"#,
                expected: r#"abc"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("ascii_downcase"),
        BuiltinFunctionDoc {
            description: "Converts ASCII uppercase letters (A-Z) in the given string to lowercase, leaving all other characters unchanged.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"ascii_downcase("ABC")"#, expected: r#"abc"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("gsub"),
        BuiltinFunctionDoc {
            description: "Replaces all occurrences matching a regular expression pattern with the replacement string.",
            params: &["from", "pattern", "to"],
            param_types: &["string", "string", "string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r##"gsub("a1b2", "[0-9]", "#")"##,
                expected: r#"a#b#"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("replace"),
        BuiltinFunctionDoc {
            description: "Replaces all occurrences of a substring with another substring.",
            params: &["from", "pattern", "to"],
            param_types: &["string", "string", "string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"replace("aXbXc", "X", "-")"#,
                expected: r#"a-b-c"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("repeat"),
        BuiltinFunctionDoc {
            description: "Repeats the given string a specified number of times.",
            params: &["string", "count"],
            param_types: &["string", "number"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"repeat("ab", 3)"#,
                expected: r#"ababab"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("word_wrap"),
        BuiltinFunctionDoc {
            description: "Wraps the given string into lines no wider than the specified display width, breaking on word boundaries (CJK and other wide characters count as two columns).",
            params: &["string", "width"],
            param_types: &["string", "number"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"word_wrap("hello world", 5)"#, expected: r#"hello
world"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("truncate"),
        BuiltinFunctionDoc {
            description: "Truncates the given string to the specified display width, appending the ellipsis string when truncated (CJK and other wide characters count as two columns).",
            params: &["string", "width", "ellipsis"],
            param_types: &["string", "number", "string"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"truncate("hello world", 5, "...")"#, expected: r#"he..."# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("explode"),
        BuiltinFunctionDoc {
            description: "Splits the given string into an array of characters.",
            params: &["string"],
            param_types: &["string"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"explode("ab")"#,
                expected: r#"[97, 98]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("implode"),
        BuiltinFunctionDoc {
            description: "Joins an array of characters into a string.",
            params: &["array"],
            param_types: &["array"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"implode(explode("ab"))"#,
                expected: r#"ab"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("trim"),
        BuiltinFunctionDoc {
            description: "Trims whitespace from both ends of the given string.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"trim("  hi  ")"#,
                expected: r#"hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("ltrim"),
        BuiltinFunctionDoc {
            description: "Trims whitespace from the left end of the given string.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"ltrim("  hi  ")"#,
                expected: r#"hi  "#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("rtrim"),
        BuiltinFunctionDoc {
            description: "Trims whitespace from the right end of the given string.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"rtrim("  hi  ")"#,
                expected: r#"  hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("upcase"),
        BuiltinFunctionDoc {
            description: "Converts the given string to uppercase.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"upcase("abc")"#,
                expected: r#"ABC"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("ascii_upcase"),
        BuiltinFunctionDoc {
            description: "Converts ASCII lowercase letters (a-z) in the given string to uppercase, leaving all other characters unchanged.",
            params: &["input"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"ascii_upcase("abc")"#, expected: r#"ABC"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::SLICE),
        BuiltinFunctionDoc {
            description: "Extracts a substring from the given string.",
            params: &["string", "start", "end"],
            param_types: &["string", "number", "number"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"slice("hello", 1, 3)"#,
                expected: r#"el"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("update"),
        BuiltinFunctionDoc {
            description: "Update the value with specified value.",
            params: &["target_value", "source_value"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("pow"),
        BuiltinFunctionDoc {
            description: "Raises the base to the power of the exponent.",
            params: &["base", "exponent"],
            param_types: &["number", "number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"pow(2, 10)"#,
                expected: r#"1024"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("index"),
        BuiltinFunctionDoc {
            description: "Finds the first occurrence of a substring or byte subsequence. Returns -1 if not found.",
            params: &["value", "needle"],
            param_types: &["dynamic", "dynamic"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"index("hello", "ll")"#,
                expected: r#"2"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("len"),
        BuiltinFunctionDoc {
            description: "Returns the length of the given string or array.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"len("hello")"#,
                expected: r#"5"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("rindex"),
        BuiltinFunctionDoc {
            description: "Finds the last occurrence of a substring or byte subsequence. Returns -1 if not found.",
            params: &["value", "needle"],
            param_types: &["dynamic", "dynamic"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"rindex("hello", "l")"#,
                expected: r#"3"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("token_count"),
        BuiltinFunctionDoc {
            description: "Estimates how many LLM tokens the given text would consume, for context-window budgeting. Uses a lightweight chars-per-token heuristic by default; built with the `tiktoken` Cargo feature, counts exactly via tiktoken-rs instead when `model` (e.g. \"gpt-5\") is given. `model` is optional; without it, the heuristic estimate is always used.",
            params: &["text", "model?"],
            param_types: &["string", "string"],
            returns: "number",
            examples: &[BuiltinExample { code: r#"token_count("Hello, world!")"#, expected: r#"4"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("token_compress"),
        BuiltinFunctionDoc {
            description: "Reduces an array of Markdown nodes to fit within `budget` LLM tokens, preserving structure as much as possible: paragraphs are cut to their first sentence, then lists/tables/code blocks are collapsed to a summary, and only as a last resort is the remaining text hard-truncated. Uses a lightweight chars-per-token heuristic by default; built with the `tiktoken` Cargo feature, counts exactly via tiktoken-rs instead when `model` (e.g. \"gpt-5\") is given. `model` is optional; without it, the heuristic estimate is always used.",
            params: &["nodes", "budget", "model?"],
            param_types: &["array", "number", "string"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("join"),
        BuiltinFunctionDoc {
            description: "Joins the elements of an array into a string with the given separator.",
            params: &["array", "separator"],
            param_types: &["array", "string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"join([1, 2, 3], ",")"#,
                expected: r#"1,2,3"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("reverse"),
        BuiltinFunctionDoc {
            description: "Reverses the given string or array.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"reverse("abc")"#,
                expected: r#"cba"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("sort"),
        BuiltinFunctionDoc {
            description: "Sorts the elements of the given array.",
            params: &["array"],
            param_types: &["array"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"sort([3, 1, 2])"#,
                expected: r#"[1, 2, 3]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("compact"),
        BuiltinFunctionDoc {
            description: "Removes None values from the given array.",
            params: &["array"],
            param_types: &["array"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"compact([1, None, 2])"#,
                expected: r#"[1, 2]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("convert"),
        BuiltinFunctionDoc {
            description: "Converts the input value to the specified format. Supported formats: base64, html, text, uri, heading (#, ##, etc.), blockquote (>), list item (-), or link (URL).",
            params: &["input", "format"],
            param_types: &["dynamic", "string"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("split"),
        BuiltinFunctionDoc {
            description: "Splits the given string by the specified separator.",
            params: &["string", "separator"],
            param_types: &["string", "string"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"split("a,b,c", ",")"#,
                expected: r#"["a", "b", "c"]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("sqrt"),
        BuiltinFunctionDoc {
            description: "Returns the square root of the given number.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"sqrt(9)"#,
                expected: r#"3"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("uniq"),
        BuiltinFunctionDoc {
            description: "Removes duplicate elements from the given array.",
            params: &["array"],
            param_types: &["array"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"uniq([1, 1, 2])"#,
                expected: r#"[1, 2]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::EQ),
        BuiltinFunctionDoc {
            description: "Checks if two values are equal.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"eq(1, 1)"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::NE),
        BuiltinFunctionDoc {
            description: "Checks if two values are not equal.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"ne(1, 2)"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::GT),
        BuiltinFunctionDoc {
            description: "Checks if the first value is greater than the second value.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"gt(2, 1)"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::GTE),
        BuiltinFunctionDoc {
            description: "Checks if the first value is greater than or equal to the second value.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"gte(1, 1)"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::LT),
        BuiltinFunctionDoc {
            description: "Checks if the first value is less than the second value.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"lt(1, 2)"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::LTE),
        BuiltinFunctionDoc {
            description: "Checks if the first value is less than or equal to the second value.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"lte(1, 1)"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::ADD),
        BuiltinFunctionDoc {
            description: "Adds two values.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"add(1, 2)"#,
                expected: r#"3"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::SUB),
        BuiltinFunctionDoc {
            description: "Subtracts the second value from the first value.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"sub(5, 2)"#,
                expected: r#"3"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::DIV),
        BuiltinFunctionDoc {
            description: "Divides the first value by the second value.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"div(6, 2)"#,
                expected: r#"3"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::MUL),
        BuiltinFunctionDoc {
            description: "Multiplies two values.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"mul(2, 3)"#,
                expected: r#"6"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::MOD),
        BuiltinFunctionDoc {
            description: "Calculates the remainder of the division of the first value by the second value.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"mod(7, 3)"#,
                expected: r#"1"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("and"),
        BuiltinFunctionDoc {
            description: "Performs a logical AND operation on two boolean values.",
            params: &["value1", "value2"],
            param_types: &["bool", "bool"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"and(true, false)"#,
                expected: r#"false"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("or"),
        BuiltinFunctionDoc {
            description: "Performs a logical OR operation on two boolean values.",
            params: &["value1", "value2"],
            param_types: &["bool", "bool"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"or(true, false)"#,
                expected: r#"true"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::NOT),
        BuiltinFunctionDoc {
            description: "Performs a logical NOT operation on a boolean value.",
            params: &["value"],
            param_types: &["bool"],
            returns: "bool",
            examples: &[BuiltinExample {
                code: r#"not(true)"#,
                expected: r#"false"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new("round"),
        BuiltinFunctionDoc {
            description: "Rounds the given number to the nearest integer.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"round(3.5)"#,
                expected: r#"4"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("trunc"),
        BuiltinFunctionDoc {
            description: "Truncates the given number to an integer by removing the fractional part.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"trunc(3.9)"#,
                expected: r#"3"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("ceil"),
        BuiltinFunctionDoc {
            description: "Rounds the given number up to the nearest integer.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"ceil(3.2)"#,
                expected: r#"4"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::FLOOR),
        BuiltinFunctionDoc {
            description: "Rounds the given number down to the nearest integer.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"floor(3.8)"#,
                expected: r#"3"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("del"),
        BuiltinFunctionDoc {
            description: "Deletes the element at the specified index in the array or string.",
            params: &["array_or_string", "index"],
            param_types: &["dynamic", "number"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"del([1, 2, 3], 1)"#,
                expected: r#"[1, 3]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("abs"),
        BuiltinFunctionDoc {
            description: "Returns the absolute value of the given number.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"abs(-10)"#,
                expected: r#"10"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::ATTR),
        BuiltinFunctionDoc {
            description: "Retrieves the value of the specified attribute from a markdown node.",
            params: &["markdown", "attribute"],
            param_types: &["markdown", "string"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("set_attr"),
        BuiltinFunctionDoc {
            description: "Sets the value of the specified attribute on a markdown node.",
            params: &["markdown", "attribute", "value"],
            param_types: &["markdown", "string", "dynamic"],
            returns: "markdown",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("set_children"),
        BuiltinFunctionDoc {
            description: "Sets the children nodes of a markdown node. Nodes without children (e.g. text, code) are left unchanged.",
            params: &["markdown", "children"],
            param_types: &["markdown", "array"],
            returns: "markdown",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_md_name"),
        BuiltinFunctionDoc {
            description: "Returns the name of the given markdown node.",
            params: &["markdown"],
            param_types: &["markdown"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"to_md_name(to_h("t", 1))"#,
                expected: r#"h1"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("set_list_ordered"),
        BuiltinFunctionDoc {
            description: "Sets the ordered property of a markdown list node.",
            params: &["list", "ordered"],
            param_types: &["markdown", "bool"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"set_list_ordered(to_md_list("Item", 0), true)"#,
                expected: r#"1. Item"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_md_text"),
        BuiltinFunctionDoc {
            description: "Creates a markdown text node with the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_text("hi")"#,
                expected: r#"hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_image"),
        BuiltinFunctionDoc {
            description: "Creates a markdown image node with the given URL, alt text, and title.",
            params: &["url", "alt", "title"],
            param_types: &["string", "string", "string"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_image("https://example.com/a.png", "Alt", "")"#,
                expected: r#"![Alt](https://example.com/a.png "")"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_code"),
        BuiltinFunctionDoc {
            description: "Creates a markdown code block with the given value and language.",
            params: &["value", "language"],
            param_types: &["dynamic", "string"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_code("x = 1", "python")"#,
                expected: r#"```python
x = 1
```"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_code_inline"),
        BuiltinFunctionDoc {
            description: "Creates an inline markdown code node with the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_code_inline("x")"#,
                expected: r#"`x`"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_h"),
        BuiltinFunctionDoc {
            description: "Creates a markdown heading node with the given value and depth.",
            params: &["value", "depth"],
            param_types: &["dynamic", "number"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_h("Title", 1)"#,
                expected: r#"# Title"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_math"),
        BuiltinFunctionDoc {
            description: "Creates a markdown math block with the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_math("x^2")"#,
                expected: r#"$$
x^2
$$"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_math_inline"),
        BuiltinFunctionDoc {
            description: "Creates an inline markdown math node with the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_math_inline("x^2")"#,
                expected: r#"$x^2$"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_strong"),
        BuiltinFunctionDoc {
            description: "Creates a markdown strong (bold) node with the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_strong("Bold")"#,
                expected: r#"**Bold**"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_em"),
        BuiltinFunctionDoc {
            description: "Creates a markdown emphasis (italic) node with the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_em("Italic")"#,
                expected: r#"*Italic*"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_blockquote"),
        BuiltinFunctionDoc {
            description: "Creates a markdown blockquote node with the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_blockquote("Quote")"#,
                expected: r#"> Quote"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_delete"),
        BuiltinFunctionDoc {
            description: "Creates a markdown delete (strikethrough) node with the given value.",
            params: &["value"],
            param_types: &["dynamic"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_delete("Old")"#,
                expected: r#"~~Old~~"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_callout"),
        BuiltinFunctionDoc {
            description: "Creates a markdown callout node with the given value, kind, and title.",
            params: &["value", "kind", "title"],
            param_types: &["dynamic", "string", "string"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_callout("Note text", "note", "")"#,
                expected: r#"> [!NOTE]
> Note text"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_md_fragment"),
        BuiltinFunctionDoc {
            description: "Creates a markdown fragment node that groups an array of markdown nodes into a single value.",
            params: &["values"],
            param_types: &["array"],
            returns: "markdown",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_hr"),
        BuiltinFunctionDoc {
            description: "Creates a markdown horizontal rule node.",
            params: &[],
            param_types: &[],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_hr()"#,
                expected: r#"***"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_link"),
        BuiltinFunctionDoc {
            description: "Creates a markdown link node  with the given  url and title.",
            params: &["url", "value", "title"],
            param_types: &["string", "dynamic", "string"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_link("https://example.com", "Example", "")"#,
                expected: r#"[Example](https://example.com)"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_md_list"),
        BuiltinFunctionDoc {
            description: "Creates a markdown list node with the given value and indent level.",
            params: &["value", "indent"],
            param_types: &["dynamic", "number"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_list("Item", 0)"#,
                expected: r#"- Item"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_md_table_row"),
        BuiltinFunctionDoc {
            description: "Creates a markdown table row node with the given values.",
            params: &["cells"],
            param_types: &["array"],
            returns: "markdown",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_md_table_cell"),
        BuiltinFunctionDoc {
            description: "Creates a markdown table cell node with the given value at the specified row and column.",
            params: &["value", "row", "column"],
            param_types: &["dynamic", "number", "number"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_table_cell("A1", 0, 0)"#,
                expected: r#"A1"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_md_table_align"),
        BuiltinFunctionDoc {
            description: "Creates a markdown table alignment row node from an array of alignments (\"left\", \"right\", \"center\", \"none\").",
            params: &["aligns"],
            param_types: &["array"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"to_md_table_align(["left", "right"])"#,
                expected: r#"|:---|---:|"#,
            }],
            capability: None,
        },
    );

    map.insert(
        SmolStr::new("get_title"),
        BuiltinFunctionDoc {
            description: "Returns the title of a markdown node.",
            params: &["node"],
            param_types: &["markdown"],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("get_url"),
        BuiltinFunctionDoc {
            description: "Returns the url of a markdown node.",
            params: &["node"],
            param_types: &["markdown"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"get_url(to_link("https://example.com", "Example", ""))"#,
                expected: r#"https://example.com"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("get_location"),
        BuiltinFunctionDoc {
            description: "Returns the source position of a markdown node as a dict with start_line, start_column, end_line, and end_column, or None if the node has no position info.",
            params: &["node"],
            param_types: &["markdown"],
            returns: "dict",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("set_check"),
        BuiltinFunctionDoc {
            description: "Creates a markdown list node with the given checked state.",
            params: &["list", "checked"],
            param_types: &["markdown", "bool"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"set_check(to_md_list("Item", 0), true)"#,
                expected: r#"- [x] Item"#,
            }],
            capability: None,
        },
    );
    map.insert(
            SmolStr::new("set_ref"),
            BuiltinFunctionDoc {
            description: "Sets the reference identifier for markdown nodes that support references (e.g., Definition, LinkRef, ImageRef, Footnote, FootnoteRef).",
            params: &["node", "reference_id"],
            param_types: &["markdown", "string"],
            returns: "markdown",
            examples: &[],
            capability: None,
            },
        );
    map.insert(
        SmolStr::new("set_code_block_lang"),
        BuiltinFunctionDoc {
            description: "Sets the language of a markdown code block node.",
            params: &["code_block", "language"],
            param_types: &["markdown", "string"],
            returns: "markdown",
            examples: &[BuiltinExample {
                code: r#"set_code_block_lang(to_code("x", "python"), "rust")"#,
                expected: r#"```rust
x
```"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::DICT),
        BuiltinFunctionDoc {
            description: "Creates a new, empty dict.",
            params: &[],
            param_types: &[],
            returns: "dict",
            examples: &[BuiltinExample {
                code: r#"dict()"#,
                expected: r#"{}"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::GET),
        BuiltinFunctionDoc {
            description: "Retrieves a value from a dict by its key. Returns None if the key is not found.",
            params: &["obj", "key"],
            param_types: &["dict", "dynamic"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
            SmolStr::new("set"),
            BuiltinFunctionDoc {
                description: "Sets a key-value pair in a dict. If the key exists, its value is updated. Returns the modified map.",
                params: &["obj", "key", "value"],
            param_types: &["dict", "dynamic", "dynamic"],
            returns: "dict",
            examples: &[],
            capability: None,
            },
        );
    map.insert(
        SmolStr::new("keys"),
        BuiltinFunctionDoc {
            description: "Returns an array of keys from the dict.",
            params: &["dict"],
            param_types: &["dict"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("values"),
        BuiltinFunctionDoc {
            description: "Returns an array of values from the dict.",
            params: &["dict"],
            param_types: &["dict"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("entries"),
        BuiltinFunctionDoc {
            description: "Returns an array of key-value pairs from the dict as arrays.",
            params: &["dict"],
            param_types: &["dict"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::RANGE),
        BuiltinFunctionDoc {
            description: "Creates an array from start to end with an optional step.",
            params: &["start", "end", "step"],
            param_types: &["number", "number", "number"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r#"range(0, 5, 1)"#,
                expected: r#"[0, 1, 2, 3, 4, 5]"#,
            }],
            capability: None,
        },
    );
    map.insert(
            SmolStr::new("insert"),
            BuiltinFunctionDoc {
            description: "Inserts a value into an array or string at the specified index, or into a dict with the specified key.",
            params: &["target", "index_or_key", "value"],
            param_types: &["dynamic", "dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample { code: r#"insert([1, 2, 3], 1, "x")"#, expected: r#"[1, "x", 2, 3]"# }],
            capability: None,
            },
        );

    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("read_file"),
        BuiltinFunctionDoc {
            description: "Reads the contents of a file at the given path and returns it as a string. Requires the --allow-read CLI flag; otherwise returns a runtime error.",
            params: &["path"],
            param_types: &["string"],
            returns: "string",
            examples: &[],
            capability: Some("file-io"),
        },
    );
    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("file_exists"),
        BuiltinFunctionDoc {
            description: "Checks if a file exists at the given path. Requires the --allow-read CLI flag; otherwise returns a runtime error.",
            params: &["path"],
            param_types: &["string"],
            returns: "bool",
            examples: &[],
            capability: Some("file-io"),
        },
    );
    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("file_size"),
        BuiltinFunctionDoc {
            description: "Returns the size, in bytes, of the file at the given path. Requires the --allow-read CLI flag; otherwise returns a runtime error.",
            params: &["path"],
            param_types: &["string"],
            returns: "number",
            examples: &[],
            capability: Some("file-io"),
        },
    );
    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("read_file_bytes"),
        BuiltinFunctionDoc {
            description: "Reads the contents of a file at the given path and returns it as raw bytes. Requires the --allow-read CLI flag; otherwise returns a runtime error.",
            params: &["path"],
            param_types: &["string"],
            returns: "bytes",
            examples: &[],
            capability: Some("file-io"),
        },
    );
    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("collection"),
        BuiltinFunctionDoc {
            description: "Recursively reads every Markdown file in the given directory (including subdirectories and symlinked files/directories) and returns an array of `{path, title, frontmatter, content}` dicts, sorted by path, so they can be filtered, sorted, or aggregated as a single dataset. `content` holds the file's Markdown nodes with frontmatter stripped. Symlink cycles are detected and only visited once. `respect_gitignore` is optional (default `false`); when `true`, dotfiles/dot-directories and any path matched by a `.gitignore` in `dir` or a subdirectory are skipped, with closer `.gitignore` files taking precedence, same as `git`. Requires the --allow-read CLI flag; otherwise returns a runtime error.",
            params: &["dir", "respect_gitignore?"],
            param_types: &["string", "boolean"],
            returns: "array",
            examples: &[],
            capability: Some("file-io"),
        },
    );
    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("write_file"),
        BuiltinFunctionDoc {
            description: "Writes content (string or bytes) to the file at the given path, creating or truncating it. Requires the --allow-write CLI flag; otherwise returns a runtime error.",
            params: &["path", "content"],
            param_types: &["string", "dynamic"],
            returns: "dynamic",
            examples: &[],
            capability: Some("file-io"),
        },
    );
    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("embed_images"),
        BuiltinFunctionDoc {
            description: "Inlines an `.image` node's local file into its `url` as a base64 `data:` URI, resolving the path relative to the given base directory (default \".\") and inferring the MIME type from the file extension. URLs that are already `data:` URIs or contain a `://` scheme (e.g. `https://`), and non-image nodes, are left unchanged. Requires the --allow-read CLI flag; otherwise returns a runtime error.",
            params: &["base_dir"],
            param_types: &["string"],
            returns: "markdown",
            examples: &[],
            capability: Some("file-io"),
        },
    );
    #[cfg(feature = "file-io")]
    map.insert(
        SmolStr::new("extract_images"),
        BuiltinFunctionDoc {
            description: "Decodes an `.image` node's base64 `data:` URI and writes the bytes to a file under the given directory, named by the content's MD5 hash with an extension inferred from the MIME type, then replaces `url` with that file's path. Nodes whose `url` is not a base64 `data:` URI, including non-image nodes, are left unchanged. Requires the --allow-write CLI flag; otherwise returns a runtime error.",
            params: &["dir"],
            param_types: &["string"],
            returns: "markdown",
            examples: &[],
            capability: Some("file-io"),
        },
    );
    #[cfg(feature = "http")]
    map.insert(
        SmolStr::new("http"),
        BuiltinFunctionDoc {
            description: "Performs an HTTPS request with the given method (a string or symbol, e.g. \"post\" or :post — get, post, put, delete, patch, head, ... are all supported) and returns the response body as a string. An optional body argument (string) sends a request body regardless of method, and an optional headers argument (a dict of string to string, e.g. {\"Content-Type\": \"application/json\"}) is applied to the request. Requires the --allow-net CLI flag; otherwise returns a runtime error. Only https:// URLs are allowed.",
            params: &["method", "url", "body", "headers"],
            param_types: &["string", "string", "string", "dict"],
            returns: "string",
            examples: &[],
            capability: Some("http"),
        },
    );
    #[cfg(all(feature = "http", feature = "mock-io"))]
    map.insert(
        SmolStr::new("mock_fetch"),
        BuiltinFunctionDoc {
            description: "Seeds the response body a subsequent http() call for the given url returns, instead of making a real request. Only meaningful against a mock Io (e.g. mq-test's engine); other Io implementations return a runtime error.",
            params: &["url", "body"],
            param_types: &["string", "string"],
            returns: "dynamic",
            examples: &[],
            capability: Some("http"),
        },
    );
    #[cfg(feature = "process-io")]
    map.insert(
        SmolStr::new("system"),
        BuiltinFunctionDoc {
            description: "Runs command as a child process, optionally passing an array of string args, and returns its captured stdout as a string. The command is never run through a shell, so shell metacharacters in args are never interpreted. A non-zero exit status is a runtime error that includes the process's stderr. Requires the --allow-run CLI flag; otherwise returns a runtime error.",
            params: &["command", "args"],
            param_types: &["string", "array"],
            returns: "string",
            examples: &[],
            capability: Some("process-io"),
        },
    );
    #[cfg(feature = "css-selector")]
    map.insert(
        SmolStr::new("css"),
        BuiltinFunctionDoc {
            description: "Returns the outer HTML of every element in the html string matching the CSS selector, as an array of strings. Queries the raw HTML directly instead of going through the -I html Markdown conversion, so tags, classes, ids, and data-* attributes that conversion discards are still available.",
            params: &["html", "selector"],
            param_types: &["string", "string"],
            returns: "array",
            examples: &[BuiltinExample { code: r#"css("<div class=\"a\"><p>hi</p></div>", "p")"#, expected: r#"["<p>hi</p>"]"# }],
            capability: Some("css-selector"),
        },
    );
    #[cfg(feature = "css-selector")]
    map.insert(
        SmolStr::new("css_text"),
        BuiltinFunctionDoc {
            description: "Returns the text content of every element in the html string matching the CSS selector, as an array of strings.",
            params: &["html", "selector"],
            param_types: &["string", "string"],
            returns: "array",
            examples: &[BuiltinExample { code: r#"css_text("<div><p>hi</p></div>", "p")"#, expected: r#"["hi"]"# }],
            capability: Some("css-selector"),
        },
    );
    #[cfg(feature = "css-selector")]
    map.insert(
        SmolStr::new("css_attr"),
        BuiltinFunctionDoc {
            description: "Returns the value of the named attribute for every element in the html string matching the CSS selector, as an array; elements without that attribute produce None.",
            params: &["html", "selector", "name"],
            param_types: &["string", "string", "string"],
            returns: "array",
            examples: &[BuiltinExample { code: r#"css_attr("<a href=\"https://example.com\">x</a>", "a", "href")"#, expected: r#"["https://example.com"]"# }],
            capability: Some("css-selector"),
        },
    );

    map.insert(
        SmolStr::new("basename"),
        BuiltinFunctionDoc {
            description: "Returns the final component of a path string (e.g. \"file.txt\" from \"/a/b/file.txt\").",
            params: &["path"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"basename("/a/b/file.txt")"#,
                expected: r#"file.txt"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("dirname"),
        BuiltinFunctionDoc {
            description: "Returns the parent directory of a path string (e.g. \"/a/b\" from \"/a/b/file.txt\"). Returns \".\" if the path has no parent.",
            params: &["path"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"dirname("/a/b/file.txt")"#, expected: r#"/a/b"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("extname"),
        BuiltinFunctionDoc {
            description: "Returns the extension of a file path including the leading dot (e.g. \".txt\" from \"file.txt\"). Returns an empty string if there is no extension.",
            params: &["path"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"extname("file.txt")"#, expected: r#".txt"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("stem"),
        BuiltinFunctionDoc {
            description: "Returns the file name without the extension (e.g. \"file\" from \"/a/b/file.txt\").",
            params: &["path"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"stem("/a/b/file.txt")"#,
                expected: r#"file"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("path_join"),
        BuiltinFunctionDoc {
            description: "Joins a base path with a component path and returns the resulting path string (e.g. path_join(\"/a/b\", \"c.txt\") → \"/a/b/c.txt\").",
            params: &["base", "component"],
            param_types: &["string", "string"],
            returns: "string",
            examples: &[BuiltinExample { code: r#"path_join("/a/b", "c.txt")"#, expected: r#"/a/b/c.txt"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("glob_match"),
        BuiltinFunctionDoc {
            description: "Checks whether the given path matches the glob pattern (e.g. \"*.md\", \"docs/**/*.rs\"), commonly used to filter file lists.",
            params: &["pattern", "path"],
            param_types: &["string", "string"],
            returns: "bool",
            examples: &[BuiltinExample { code: r#"glob_match("*.md", "readme.md")"#, expected: r#"true"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("negate"),
        BuiltinFunctionDoc {
            description: "Returns the negation of the given number.",
            params: &["number"],
            param_types: &["number"],
            returns: "number",
            examples: &[BuiltinExample {
                code: r#"negate(5)"#,
                expected: r#"-5"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("intern"),
        BuiltinFunctionDoc {
            description: "Interns the given string, returning a canonical reference for efficient comparison.",
            params: &["string"],
            param_types: &["string"],
            returns: "string",
            examples: &[BuiltinExample {
                code: r#"intern("hi")"#,
                expected: r#"hi"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("nan"),
        BuiltinFunctionDoc {
            description: "Returns a Not-a-Number (NaN) value.",
            params: &[],
            param_types: &[],
            returns: "number",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("infinite"),
        BuiltinFunctionDoc {
            description: "Returns an infinite number value.",
            params: &[],
            param_types: &[],
            returns: "number",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("coalesce"),
        BuiltinFunctionDoc {
            description: "Returns the first non-None value from the two provided arguments.",
            params: &["value1", "value2"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[BuiltinExample {
                code: r#"coalesce(None, 5)"#,
                expected: r#"5"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("input"),
        BuiltinFunctionDoc {
            description: "Reads a line from standard input and returns it as a string.",
            params: &[],
            param_types: &[],
            returns: "string",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("all_symbols"),
        BuiltinFunctionDoc {
            description: "Returns an array of all interned symbols.",
            params: &[],
            param_types: &[],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_markdown"),
        BuiltinFunctionDoc {
            description: "Parses a markdown string and returns an array of markdown nodes.",
            params: &["markdown_string"],
            param_types: &["string"],
            returns: "array",
            examples: &[BuiltinExample {
                code: r##"to_markdown("# Hi")"##,
                expected: r#"[# Hi]"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("to_mdx"),
        BuiltinFunctionDoc {
            description: "Parses an MDX string and returns an array of MDX nodes.",
            params: &["mdx_string"],
            param_types: &["string"],
            returns: "array",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("set_variable"),
        BuiltinFunctionDoc {
            description: "Sets a symbol or variable in the current environment with the given value.",
            params: &["symbol_or_string", "value"],
            param_types: &["dynamic", "dynamic"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("get_variable"),
        BuiltinFunctionDoc {
            description: "Retrieves the value of a symbol or variable from the current environment.",
            params: &["symbol_or_string"],
            param_types: &["dynamic"],
            returns: "dynamic",
            examples: &[],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::BREAKPOINT),
        BuiltinFunctionDoc {
            description: "Sets a breakpoint for debugging; execution will pause at this point if a debugger is attached.",
            params: &[],
            param_types: &[],
            returns: "dynamic",
            examples: &[],
            capability: None,
            },
    );
    map.insert(
        SmolStr::new("capture"),
        BuiltinFunctionDoc {
            description: "Captures named groups from the given string based on the specified regular expression pattern and returns them as a dictionary keyed by group names.",
            params: &["string", "pattern"],
            param_types: &["string", "string"],
            returns: "dict",
            examples: &[BuiltinExample {
                code: r#"capture("v1.2.3", "(?P<major>[0-9]+)")"#,
                expected: r#"{"major": "1"}"#,
            }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::SHIFT_LEFT),
        BuiltinFunctionDoc {
            description: "Performs a left shift operation on the given value: for numbers, this is a bitwise left shift by the specified number of positions; for strings, this removes characters from the start; for Markdown headings, this increases the heading level accordingly.",
            params: &["value", "shift_amount"],
            param_types: &["dynamic", "number"],
            returns: "dynamic",
            examples: &[BuiltinExample { code: r#"shift_left(1, 2)"#, expected: r#"4"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new(constants::builtins::SHIFT_RIGHT),
        BuiltinFunctionDoc {
            description: "Performs a bitwise right shift on numbers, slices characters from the end of strings, and adjusts Markdown heading levels when applied to headings, using the given shift amount.",
            params: &["value", "shift_amount"],
            param_types: &["dynamic", "number"],
            returns: "dynamic",
            examples: &[BuiltinExample { code: r#"shift_right(8, 2)"#, expected: r#"2"# }],
            capability: None,
        },
    );
    map.insert(
        SmolStr::new("partial"),
        BuiltinFunctionDoc {
            description: "Creates a new function by partially applying the given arguments to the specified function.",
            params: &["function", "arg1", "arg2", "..."],
            param_types: &["function", "dynamic"],
            returns: "function",
            examples: &[],
            capability: None,
        },
    );

    map
});

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("")]
    InvalidBase64String(#[from] base64::DecodeError),
    #[error("")]
    NotDefined(FunctionName, Vec<String>),
    #[error("")]
    UndefinedReference(String, Vec<String>),
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
            env::EnvError::UndefinedReference(name, candidates) => Error::UndefinedReference(name, candidates),
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
            Error::NotDefined(name, candidates) => RuntimeError::NotDefined(
                (*get_token(token_arena, node.token_id)).clone(),
                name.clone(),
                candidates.clone().into(),
            ),
            Error::UndefinedReference(a, candidates) => RuntimeError::UndefinedReference(
                (*get_token(token_arena, node.token_id)).clone(),
                a.clone(),
                candidates.clone().into(),
            ),
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
        || {
            #[cfg(not(feature = "sync"))]
            let candidates = env.borrow().defined_names();
            #[cfg(feature = "sync")]
            let candidates = env.read().unwrap().defined_names();

            Err(Error::NotDefined(ident.to_string(), candidates))
        },
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
/// - `Callout`: filters by kind using string args (e.g. `.callout("NOTE")`)
/// - `WikiLink`: filters by target using string args (e.g. `.wikilink("Some Page")`)
/// - `Embed`: filters by target using string args (e.g. `.embed("image.png")`)
/// - `LinkRef`: filters by identifier using string args (e.g. `.link_ref("ref")`)
/// - `ImageRef`: filters by identifier using string args (e.g. `.image_ref("ref")`)
/// - `FootnoteRef`: filters by identifier using string args (e.g. `.footnote_ref("1")`)
/// - `Footnote`: filters by identifier using string args (e.g. `.footnote("1")`)
/// - `Definition`: filters by identifier using string args (e.g. `.definition("ref")`)
/// - `MdxJsxFlowElement`: filters by element name using string args (e.g. `.mdx_jsx_flow_element("Alert")`)
/// - `MdxJsxTextElement`: filters by element name using string args (e.g. `.mdx_jsx_text_element("Alert")`)
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
        Selector::Callout => {
            let kinds = collect_string_values(args);

            if kinds.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::Callout(mq_markdown::Callout { kind, .. }) = node {
                kinds.iter().any(|k| k == kind)
            } else {
                false
            }
        }
        Selector::WikiLink => {
            let targets = collect_string_values(args);

            if targets.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::WikiLink(mq_markdown::WikiLink { target, .. }) = node {
                targets.iter().any(|t| t == target)
            } else {
                false
            }
        }
        Selector::Embed => {
            let targets = collect_string_values(args);

            if targets.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::Embed(mq_markdown::Embed { target, .. }) = node {
                targets.iter().any(|t| t == target)
            } else {
                false
            }
        }
        Selector::LinkRef => {
            let idents = collect_string_values(args);

            if idents.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::LinkRef(mq_markdown::LinkRef { ident, .. }) = node {
                idents.iter().any(|i| i == ident)
            } else {
                false
            }
        }
        Selector::ImageRef => {
            let idents = collect_string_values(args);

            if idents.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::ImageRef(mq_markdown::ImageRef { ident, .. }) = node {
                idents.iter().any(|i| i == ident)
            } else {
                false
            }
        }
        Selector::FootnoteRef => {
            let idents = collect_string_values(args);

            if idents.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::FootnoteRef(mq_markdown::FootnoteRef { ident, .. }) = node {
                idents.iter().any(|i| i == ident)
            } else {
                false
            }
        }
        Selector::Footnote => {
            let idents = collect_string_values(args);

            if idents.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::Footnote(mq_markdown::Footnote { ident, .. }) = node {
                idents.iter().any(|i| i == ident)
            } else {
                false
            }
        }
        Selector::Definition => {
            let idents = collect_string_values(args);

            if idents.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::Definition(mq_markdown::Definition { ident, .. }) = node {
                idents.iter().any(|i| i == ident)
            } else {
                false
            }
        }
        Selector::MdxJsxFlowElement => {
            let names = collect_string_values(args);

            if names.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name, .. }) = node {
                let node_name = name.as_deref().unwrap_or("");
                names.iter().any(|n| n == node_name)
            } else {
                false
            }
        }
        Selector::MdxJsxTextElement => {
            let names = collect_string_values(args);

            if names.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name, .. }) = node {
                let node_name = name.as_deref().unwrap_or("");
                names.iter().any(|n| n == node_name)
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
        Selector::WikiLink => node.is_wikilink(),
        Selector::Callout => node.is_callout(),
        Selector::Embed => node.is_embed(),
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
    RuntimeValue::Array(Shared::new(
        extract_recursive_node(node)
            .into_iter()
            .map(RuntimeValue::new_markdown)
            .collect(),
    ))
}

/// Wraps `text` on word boundaries to `width` display columns (Unicode East Asian Width, so CJK counts as 2).
fn word_wrap(text: &str, width: usize) -> String {
    if width == 0 {
        return text.to_string();
    }

    text.split('\n')
        .map(|line| word_wrap_line(line, width))
        .collect::<Vec<_>>()
        .join("\n")
}

fn word_wrap_line(line: &str, width: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in line.split_whitespace() {
        let word_width = word.width();

        if word_width > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                current_width = 0;
            }
            for c in word.chars() {
                let cw = c.width().unwrap_or(0);
                if current_width > 0 && current_width + cw > width {
                    lines.push(std::mem::take(&mut current));
                    current_width = 0;
                }
                current.push(c);
                current_width += cw;
            }
            continue;
        }

        let needed = if current.is_empty() {
            word_width
        } else {
            current_width + 1 + word_width
        };

        if !current.is_empty() && needed > width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }

        if !current.is_empty() {
            current.push(' ');
            current_width += 1;
        }
        current.push_str(word);
        current_width += word_width;
    }

    lines.push(current);
    lines.join("\n")
}

/// Truncates `text` to `max_width` display columns, appending `ellipsis` (CJK counts as 2 columns).
fn truncate_str(text: &str, max_width: usize, ellipsis: &str) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }

    let ellipsis_width = ellipsis.width();

    if ellipsis_width >= max_width {
        let mut result = String::new();
        let mut width = 0;
        for c in ellipsis.chars() {
            let cw = c.width().unwrap_or(0);
            if width + cw > max_width {
                break;
            }
            result.push(c);
            width += cw;
        }
        return result;
    }

    let target_width = max_width - ellipsis_width;
    let mut result = String::new();
    let mut width = 0;

    for c in text.chars() {
        let cw = c.width().unwrap_or(0);
        if width + cw > target_width {
            break;
        }
        result.push(c);
        width += cw;
    }

    result.push_str(ellipsis);
    result
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
                return Ok(RuntimeValue::empty_array());
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
            Ok(RuntimeValue::Array(Shared::new(repeated_array)))
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

    #[cfg(all(feature = "http", feature = "mock-io"))]
    use crate::io::MemIo;
    #[cfg(any(feature = "file-io", feature = "http", feature = "process-io"))]
    use crate::io::{NativeIo, SandboxedIo};

    use super::*;

    #[rstest]
    #[case("type", vec![RuntimeValue::String("test".into())], Ok(RuntimeValue::String("string".into())))]
    #[case("len", vec![RuntimeValue::String("test".into())], Ok(RuntimeValue::Number(4.into())))]
    #[case("token_count", vec![RuntimeValue::String("Hello, world!".into()), RuntimeValue::String("gpt-4".into())], Ok(RuntimeValue::Number(4.into())))]
    #[case("token_count", vec![RuntimeValue::String("".into()), RuntimeValue::String("gpt-4".into())], Ok(RuntimeValue::Number(0.into())))]
    #[case("token_count", vec![RuntimeValue::String("Hello, world!".into())], Ok(RuntimeValue::Number(4.into())))]
    #[case("token_count", vec![RuntimeValue::String("".into())], Ok(RuntimeValue::Number(0.into())))]
    #[case(
        "token_compress",
        vec![
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Box::new(Node::from("hi".to_string())), None)])),
            RuntimeValue::Number(1000.into()),
        ],
        Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Box::new(Node::from("hi".to_string())), None)])))
    )]
    #[case(
        "token_compress",
        vec![RuntimeValue::None, RuntimeValue::Number(100.into())],
        Ok(RuntimeValue::Array(Shared::new(vec![])))
    )]
    #[case(
        "token_compress",
        vec![
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Box::new(Node::from("hi".to_string())), None)])),
            RuntimeValue::Number(1000.into()),
            RuntimeValue::String("gpt-4".into()),
        ],
        Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Box::new(Node::from("hi".to_string())), None)])))
    )]
    #[case(
        "token_compress",
        vec![RuntimeValue::None, RuntimeValue::Number(100.into()), RuntimeValue::String("gpt-4".into())],
        Ok(RuntimeValue::Array(Shared::new(vec![])))
    )]
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
    #[case("unknown_func", vec![RuntimeValue::Number(1.0.into())], Error::NotDefined("unknown_func".to_string(), vec![]))]
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
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Number(1970.into()), // year
                RuntimeValue::Number(0.into()),    // mon (Jan=0)
                RuntimeValue::Number(1.into()),    // mday
                RuntimeValue::Number(0.into()),    // hour
                RuntimeValue::Number(0.into()),    // min
                RuntimeValue::Number(0.into()),    // sec
                RuntimeValue::Number(4.into()),    // wday (Thu=4)
                RuntimeValue::Number(0.into()),    // yday
            ]))
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
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Number(2024.into()), // year
                RuntimeValue::Number(0.into()),    // mon (Jan=0)
                RuntimeValue::Number(1.into()),    // mday
                RuntimeValue::Number(0.into()),    // hour
                RuntimeValue::Number(0.into()),    // min
                RuntimeValue::Number(0.into()),    // sec
                RuntimeValue::Number(1.into()),    // wday (Mon=1)
                RuntimeValue::Number(0.into()),    // yday
            ]))
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

    fn call_uuid_fn(name: &str) -> String {
        let env = Shared::new(SharedCell::new(Env::default()));
        match eval_builtin(&RuntimeValue::None, &Ident::new(name), vec![], &env).unwrap() {
            RuntimeValue::String(s) => s.to_string(),
            other => panic!("{name} should return a string, got {other:?}"),
        }
    }

    fn assert_uuid_shape(uuid: &str, expected_version: char) {
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5, "uuid {uuid} should have 5 hyphen-separated groups");
        assert_eq!(
            [
                parts[0].len(),
                parts[1].len(),
                parts[2].len(),
                parts[3].len(),
                parts[4].len()
            ],
            [8, 4, 4, 4, 12]
        );
        assert!(uuid.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert_eq!(parts[2].chars().next().unwrap(), expected_version, "version nibble");
        assert!(
            matches!(parts[3].chars().next().unwrap(), '8' | '9' | 'a' | 'b'),
            "variant nibble should be 10xxxxxx, got {}",
            parts[3]
        );
    }

    #[test]
    fn test_uuid_is_version_4() {
        assert_uuid_shape(&call_uuid_fn("uuid"), '4');
    }

    #[test]
    fn test_uuid_v4_is_version_4() {
        assert_uuid_shape(&call_uuid_fn("uuid_v4"), '4');
    }

    #[test]
    fn test_uuid_v7_is_version_7() {
        assert_uuid_shape(&call_uuid_fn("uuid_v7"), '7');
    }

    #[test]
    fn test_uuid_calls_are_unique() {
        let values: std::collections::HashSet<String> = (0..200).map(|_| call_uuid_fn("uuid")).collect();
        assert_eq!(values.len(), 200, "uuid() should not repeat across calls");
    }

    #[test]
    fn test_rand_is_in_unit_range() {
        let env = Shared::new(SharedCell::new(Env::default()));
        for _ in 0..200 {
            match eval_builtin(&RuntimeValue::None, &Ident::new("rand"), vec![], &env).unwrap() {
                RuntimeValue::Number(n) => assert!((0.0..1.0).contains(&n.value()), "rand() out of [0, 1)"),
                other => panic!("rand() should return a number, got {other:?}"),
            }
        }
    }

    #[rstest]
    #[case(1, 10)]
    #[case(-5, 5)]
    #[case(7, 7)]
    fn test_rand_int_within_bounds(#[case] min: i64, #[case] max: i64) {
        let env = Shared::new(SharedCell::new(Env::default()));
        for _ in 0..200 {
            let result = eval_builtin(
                &RuntimeValue::None,
                &Ident::new("rand_int"),
                vec![RuntimeValue::Number(min.into()), RuntimeValue::Number(max.into())],
                &env,
            )
            .unwrap();
            match result {
                RuntimeValue::Number(n) => {
                    let v = n.to_int();
                    assert!((min..=max).contains(&v), "rand_int({min}, {max}) produced {v}");
                }
                other => panic!("rand_int should return a number, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_rand_int_invalid_range_errors() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("rand_int"),
            vec![RuntimeValue::Number(10.into()), RuntimeValue::Number(1.into())],
            &env,
        );
        assert!(result.is_err(), "rand_int(10, 1) should error since min > max");
    }

    #[test]
    fn test_random_string_uses_only_charset_chars_and_requested_length() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("random_string"),
            vec![RuntimeValue::Number(12.into()), RuntimeValue::String("abc".into())],
            &env,
        )
        .unwrap();
        match result {
            RuntimeValue::String(s) => {
                assert_eq!(s.chars().count(), 12);
                assert!(s.chars().all(|c| "abc".contains(c)));
            }
            other => panic!("random_string should return a string, got {other:?}"),
        }
    }

    #[test]
    fn test_random_string_zero_length_is_empty() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("random_string"),
            vec![RuntimeValue::Number(0.into()), RuntimeValue::String("abc".into())],
            &env,
        )
        .unwrap();
        assert_eq!(result, RuntimeValue::String("".into()));
    }

    #[test]
    fn test_random_string_empty_charset_errors() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("random_string"),
            vec![RuntimeValue::Number(5.into()), RuntimeValue::String("".into())],
            &env,
        );
        assert!(
            result.is_err(),
            "random_string(5, \"\") should error since charset is empty"
        );
    }

    #[test]
    fn test_random_string_calls_are_unique() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let values: std::collections::HashSet<String> = (0..200)
            .map(|_| {
                match eval_builtin(
                    &RuntimeValue::None,
                    &Ident::new("random_string"),
                    vec![
                        RuntimeValue::Number(16.into()),
                        RuntimeValue::String("abcdefghijklmnopqrstuvwxyz0123456789".into()),
                    ],
                    &env,
                )
                .unwrap()
                {
                    RuntimeValue::String(s) => s.to_string(),
                    other => panic!("random_string should return a string, got {other:?}"),
                }
            })
            .collect();
        assert_eq!(
            values.len(),
            200,
            "random_string(16, ...) should not repeat across calls"
        );
    }

    #[test]
    fn test_shuffle_preserves_elements() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let input: Vec<RuntimeValue> = (1..=10).map(|n| RuntimeValue::Number(n.into())).collect();
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("shuffle"),
            vec![RuntimeValue::Array(Shared::new(input.clone()))],
            &env,
        )
        .unwrap();
        match result {
            RuntimeValue::Array(shuffled) => {
                assert_eq!(shuffled.len(), input.len());
                let mut sorted_input = input.clone();
                let mut sorted_shuffled = (*shuffled).clone();
                sorted_input.sort_by(|a, b| a.partial_cmp(b).unwrap());
                sorted_shuffled.sort_by(|a, b| a.partial_cmp(b).unwrap());
                assert_eq!(sorted_input, sorted_shuffled);
            }
            other => panic!("shuffle should return an array, got {other:?}"),
        }
    }

    #[test]
    fn test_sample_returns_subset_without_duplicates() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let input: Vec<RuntimeValue> = (1..=10).map(|n| RuntimeValue::Number(n.into())).collect();
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("sample"),
            vec![
                RuntimeValue::Array(Shared::new(input.clone())),
                RuntimeValue::Number(4.into()),
            ],
            &env,
        )
        .unwrap();
        match result {
            RuntimeValue::Array(sampled) => {
                assert_eq!(sampled.len(), 4);
                for v in sampled.iter() {
                    assert!(input.contains(v));
                }
                let mut sorted = (*sampled).clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                sorted.dedup();
                assert_eq!(sorted.len(), 4, "sample should not contain duplicates");
            }
            other => panic!("sample should return an array, got {other:?}"),
        }
    }

    #[test]
    fn test_sample_n_exceeds_length_errors() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let input: Vec<RuntimeValue> = (1..=3).map(|n| RuntimeValue::Number(n.into())).collect();
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("sample"),
            vec![RuntimeValue::Array(Shared::new(input)), RuntimeValue::Number(10.into())],
            &env,
        );
        assert!(
            result.is_err(),
            "sample(arr, n) should error when n exceeds the array length"
        );
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

    #[rstest]
    #[case("2024-01-01", "%Y-%m-%d", 1704067200_i64)]
    #[case("1970-01-01T00:00:00", "%Y-%m-%dT%H:%M:%S", 0_i64)]
    #[case("01/02/2024", "%m/%d/%Y", 1704153600_i64)]
    fn test_strptime(#[case] date_str: &str, #[case] fmt: &str, #[case] expected: i64) {
        let ident = Ident::new("strptime");
        let args = vec![RuntimeValue::String(date_str.into()), RuntimeValue::String(fmt.into())];
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            args,
            &Shared::new(SharedCell::new(Env::default())),
        )
        .unwrap();
        assert_eq!(result, RuntimeValue::Number(expected.into()));
    }

    #[test]
    fn test_strptime_invalid_format() {
        let ident = Ident::new("strptime");
        let args = vec![
            RuntimeValue::String("not-a-date".into()),
            RuntimeValue::String("%Y-%m-%d".into()),
        ];
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            args,
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert!(result.is_err());
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
        let bad_arr = RuntimeValue::Array(Shared::new(vec![RuntimeValue::String("x".into()); 8]));
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
        let bad_arr = RuntimeValue::Array(Shared::new(vec![RuntimeValue::String("x".into()); 8]));
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

    // date_relative: base is 2024-01-15T00:00:00Z (Monday), 1705276800
    #[rstest]
    #[case("now", 1705276800_i64)]
    #[case("today", 1705276800_i64)]
    #[case("yesterday", 1705190400_i64)]
    #[case("tomorrow", 1705363200_i64)]
    #[case("3 days ago", 1705017600_i64)]
    #[case("in 2 weeks", 1706486400_i64)]
    #[case("next monday", 1705881600_i64)]
    #[case("last friday", 1705017600_i64)]
    fn test_date_relative(#[case] input: &str, #[case] expected_secs: i64) {
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_relative"),
            vec![
                RuntimeValue::Number(1705276800_i64.into()),
                RuntimeValue::String(input.into()),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, RuntimeValue::Number(expected_secs.into()));
    }

    #[test]
    fn test_date_relative_invalid_expression() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_relative"),
            vec![
                RuntimeValue::Number(1705276800_i64.into()),
                RuntimeValue::String("not a relative date".into()),
            ],
            &env,
        );
        match result {
            Err(Error::Runtime(msg)) => assert!(
                msg.starts_with("date_relative:"),
                "expected date_relative prefix, got: {msg}"
            ),
            other => panic!("expected Runtime error, got: {other:?}"),
        }
    }

    #[test]
    fn test_date_relative_invalid_types() {
        let env = Shared::new(SharedCell::new(Env::default()));
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("date_relative"),
            vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
            &env,
        );
        assert!(matches!(result, Err(Error::InvalidTypes(_, _))));
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
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(Some(1), Some(true)),
        true
    )]
    #[case::list_with_wrong_index(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(Some(2), Some(true)),
        false
    )]
    #[case::list_without_index(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(None, None),
        true
    )]
    #[case::task_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Task,
        true
    )]
    #[case::task_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Task,
        true
    )]
    #[case::task_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
        Selector::Task,
        false
    )]
    #[case::todo_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Todo,
        true
    )]
    #[case::todo_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Todo,
        false
    )]
    #[case::todo_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
        Selector::Todo,
        false
    )]
    #[case::done_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Done,
        true
    )]
    #[case::done_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Done,
        false
    )]
    #[case::done_list(
        Node::List(mq_markdown::List { start: None, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
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
            RuntimeValue::Array(Shared::new(vec![
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
            ]))
        );
    }

    #[test]
    fn test_eval_recursive_selector_leaf_node() {
        let node = Node::Text(mq_markdown::Text {
            value: "leaf".into(),
            position: None,
        });
        let result = eval_selector(&node, &Selector::Recursive);
        assert_eq!(result, RuntimeValue::Array(Shared::new(vec![])));
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
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::new_markdown(inner_text),
                RuntimeValue::new_markdown(heading),
            ]))
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
            vec![RuntimeValue::Dict(Shared::new(d1)), RuntimeValue::Dict(Shared::new(d2))],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::Dict(Shared::new(expected))));
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
            vec![RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("key".into()),
                RuntimeValue::String("value".into()),
            ]))],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(
            result,
            Ok(RuntimeValue::Dict(Shared::new(BTreeMap::from([(
                "key".into(),
                RuntimeValue::String("value".into())
            )]))))
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
        assert_eq!(result1, Ok(RuntimeValue::Array(Shared::new(vec![]))));

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
                let keys_str: Vec<String> = Shared::unwrap_or_clone(keys_array)
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
        assert_eq!(result1, Ok(RuntimeValue::Array(Shared::new(vec![]))));

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
        let mut value = RuntimeValue::Array(Shared::new(array));
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
        let mut value = RuntimeValue::Array(Shared::new(array));
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
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("1".to_string()),
                RuntimeValue::String("2".to_string()),
                RuntimeValue::String("3".to_string()),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("4".to_string()),
                RuntimeValue::String("5".to_string()),
                RuntimeValue::String("6".to_string()),
            ])),
        ])))
    )]
    #[case::single_row_no_header(
        "x,y",
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("x".to_string()),
                RuntimeValue::String("y".to_string()),
            ])),
        ])))
    )]
    #[case::empty_no_header(
        "",
        Ok(RuntimeValue::Array(Shared::new(vec![])))
    )]
    #[case::ragged_rows_no_header(
        "a,b,c\n1,2\n3,4,5,6",
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("1".to_string()),
                RuntimeValue::String("2".to_string()),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("3".to_string()),
                RuntimeValue::String("4".to_string()),
                RuntimeValue::String("5".to_string()),
                RuntimeValue::String("6".to_string()),
            ])),
        ])))
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
            Ok(RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(alice)),
                RuntimeValue::Dict(Shared::new(bob)),
            ])))
        }
    )]
    #[case::single_row_with_header(
        "id,value\n1,hello",
        {
            let mut row = BTreeMap::new();
            row.insert(Ident::new("id"), RuntimeValue::String("1".to_string()));
            row.insert(Ident::new("value"), RuntimeValue::String("hello".to_string()));
            Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(row))])))
        }
    )]
    #[case::quoted_fields_with_header(
        "name,note\n\"Doe, Jane\",\"says \"\"hi\"\"\"",
        {
            let mut row = BTreeMap::new();
            row.insert(Ident::new("name"), RuntimeValue::String("Doe, Jane".to_string()));
            row.insert(Ident::new("note"), RuntimeValue::String("says \"hi\"".to_string()));
            Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(row))])))
        }
    )]
    #[case::ragged_rows_with_header(
        "a,b,c\n1,2\n3,4,5,6",
        {
            let mut short_row = BTreeMap::new();
            short_row.insert(Ident::new("a"), RuntimeValue::String("1".to_string()));
            short_row.insert(Ident::new("b"), RuntimeValue::String("2".to_string()));
            short_row.insert(Ident::new("c"), RuntimeValue::String("".to_string()));
            let mut long_row = BTreeMap::new();
            long_row.insert(Ident::new("a"), RuntimeValue::String("3".to_string()));
            long_row.insert(Ident::new("b"), RuntimeValue::String("4".to_string()));
            long_row.insert(Ident::new("c"), RuntimeValue::String("5".to_string()));
            Ok(RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(short_row)),
                RuntimeValue::Dict(Shared::new(long_row)),
            ])))
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
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("1".to_string()),
                RuntimeValue::String("2".to_string()),
                RuntimeValue::String("3".to_string()),
            ])),
        ])))
    )]
    #[case::tsv_with_header(
        "name\tage\nAlice\t30",
        "\t",
        true,
        {
            let mut row = BTreeMap::new();
            row.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            row.insert(Ident::new("age"), RuntimeValue::String("30".to_string()));
            Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(row))])))
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
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::array(
        r#"[1, 2, 3]"#,
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])))
    )]
    #[case::nested(
        r#"{"a": [true, null], "b": {"c": 1.2}}"#,
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("a"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Boolean(true),
                RuntimeValue::NONE,
            ])));
            let mut inner = BTreeMap::new();
            inner.insert(Ident::new("c"), RuntimeValue::Number(1.2.into()));
            map.insert(Ident::new("b"), RuntimeValue::Dict(Shared::new(inner)));
            Ok(RuntimeValue::Dict(Shared::new(map)))
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
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::sequence(
        "- 1\n- 2\n- 3",
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])))
    )]
    #[case::nested(
        "a:\n  b: 42",
        {
            let mut inner = BTreeMap::new();
            inner.insert(Ident::new("b"), RuntimeValue::Number(42.into()));
            let mut map = BTreeMap::new();
            map.insert(Ident::new("a"), RuntimeValue::Dict(Shared::new(inner)));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::boolean(
        "flag: true",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("flag"), RuntimeValue::Boolean(true));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::null(
        "value: null",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("value"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::float(
        "ratio: 1.5",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("ratio"), RuntimeValue::Number(1.5.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::multi_document(
        "a: 1\n---\nb: 2\n",
        {
            let mut first = BTreeMap::new();
            first.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
            let mut second = BTreeMap::new();
            second.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
            Ok(RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(first)),
                RuntimeValue::Dict(Shared::new(second)),
            ])))
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
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::nested_indent(
        "parent:\n  child: value",
        {
            let mut child_map = BTreeMap::new();
            child_map.insert(Ident::new("child"), RuntimeValue::String("value".to_string()));
            let mut parent_map = BTreeMap::new();
            parent_map.insert(Ident::new("parent"), RuntimeValue::Dict(Shared::new(child_map)));
            Ok(RuntimeValue::Dict(Shared::new(parent_map)))
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
            map.insert(Ident::new("hikes"), RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(row1)), RuntimeValue::Dict(Shared::new(row2))])));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::inline_array(
        "items[3]: 1, 2, 3",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("items"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Number(3.into()),
            ])));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::expanded_array(
        "items[2]:\n  - 1\n  - 2",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("items"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
            ])));
            Ok(RuntimeValue::Dict(Shared::new(map)))
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
            Ok(RuntimeValue::Dict(Shared::new(map)))
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

    // Each case is a lone `RuntimeValue`: multi-key dicts are exercised separately in
    // `test_toon_stringify_round_trip` instead of here, since `RuntimeValue::Dict` (a
    // `BTreeMap<Ident, _>`) orders keys by interned symbol id, which isn't stable across
    // a test binary run and would make an exact-string assertion on more than one key flaky.
    #[rstest]
    #[case::string(RuntimeValue::String("hello".to_string()), "hello")]
    #[case::number(RuntimeValue::Number(42.into()), "42")]
    #[case::bool_true(RuntimeValue::TRUE, "true")]
    #[case::bool_false(RuntimeValue::FALSE, "false")]
    #[case::none(RuntimeValue::NONE, "null")]
    #[case::single_key_dict(
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            RuntimeValue::Dict(Shared::new(map))
        },
        "name: Alice"
    )]
    #[case::array_of_primitives(
        RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])),
        "[2]: 1,2"
    )]
    #[case::empty_array(RuntimeValue::Array(Shared::new(vec![])), "[0]:")]
    #[case::empty_dict(RuntimeValue::Dict(Shared::new(BTreeMap::new())), "")]
    #[case::empty_string_needs_quoting(RuntimeValue::String("".to_string()), "\"\"")]
    #[case::numeric_like_string_needs_quoting(RuntimeValue::String("123".to_string()), "\"123\"")]
    #[case::keyword_like_string_needs_quoting(RuntimeValue::String("true".to_string()), "\"true\"")]
    #[case::string_with_colon_needs_quoting(RuntimeValue::String("a:b".to_string()), "\"a:b\"")]
    #[case::string_with_delimiter_needs_quoting(RuntimeValue::String("a,b".to_string()), "\"a,b\"")]
    #[case::string_starting_with_dash_needs_quoting(RuntimeValue::String("-x".to_string()), "\"-x\"")]
    fn test_toon_stringify(#[case] input: RuntimeValue, #[case] expected: &str) {
        let ident = Ident::new("_toon_stringify");
        let result = eval_builtin(
            &RuntimeValue::None,
            &ident,
            vec![input],
            &Shared::new(SharedCell::new(Env::default())),
        );
        assert_eq!(result, Ok(RuntimeValue::String(expected.to_string())));
    }

    fn toon_tabular_row(id: i64, name: &str) -> RuntimeValue {
        let mut map = BTreeMap::new();
        map.insert(Ident::new("id"), RuntimeValue::Number(id.into()));
        map.insert(Ident::new("name"), RuntimeValue::String(name.to_string()));
        RuntimeValue::Dict(Shared::new(map))
    }

    // Structural round trip (stringify then re-parse) rather than an exact string:
    // `RuntimeValue`'s `Dict` equality ignores key order, so this stays robust regardless
    // of how the global symbol interner happens to order a multi-key dict's fields.
    #[rstest]
    #[case::tabular_array(RuntimeValue::Array(Shared::new(vec![
        toon_tabular_row(1, "Blue Lake"),
        toon_tabular_row(2, "Ridge Trail"),
    ])))]
    #[case::nested_dict({
        let mut inner = BTreeMap::new();
        inner.insert(Ident::new("inner"), RuntimeValue::Number(1.into()));
        let mut outer = BTreeMap::new();
        outer.insert(Ident::new("outer"), RuntimeValue::Dict(Shared::new(inner)));
        RuntimeValue::Dict(Shared::new(outer))
    })]
    #[case::non_uniform_array(RuntimeValue::Array(Shared::new(vec![
        RuntimeValue::Number(1.into()),
        toon_tabular_row(2, "Ridge Trail"),
        RuntimeValue::String("text".to_string()),
    ])))]
    fn test_toon_stringify_round_trip(#[case] original: RuntimeValue) {
        let env = Shared::new(SharedCell::new(Env::default()));

        let ident_stringify = Ident::new("_toon_stringify");
        let stringified = eval_builtin(&RuntimeValue::None, &ident_stringify, vec![original.clone()], &env).unwrap();
        let RuntimeValue::String(toon_str) = stringified else {
            panic!("expected a string result");
        };

        let ident_parse = Ident::new("_toon_parse");
        let round_tripped = eval_builtin(
            &RuntimeValue::None,
            &ident_parse,
            vec![RuntimeValue::String(toon_str)],
            &env,
        )
        .unwrap();

        assert_eq!(round_tripped, original);
    }

    #[rstest]
    #[case::simple_kv(
        "name = \"Alice\"\nage = 30",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::boolean(
        "enabled = true\ndisabled = false",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("enabled"), RuntimeValue::Boolean(true));
            map.insert(Ident::new("disabled"), RuntimeValue::Boolean(false));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::nested_table(
        "[server]\nhost = \"localhost\"\nport = 8080",
        {
            let mut inner = BTreeMap::new();
            inner.insert(Ident::new("host"), RuntimeValue::String("localhost".to_string()));
            inner.insert(Ident::new("port"), RuntimeValue::Number(8080.into()));
            let mut map = BTreeMap::new();
            map.insert(Ident::new("server"), RuntimeValue::Dict(Shared::new(inner)));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
    #[case::array(
        "tags = [\"rust\", \"toml\"]",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("tags"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("rust".to_string()),
                RuntimeValue::String("toml".to_string()),
            ])));
            Ok(RuntimeValue::Dict(Shared::new(map)))
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
            Ok(RuntimeValue::Dict(Shared::new(map)))
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
    #[case::simple_map(
        // {"name": "Alice", "age": 30} encoded as CBOR then base64
        "omRuYW1lZUFsaWNlY2FnZRge",
        {
            let mut map = BTreeMap::new();
            map.insert(Ident::new("name"), RuntimeValue::String("Alice".to_string()));
            map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
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
        assert_eq!(result.unwrap(), RuntimeValue::Dict(Shared::new(expected)));
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
        RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(0.into()),
            RuntimeValue::Number(255.into()),
            RuntimeValue::Number(128.into()),
        ])),
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
    #[case::array_with_non_number(RuntimeValue::Array(Shared::new(vec![RuntimeValue::String("x".to_string())])))]
    #[case::array_with_negative(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number((-1i64).into())])))]
    #[case::array_with_256(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(256i64.into())])))]
    #[case::array_with_fractional(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.5f64.into())])))]
    #[case::array_with_nan(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(f64::NAN.into())])))]
    #[case::array_with_infinity(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(f64::INFINITY.into())])))]
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
            root.insert(Ident::new("children"), RuntimeValue::empty_array());
            root.insert(Ident::new("text"), RuntimeValue::String("hello".to_string()));
            Ok(RuntimeValue::Dict(Shared::new(root)))
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
            root.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs)));
            root.insert(Ident::new("children"), RuntimeValue::empty_array());
            root.insert(Ident::new("text"), RuntimeValue::String("hello".to_string()));
            Ok(RuntimeValue::Dict(Shared::new(root)))
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
            child1.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs1)));
            child1.insert(Ident::new("children"), RuntimeValue::empty_array());
            child1.insert(Ident::new("text"), RuntimeValue::String("hello".to_string()));

            let mut child2 = BTreeMap::new();
            let mut attrs2 = BTreeMap::new();
            attrs2.insert(Ident::new("id"), RuntimeValue::String("2".to_string()));
            child2.insert(Ident::new("tag"), RuntimeValue::String("child".to_string()));
            child2.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs2)));
            child2.insert(Ident::new("children"), RuntimeValue::empty_array());
            child2.insert(Ident::new("text"), RuntimeValue::String("world".to_string()));

            root.insert(Ident::new("tag"), RuntimeValue::String("root".to_string()));
            root.insert(Ident::new("attributes"), RuntimeValue::new_dict());
            root.insert(Ident::new("children"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(child1)),
                RuntimeValue::Dict(Shared::new(child2)),
            ])));
            root.insert(Ident::new("text"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(Shared::new(root)))
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
            child.insert(Ident::new("attributes"), RuntimeValue::Dict(Shared::new(attrs)));
            child.insert(Ident::new("children"), RuntimeValue::empty_array());
            child.insert(Ident::new("text"), RuntimeValue::NONE);

            root.insert(Ident::new("tag"), RuntimeValue::String("root".to_string()));
            root.insert(Ident::new("attributes"), RuntimeValue::new_dict());
            root.insert(Ident::new("children"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(child)),
            ])));
            root.insert(Ident::new("text"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(Shared::new(root)))
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
                RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into())])),
                RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(2.into())])),
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
    #[case::number_array(vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())]))], vec![1u8, 2u8])]
    #[case::empty(vec![], vec![])]
    #[case::ignores_strings(vec![RuntimeValue::String("x".into())], vec![])]
    fn test_collect_depth_values(#[case] args: Vec<RuntimeValue>, #[case] expected: Vec<u8>) {
        assert_eq!(collect_depth_values(&args), expected);
    }

    #[rstest]
    #[case::single_string(vec![RuntimeValue::String("rust".into())], vec!["rust".to_string()])]
    #[case::multiple_strings(vec![RuntimeValue::String("rust".into()), RuntimeValue::String("go".into())], vec!["rust".to_string(), "go".to_string()])]
    #[case::string_array(vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::String("rust".into()), RuntimeValue::String("go".into())]))], vec!["rust".to_string(), "go".to_string()])]
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
    #[case::callout_kind_match(
        Node::Callout(mq_markdown::Callout { kind: "NOTE".to_string(), title: None, values: vec![], position: None }),
        Selector::Callout,
        vec![RuntimeValue::String("NOTE".into())],
        true
    )]
    #[case::callout_kind_no_match(
        Node::Callout(mq_markdown::Callout { kind: "WARNING".to_string(), title: None, values: vec![], position: None }),
        Selector::Callout,
        vec![RuntimeValue::String("NOTE".into())],
        false
    )]
    #[case::callout_multi_kind_match(
        Node::Callout(mq_markdown::Callout { kind: "TIP".to_string(), title: None, values: vec![], position: None }),
        Selector::Callout,
        vec![RuntimeValue::String("NOTE".into()), RuntimeValue::String("TIP".into())],
        true
    )]
    #[case::callout_no_args_fallback(
        Node::Callout(mq_markdown::Callout { kind: "NOTE".to_string(), title: None, values: vec![], position: None }),
        Selector::Callout,
        vec![],
        true
    )]
    #[case::callout_non_callout_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::Callout,
        vec![RuntimeValue::String("NOTE".into())],
        false
    )]
    #[case::wikilink_target_match(
        Node::WikiLink(mq_markdown::WikiLink { target: "Some Page".to_string(), text: None, position: None }),
        Selector::WikiLink,
        vec![RuntimeValue::String("Some Page".into())],
        true
    )]
    #[case::wikilink_target_no_match(
        Node::WikiLink(mq_markdown::WikiLink { target: "Other Page".to_string(), text: None, position: None }),
        Selector::WikiLink,
        vec![RuntimeValue::String("Some Page".into())],
        false
    )]
    #[case::wikilink_multi_target_match(
        Node::WikiLink(mq_markdown::WikiLink { target: "Other Page".to_string(), text: None, position: None }),
        Selector::WikiLink,
        vec![RuntimeValue::String("Some Page".into()), RuntimeValue::String("Other Page".into())],
        true
    )]
    #[case::wikilink_no_args_fallback(
        Node::WikiLink(mq_markdown::WikiLink { target: "Some Page".to_string(), text: None, position: None }),
        Selector::WikiLink,
        vec![],
        true
    )]
    #[case::wikilink_non_wikilink_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::WikiLink,
        vec![RuntimeValue::String("Some Page".into())],
        false
    )]
    #[case::embed_target_match(
        Node::Embed(mq_markdown::Embed { target: "image.png".to_string(), display: None, position: None }),
        Selector::Embed,
        vec![RuntimeValue::String("image.png".into())],
        true
    )]
    #[case::embed_target_no_match(
        Node::Embed(mq_markdown::Embed { target: "note.md".to_string(), display: None, position: None }),
        Selector::Embed,
        vec![RuntimeValue::String("image.png".into())],
        false
    )]
    #[case::embed_multi_target_match(
        Node::Embed(mq_markdown::Embed { target: "note.md".to_string(), display: None, position: None }),
        Selector::Embed,
        vec![RuntimeValue::String("image.png".into()), RuntimeValue::String("note.md".into())],
        true
    )]
    #[case::embed_no_args_fallback(
        Node::Embed(mq_markdown::Embed { target: "image.png".to_string(), display: None, position: None }),
        Selector::Embed,
        vec![],
        true
    )]
    #[case::embed_non_embed_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::Embed,
        vec![RuntimeValue::String("image.png".into())],
        false
    )]
    #[case::link_ref_ident_match(
        Node::LinkRef(mq_markdown::LinkRef { ident: "ref".to_string(), label: None, values: vec![], position: None }),
        Selector::LinkRef,
        vec![RuntimeValue::String("ref".into())],
        true
    )]
    #[case::link_ref_ident_no_match(
        Node::LinkRef(mq_markdown::LinkRef { ident: "other".to_string(), label: None, values: vec![], position: None }),
        Selector::LinkRef,
        vec![RuntimeValue::String("ref".into())],
        false
    )]
    #[case::link_ref_multi_ident_match(
        Node::LinkRef(mq_markdown::LinkRef { ident: "other".to_string(), label: None, values: vec![], position: None }),
        Selector::LinkRef,
        vec![RuntimeValue::String("ref".into()), RuntimeValue::String("other".into())],
        true
    )]
    #[case::link_ref_no_args_fallback(
        Node::LinkRef(mq_markdown::LinkRef { ident: "ref".to_string(), label: None, values: vec![], position: None }),
        Selector::LinkRef,
        vec![],
        true
    )]
    #[case::link_ref_non_link_ref_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::LinkRef,
        vec![RuntimeValue::String("ref".into())],
        false
    )]
    #[case::image_ref_ident_match(
        Node::ImageRef(mq_markdown::ImageRef { alt: "".to_string(), ident: "ref".to_string(), label: None, position: None }),
        Selector::ImageRef,
        vec![RuntimeValue::String("ref".into())],
        true
    )]
    #[case::image_ref_ident_no_match(
        Node::ImageRef(mq_markdown::ImageRef { alt: "".to_string(), ident: "other".to_string(), label: None, position: None }),
        Selector::ImageRef,
        vec![RuntimeValue::String("ref".into())],
        false
    )]
    #[case::image_ref_multi_ident_match(
        Node::ImageRef(mq_markdown::ImageRef { alt: "".to_string(), ident: "other".to_string(), label: None, position: None }),
        Selector::ImageRef,
        vec![RuntimeValue::String("ref".into()), RuntimeValue::String("other".into())],
        true
    )]
    #[case::image_ref_no_args_fallback(
        Node::ImageRef(mq_markdown::ImageRef { alt: "".to_string(), ident: "ref".to_string(), label: None, position: None }),
        Selector::ImageRef,
        vec![],
        true
    )]
    #[case::image_ref_non_image_ref_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::ImageRef,
        vec![RuntimeValue::String("ref".into())],
        false
    )]
    #[case::footnote_ref_ident_match(
        Node::FootnoteRef(mq_markdown::FootnoteRef { ident: "1".to_string(), label: None, position: None }),
        Selector::FootnoteRef,
        vec![RuntimeValue::String("1".into())],
        true
    )]
    #[case::footnote_ref_ident_no_match(
        Node::FootnoteRef(mq_markdown::FootnoteRef { ident: "2".to_string(), label: None, position: None }),
        Selector::FootnoteRef,
        vec![RuntimeValue::String("1".into())],
        false
    )]
    #[case::footnote_ref_multi_ident_match(
        Node::FootnoteRef(mq_markdown::FootnoteRef { ident: "2".to_string(), label: None, position: None }),
        Selector::FootnoteRef,
        vec![RuntimeValue::String("1".into()), RuntimeValue::String("2".into())],
        true
    )]
    #[case::footnote_ref_no_args_fallback(
        Node::FootnoteRef(mq_markdown::FootnoteRef { ident: "1".to_string(), label: None, position: None }),
        Selector::FootnoteRef,
        vec![],
        true
    )]
    #[case::footnote_ref_non_footnote_ref_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::FootnoteRef,
        vec![RuntimeValue::String("1".into())],
        false
    )]
    #[case::footnote_ident_match(
        Node::Footnote(mq_markdown::Footnote { ident: "1".to_string(), values: vec![], position: None }),
        Selector::Footnote,
        vec![RuntimeValue::String("1".into())],
        true
    )]
    #[case::footnote_ident_no_match(
        Node::Footnote(mq_markdown::Footnote { ident: "2".to_string(), values: vec![], position: None }),
        Selector::Footnote,
        vec![RuntimeValue::String("1".into())],
        false
    )]
    #[case::footnote_multi_ident_match(
        Node::Footnote(mq_markdown::Footnote { ident: "2".to_string(), values: vec![], position: None }),
        Selector::Footnote,
        vec![RuntimeValue::String("1".into()), RuntimeValue::String("2".into())],
        true
    )]
    #[case::footnote_no_args_fallback(
        Node::Footnote(mq_markdown::Footnote { ident: "1".to_string(), values: vec![], position: None }),
        Selector::Footnote,
        vec![],
        true
    )]
    #[case::footnote_non_footnote_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::Footnote,
        vec![RuntimeValue::String("1".into())],
        false
    )]
    #[case::definition_ident_match(
        Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://example.com".to_string()), title: None, ident: "ref".to_string(), label: None }),
        Selector::Definition,
        vec![RuntimeValue::String("ref".into())],
        true
    )]
    #[case::definition_ident_no_match(
        Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://example.com".to_string()), title: None, ident: "other".to_string(), label: None }),
        Selector::Definition,
        vec![RuntimeValue::String("ref".into())],
        false
    )]
    #[case::definition_multi_ident_match(
        Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://example.com".to_string()), title: None, ident: "other".to_string(), label: None }),
        Selector::Definition,
        vec![RuntimeValue::String("ref".into()), RuntimeValue::String("other".into())],
        true
    )]
    #[case::definition_no_args_fallback(
        Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://example.com".to_string()), title: None, ident: "ref".to_string(), label: None }),
        Selector::Definition,
        vec![],
        true
    )]
    #[case::definition_non_definition_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::Definition,
        vec![RuntimeValue::String("ref".into())],
        false
    )]
    #[case::mdx_jsx_flow_element_name_match(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("Alert".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        vec![RuntimeValue::String("Alert".into())],
        true
    )]
    #[case::mdx_jsx_flow_element_name_no_match(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        vec![RuntimeValue::String("Alert".into())],
        false
    )]
    #[case::mdx_jsx_flow_element_multi_name_match(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        vec![RuntimeValue::String("Alert".into()), RuntimeValue::String("div".into())],
        true
    )]
    #[case::mdx_jsx_flow_element_no_args_fallback(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("Alert".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        vec![],
        true
    )]
    #[case::mdx_jsx_flow_element_non_matching_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::MdxJsxFlowElement,
        vec![RuntimeValue::String("Alert".into())],
        false
    )]
    #[case::mdx_jsx_text_element_name_match(
        Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name: Some(SmolStr::new("Alert")), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxTextElement,
        vec![RuntimeValue::String("Alert".into())],
        true
    )]
    #[case::mdx_jsx_text_element_name_no_match(
        Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name: Some(SmolStr::new("span")), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxTextElement,
        vec![RuntimeValue::String("Alert".into())],
        false
    )]
    #[case::mdx_jsx_text_element_multi_name_match(
        Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name: Some(SmolStr::new("span")), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxTextElement,
        vec![RuntimeValue::String("Alert".into()), RuntimeValue::String("span".into())],
        true
    )]
    #[case::mdx_jsx_text_element_no_args_fallback(
        Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name: Some(SmolStr::new("Alert")), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxTextElement,
        vec![],
        true
    )]
    #[case::mdx_jsx_text_element_non_matching_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::MdxJsxTextElement,
        vec![RuntimeValue::String("Alert".into())],
        false
    )]
    #[case::non_heading_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
    #[case::list_index_match(
        Node::List(mq_markdown::List { index: 2, level: 0, checked: None, ordered: false, start: None, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(2.into())],
        true
    )]
    #[case::list_index_no_match(
        Node::List(mq_markdown::List { index: 0, level: 0, checked: None, ordered: false, start: None, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
    #[case::list_multi_index_match(
        Node::List(mq_markdown::List { index: 3, level: 0, checked: None, ordered: false, start: None, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into())],
        true
    )]
    #[case::list_no_args_fallback(
        Node::List(mq_markdown::List { index: 0, level: 0, checked: None, ordered: false, start: None, values: vec![], position: None }),
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
    fn test_read_capability_gate_and_success() {
        use std::io::Write;

        let get = |entry: &RuntimeValue, key: &str| match entry {
            RuntimeValue::Dict(d) => d.get(&Ident::new(key)).cloned().unwrap_or(RuntimeValue::NONE),
            other => panic!("expected Dict, got {other:?}"),
        };
        let as_entries = |result: RuntimeValue| match result {
            RuntimeValue::Array(entries) => entries,
            other => panic!("expected Array, got {other:?}"),
        };

        let mut text_tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
        text_tmp.write_all(b"hello").expect("failed to write");
        let text_path = text_tmp.path().to_string_lossy().to_string();

        let mut bytes_tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
        bytes_tmp.write_all(&[0x89, 0x50, 0x4e, 0x47]).expect("failed to write");
        let bytes_path = bytes_tmp.path().to_string_lossy().to_string();

        let collection_dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(collection_dir.path().join("root.md"), "# Root\n").expect("failed to write");

        // No ambient Io guard installed yet: the default is all-denied.
        assert!(
            call("read_file", vec![RuntimeValue::String(text_path.clone())]).is_err(),
            "read_file should be blocked when read access is not allowed"
        );
        assert!(
            call("read_file_bytes", vec![RuntimeValue::String(bytes_path.clone())]).is_err(),
            "read_file_bytes should be blocked when read access is not allowed"
        );
        assert!(
            call(
                "collection",
                vec![RuntimeValue::String(
                    collection_dir.path().to_string_lossy().into_owned()
                )],
            )
            .is_err(),
            "collection should be blocked when read access is not allowed"
        );
        assert!(
            call("file_exists", vec![RuntimeValue::String(text_path.clone())]).is_err(),
            "file_exists should be blocked when read access is not allowed"
        );
        assert!(
            call("file_size", vec![RuntimeValue::String(text_path.clone())]).is_err(),
            "file_size should be blocked when read access is not allowed"
        );

        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));
        assert_eq!(
            call("read_file", vec![RuntimeValue::String(text_path.clone())]),
            Ok(RuntimeValue::String("hello".to_string()))
        );
        assert_eq!(
            call("read_file_bytes", vec![RuntimeValue::String(bytes_path.clone())]),
            Ok(RuntimeValue::Bytes(vec![0x89, 0x50, 0x4e, 0x47]))
        );

        let result = call(
            "read_file_bytes",
            vec![RuntimeValue::String("/nonexistent/path/no_such_file.png".into())],
        );
        assert!(result.is_err(), "read_file_bytes should error for a nonexistent file");

        let result = call(
            "read_file",
            vec![RuntimeValue::String("/nonexistent/path/no_such_file.md".into())],
        );
        assert!(result.is_err(), "read_file should error for a nonexistent file");

        assert_eq!(
            call("file_exists", vec![RuntimeValue::String(text_path.clone())]),
            Ok(RuntimeValue::Boolean(true))
        );
        assert_eq!(
            call(
                "file_exists",
                vec![RuntimeValue::String("/nonexistent/path/no_such_file.md".into())]
            ),
            Ok(RuntimeValue::Boolean(false))
        );
        assert!(call("file_exists", vec![RuntimeValue::Number(42.into())]).is_err());

        assert_eq!(
            call("file_size", vec![RuntimeValue::String(text_path.clone())]),
            Ok(RuntimeValue::Number(5.into()))
        );
        assert!(
            call(
                "file_size",
                vec![RuntimeValue::String("/nonexistent/path/no_such_file.md".into())]
            )
            .is_err(),
            "file_size should error for a nonexistent file"
        );
        assert!(call("file_size", vec![RuntimeValue::Number(42.into())]).is_err());

        // Basic collection: YAML/TOML frontmatter, title, content, sorted by path.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            std::fs::write(
                dir.path().join("b.md"),
                "---\ntitle: Hello\ntags:\n  - rust\n---\n\n# Hello\n\nbody text\n",
            )
            .expect("failed to write");
            std::fs::write(dir.path().join("a.md"), "+++\ntitle = \"World\"\n+++\n\n# World\n")
                .expect("failed to write");
            std::fs::write(dir.path().join("c.md"), "# No frontmatter\n\nplain\n").expect("failed to write");
            std::fs::write(dir.path().join("ignore.txt"), "not markdown").expect("failed to write");

            let entries = as_entries(
                call(
                    "collection",
                    vec![RuntimeValue::String(dir.path().to_string_lossy().into_owned())],
                )
                .expect("collection should succeed"),
            );
            assert_eq!(entries.len(), 3);

            // sorted by path: a.md, b.md, c.md
            assert_eq!(get(&entries[0], "title"), RuntimeValue::String("World".into()));
            let mut toml_frontmatter = BTreeMap::new();
            toml_frontmatter.insert(Ident::new("title"), RuntimeValue::String("World".into()));
            assert_eq!(
                get(&entries[0], "frontmatter"),
                RuntimeValue::Dict(Shared::new(toml_frontmatter))
            );

            assert_eq!(get(&entries[1], "title"), RuntimeValue::String("Hello".into()));
            match get(&entries[1], "frontmatter") {
                RuntimeValue::Dict(d) => {
                    assert_eq!(d.get(&Ident::new("title")), Some(&RuntimeValue::String("Hello".into())));
                    assert_eq!(
                        d.get(&Ident::new("tags")),
                        Some(&RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(
                            "rust".into()
                        )])))
                    );
                }
                other => panic!("expected Dict, got {other:?}"),
            }
            match get(&entries[1], "content") {
                RuntimeValue::Array(nodes) => {
                    assert!(nodes.iter().any(|n| match n {
                        RuntimeValue::Markdown(node, _) => node.value().contains("body text"),
                        _ => false,
                    }));
                    assert!(
                        !nodes
                            .iter()
                            .any(|n| matches!(n, RuntimeValue::Markdown(node, _) if node.is_yaml()))
                    );
                }
                other => panic!("expected Array, got {other:?}"),
            }

            assert_eq!(get(&entries[2], "title"), RuntimeValue::String("No frontmatter".into()));
            assert_eq!(get(&entries[2], "frontmatter"), RuntimeValue::NONE);
        }

        // Recurses into subdirectories.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            std::fs::write(dir.path().join("root.md"), "# Root\n").expect("failed to write");

            let sub = dir.path().join("sub");
            std::fs::create_dir(&sub).expect("failed to create subdir");
            std::fs::write(sub.join("nested.md"), "# Nested\n").expect("failed to write");

            let nested_sub = sub.join("deeper");
            std::fs::create_dir(&nested_sub).expect("failed to create nested subdir");
            std::fs::write(nested_sub.join("deepest.md"), "# Deepest\n").expect("failed to write");

            let entries = as_entries(
                call(
                    "collection",
                    vec![RuntimeValue::String(dir.path().to_string_lossy().into_owned())],
                )
                .expect("collection should succeed"),
            );
            assert_eq!(entries.len(), 3);

            let titles: Vec<_> = entries
                .iter()
                .map(|entry| match get(entry, "title") {
                    RuntimeValue::String(s) => s,
                    other => panic!("expected String, got {other:?}"),
                })
                .collect();
            assert!(titles.contains(&"Root".to_string()));
            assert!(titles.contains(&"Nested".to_string()));
            assert!(titles.contains(&"Deepest".to_string()));
        }

        #[cfg(unix)]
        {
            // Follows symlinks to both files and directories.
            {
                let dir = tempfile::tempdir().expect("failed to create temp dir");
                std::fs::write(dir.path().join("real.md"), "# Real\n").expect("failed to write");

                let real_sub = dir.path().join("real_sub");
                std::fs::create_dir(&real_sub).expect("failed to create subdir");
                std::fs::write(real_sub.join("inside.md"), "# Inside\n").expect("failed to write");

                // Symlink to a file, placed directly in the root directory.
                std::os::unix::fs::symlink(dir.path().join("real.md"), dir.path().join("linked.md"))
                    .expect("failed to create file symlink");

                // Symlink to a directory, which should be traversed like a normal directory.
                std::os::unix::fs::symlink(&real_sub, dir.path().join("linked_dir"))
                    .expect("failed to create dir symlink");

                let entries = as_entries(
                    call(
                        "collection",
                        vec![RuntimeValue::String(dir.path().to_string_lossy().into_owned())],
                    )
                    .expect("collection should succeed"),
                );
                let titles: Vec<_> = entries
                    .iter()
                    .map(|entry| match get(entry, "title") {
                        RuntimeValue::String(s) => s,
                        other => panic!("expected String, got {other:?}"),
                    })
                    .collect();

                // real.md, linked.md (-> real.md), real_sub/inside.md, linked_dir/inside.md (-> real_sub/inside.md)
                assert_eq!(entries.len(), 4);
                assert_eq!(titles.iter().filter(|t| *t == "Real").count(), 2);
                assert_eq!(titles.iter().filter(|t| *t == "Inside").count(), 2);
            }

            // Detects and stops at symlink cycles.
            {
                let dir = tempfile::tempdir().expect("failed to create temp dir");
                std::fs::write(dir.path().join("root.md"), "# Root\n").expect("failed to write");

                let sub = dir.path().join("sub");
                std::fs::create_dir(&sub).expect("failed to create subdir");

                // Symlink back to the root directory, creating a cycle.
                std::os::unix::fs::symlink(dir.path(), sub.join("back_to_root")).expect("failed to create dir symlink");

                let entries = as_entries(
                    call(
                        "collection",
                        vec![RuntimeValue::String(dir.path().to_string_lossy().into_owned())],
                    )
                    .expect("collection should succeed despite the symlink cycle"),
                );
                assert_eq!(entries.len(), 1);
            }

            // Skips broken symlinks instead of erroring.
            {
                let dir = tempfile::tempdir().expect("failed to create temp dir");
                std::fs::write(dir.path().join("root.md"), "# Root\n").expect("failed to write");

                std::os::unix::fs::symlink(dir.path().join("does_not_exist.md"), dir.path().join("broken.md"))
                    .expect("failed to create broken symlink");

                let entries = as_entries(
                    call(
                        "collection",
                        vec![RuntimeValue::String(dir.path().to_string_lossy().into_owned())],
                    )
                    .expect("collection should succeed despite the broken symlink"),
                );
                assert_eq!(entries.len(), 1);
            }
        }

        // Skips empty subdirectories.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            std::fs::write(dir.path().join("root.md"), "# Root\n").expect("failed to write");
            std::fs::create_dir(dir.path().join("empty_sub")).expect("failed to create empty subdir");

            let entries = as_entries(
                call(
                    "collection",
                    vec![RuntimeValue::String(dir.path().to_string_lossy().into_owned())],
                )
                .expect("collection should succeed with an empty subdirectory present"),
            );
            assert_eq!(entries.len(), 1);
        }

        // Errors when the path is a file rather than a directory.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            let file_path = dir.path().join("not_a_dir.md");
            std::fs::write(&file_path, "# Root\n").expect("failed to write");

            let result = call(
                "collection",
                vec![RuntimeValue::String(file_path.to_string_lossy().into_owned())],
            );
            assert!(result.is_err());
        }

        // Errors for a nonexistent directory.
        assert!(
            call(
                "collection",
                vec![RuntimeValue::String("/nonexistent/path/no_such_dir".into())],
            )
            .is_err()
        );

        // Errors for an invalid argument type.
        assert!(call("collection", vec![RuntimeValue::Number(42.into())]).is_err());

        // respect_gitignore defaults to false: prior behavior is unchanged.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            std::fs::write(dir.path().join(".gitignore"), "ignored.md\n").expect("failed to write");
            std::fs::write(dir.path().join("ignored.md"), "# Ignored\n").expect("failed to write");
            std::fs::write(dir.path().join("kept.md"), "# Kept\n").expect("failed to write");

            let hidden_dir = dir.path().join(".hidden");
            std::fs::create_dir(&hidden_dir).expect("failed to create hidden dir");
            std::fs::write(hidden_dir.join("secret.md"), "# Secret\n").expect("failed to write");

            let entries = as_entries(
                call(
                    "collection",
                    vec![RuntimeValue::String(dir.path().to_string_lossy().into_owned())],
                )
                .expect("collection should succeed"),
            );
            assert_eq!(
                entries.len(),
                3,
                "ignored.md and .hidden/secret.md should still be collected"
            );
        }

        // respect_gitignore = true: skips dotfiles and .gitignore matches; nested .gitignore wins.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            std::fs::write(dir.path().join(".gitignore"), "*.log.md\nbuild/\n").expect("failed to write");
            std::fs::write(dir.path().join("kept.md"), "# Kept\n").expect("failed to write");
            std::fs::write(dir.path().join("debug.log.md"), "# Debug\n").expect("failed to write");

            let hidden_dir = dir.path().join(".hidden");
            std::fs::create_dir(&hidden_dir).expect("failed to create hidden dir");
            std::fs::write(hidden_dir.join("secret.md"), "# Secret\n").expect("failed to write");

            let build_dir = dir.path().join("build");
            std::fs::create_dir(&build_dir).expect("failed to create build dir");
            std::fs::write(build_dir.join("out.md"), "# Out\n").expect("failed to write");

            // A subdirectory's .gitignore re-allows a file the parent's .gitignore ignores.
            let sub = dir.path().join("sub");
            std::fs::create_dir(&sub).expect("failed to create subdir");
            std::fs::write(sub.join(".gitignore"), "!debug.log.md\n").expect("failed to write");
            std::fs::write(sub.join("debug.log.md"), "# Sub debug\n").expect("failed to write");

            let entries = as_entries(
                call(
                    "collection",
                    vec![
                        RuntimeValue::String(dir.path().to_string_lossy().into_owned()),
                        RuntimeValue::Boolean(true),
                    ],
                )
                .expect("collection should succeed"),
            );
            let titles: Vec<_> = entries
                .iter()
                .map(|entry| match get(entry, "title") {
                    RuntimeValue::String(s) => s,
                    other => panic!("expected String, got {other:?}"),
                })
                .collect();

            assert_eq!(entries.len(), 2, "titles: {titles:?}");
            assert!(titles.contains(&"Kept".to_string()));
            assert!(titles.contains(&"Sub debug".to_string()));
        }

        // Errors for an invalid respect_gitignore argument type.
        assert!(
            call(
                "collection",
                vec![
                    RuntimeValue::String("/nonexistent/path".into()),
                    RuntimeValue::Number(1.into())
                ],
            )
            .is_err()
        );
    }

    #[cfg(feature = "file-io")]
    #[test]
    fn test_write_file_capability_gate_and_success() {
        let tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
        let path = tmp.path().to_string_lossy().to_string();
        assert!(
            call(
                "write_file",
                vec![RuntimeValue::String(path.clone()), RuntimeValue::String("hello".into())]
            )
            .is_err(),
            "write_file should be blocked when write access is not allowed"
        );

        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_write(true)));
        assert_eq!(
            call(
                "write_file",
                vec![RuntimeValue::String(path.clone()), RuntimeValue::String("hello".into())]
            ),
            Ok(RuntimeValue::NONE)
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");

        assert_eq!(
            call(
                "write_file",
                vec![RuntimeValue::String(path.clone()), RuntimeValue::Bytes(vec![1, 2, 3])]
            ),
            Ok(RuntimeValue::NONE)
        );
        assert_eq!(std::fs::read(&path).unwrap(), vec![1, 2, 3]);

        let result = call(
            "write_file",
            vec![
                RuntimeValue::String("/nonexistent/dir/no_such_file.md".into()),
                RuntimeValue::String("hello".into()),
            ],
        );
        assert!(
            result.is_err(),
            "write_file should error when the parent directory doesn't exist"
        );
    }

    #[cfg(feature = "process-io")]
    #[test]
    fn test_system_capability_gate_and_success() {
        assert!(
            call("system", vec![RuntimeValue::String("echo".into())]).is_err(),
            "system should be blocked when run access is not allowed"
        );

        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_run(true)));

        let result = call(
            "system",
            vec![
                RuntimeValue::String("echo".into()),
                RuntimeValue::Array(Shared::new(vec![RuntimeValue::String("hello".into())])),
            ],
        )
        .unwrap();
        assert_eq!(result, RuntimeValue::String("hello\n".to_string()));

        assert!(
            call(
                "system",
                vec![RuntimeValue::String("mq-this-command-should-not-exist".into())]
            )
            .is_err(),
            "system should error for a missing command"
        );

        assert!(
            call(
                "system",
                vec![
                    RuntimeValue::String("echo".into()),
                    RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into())])),
                ],
            )
            .is_err(),
            "system should error when args contains a non-string value"
        );

        assert!(call("system", vec![RuntimeValue::Number(42.into())]).is_err());
    }

    #[cfg(feature = "file-io")]
    fn image_value(url: &str) -> RuntimeValue {
        RuntimeValue::Markdown(
            Box::new(mq_markdown::Node::Image(mq_markdown::Image {
                alt: "alt".to_string(),
                url: url.to_string(),
                title: None,
                position: None,
            })),
            None,
        )
    }

    #[cfg(feature = "file-io")]
    fn image_url(value: &RuntimeValue) -> String {
        match value {
            RuntimeValue::Markdown(node, _) => match &**node {
                mq_markdown::Node::Image(image) => image.url.clone(),
                other => panic!("expected Image node, got {other:?}"),
            },
            other => panic!("expected Markdown value, got {other:?}"),
        }
    }

    #[cfg(feature = "file-io")]
    #[test]
    fn test_embed_images_capability_gate_and_success() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(dir.path().join("img.png"), [0x89, 0x50, 0x4e, 0x47]).expect("failed to write");
        let base_dir = dir.path().to_string_lossy().to_string();

        assert!(
            call(
                "embed_images",
                vec![image_value("img.png"), RuntimeValue::String(base_dir.clone())]
            )
            .is_err(),
            "embed_images should be blocked when read access is not allowed"
        );

        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

        let result = call(
            "embed_images",
            vec![image_value("img.png"), RuntimeValue::String(base_dir.clone())],
        )
        .expect("embed_images should succeed");
        assert_eq!(image_url(&result), "data:image/png;base64,iVBORw==");

        // Default base_dir (".") combined with an absolute path.
        let absolute_path = dir.path().join("img.png").to_string_lossy().to_string();
        let result = call("embed_images", vec![image_value(&absolute_path)]).expect("embed_images should succeed");
        assert_eq!(image_url(&result), "data:image/png;base64,iVBORw==");

        // Already-embedded, remote, and non-image nodes pass through unchanged.
        let already_embedded = "data:image/png;base64,aGVsbG8=";
        assert_eq!(
            call(
                "embed_images",
                vec![image_value(already_embedded), RuntimeValue::String(base_dir.clone())]
            ),
            Ok(image_value(already_embedded))
        );
        assert_eq!(
            call(
                "embed_images",
                vec![
                    image_value("https://example.com/img.png"),
                    RuntimeValue::String(base_dir.clone())
                ]
            ),
            Ok(image_value("https://example.com/img.png"))
        );
        let text_node = RuntimeValue::Markdown(
            Box::new(mq_markdown::Node::Text(mq_markdown::Text {
                value: "hello".to_string(),
                position: None,
            })),
            None,
        );
        assert_eq!(
            call(
                "embed_images",
                vec![text_node.clone(), RuntimeValue::String(base_dir.clone())]
            ),
            Ok(text_node)
        );

        let result = call(
            "embed_images",
            vec![image_value("no_such_file.png"), RuntimeValue::String(base_dir.clone())],
        );
        assert!(result.is_err(), "embed_images should error for a nonexistent file");

        let result = call(
            "embed_images",
            vec![image_value("img.unsupported"), RuntimeValue::String(base_dir)],
        );
        assert!(
            result.is_err(),
            "embed_images should error for an unsupported extension"
        );
    }

    #[cfg(feature = "file-io")]
    #[test]
    fn test_extract_images_capability_gate_and_success() {
        let bytes = [0x89, 0x50, 0x4e, 0x47];
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let data_url = format!("data:image/png;base64,{encoded}");
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let out_dir = dir.path().to_string_lossy().to_string();

        assert!(
            call(
                "extract_images",
                vec![image_value(&data_url), RuntimeValue::String(out_dir.clone())]
            )
            .is_err(),
            "extract_images should be blocked when write access is not allowed"
        );

        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_write(true)));

        let result = call(
            "extract_images",
            vec![image_value(&data_url), RuntimeValue::String(out_dir.clone())],
        )
        .expect("extract_images should succeed");
        let hash = match convert::md5_bytes(&bytes).unwrap() {
            RuntimeValue::String(s) => s,
            _ => unreachable!(),
        };
        let expected_path = dir.path().join(format!("{hash}.png"));
        assert_eq!(image_url(&result), expected_path.to_string_lossy());
        assert_eq!(std::fs::read(&expected_path).unwrap(), bytes);

        // Non-data-URI and non-image nodes pass through unchanged.
        assert_eq!(
            call(
                "extract_images",
                vec![image_value("img.png"), RuntimeValue::String(out_dir.clone())]
            ),
            Ok(image_value("img.png"))
        );

        let result = call(
            "extract_images",
            vec![
                image_value("data:image/png;base64"),
                RuntimeValue::String(out_dir.clone()),
            ],
        );
        assert!(result.is_err(), "extract_images should error on a malformed data URI");

        let result = call(
            "extract_images",
            vec![
                image_value(&format!("data:image/png,{encoded}")),
                RuntimeValue::String(out_dir.clone()),
            ],
        );
        assert!(result.is_err(), "extract_images should error on a non-base64 data URI");

        let result = call(
            "extract_images",
            vec![
                image_value(&format!("data:application/pdf;base64,{encoded}")),
                RuntimeValue::String(out_dir),
            ],
        );
        assert!(
            result.is_err(),
            "extract_images should error on an unsupported MIME type"
        );
    }

    #[cfg(all(feature = "http", feature = "mock-io"))]
    #[test]
    fn test_mock_fetch_seeds_a_response_the_http_builtin_then_reads_back() {
        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));

        assert_eq!(
            call(
                "mock_fetch",
                vec![
                    RuntimeValue::String("https://example.invalid".into()),
                    RuntimeValue::String("body".into()),
                ]
            ),
            Ok(RuntimeValue::NONE)
        );
        assert_eq!(
            call(
                "http",
                vec![
                    RuntimeValue::Symbol(Ident::new("get")),
                    RuntimeValue::String("https://example.invalid".into()),
                ]
            ),
            Ok(RuntimeValue::String("body".into()))
        );
    }

    #[cfg(all(feature = "http", feature = "mock-io"))]
    #[test]
    fn test_http_all_fetches_each_request_in_batch_order() {
        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));
        for (url, body) in [
            ("https://example.invalid/a", "body-a"),
            ("https://example.invalid/b", "body-b"),
            ("https://example.invalid/c", "body-c"),
        ] {
            call(
                "mock_fetch",
                vec![RuntimeValue::String(url.into()), RuntimeValue::String(body.into())],
            )
            .unwrap();
        }

        let requests = RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Dict(Shared::new(std::collections::BTreeMap::from([(
                Ident::new("url"),
                RuntimeValue::String("https://example.invalid/a".into()),
            )]))),
            RuntimeValue::Dict(Shared::new(std::collections::BTreeMap::from([(
                Ident::new("url"),
                RuntimeValue::String("https://example.invalid/b".into()),
            )]))),
            RuntimeValue::Dict(Shared::new(std::collections::BTreeMap::from([
                (Ident::new("method"), RuntimeValue::Symbol(Ident::new("post"))),
                (
                    Ident::new("url"),
                    RuntimeValue::String("https://example.invalid/c".into()),
                ),
                (Ident::new("body"), RuntimeValue::String("payload".into())),
            ]))),
        ]));

        assert_eq!(
            call("http_all", vec![requests]),
            Ok(RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String("body-a".into()),
                RuntimeValue::String("body-b".into()),
                RuntimeValue::String("body-c".into()),
            ])))
        );
    }

    #[cfg(all(feature = "http", feature = "mock-io"))]
    #[test]
    fn test_http_all_rejects_non_dict_request() {
        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));

        let requests = RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(
            "https://example.invalid".into(),
        )]));

        assert!(call("http_all", vec![requests]).is_err());
    }

    #[cfg(all(feature = "http", feature = "mock-io"))]
    #[test]
    fn test_http_all_rejects_request_without_url() {
        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));

        let requests = RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(
            std::collections::BTreeMap::from([(Ident::new("method"), RuntimeValue::Symbol(Ident::new("get")))]),
        ))]));

        assert!(call("http_all", vec![requests]).is_err());
    }

    #[cfg(all(feature = "http", feature = "mock-io"))]
    #[test]
    fn test_mock_fetch_is_refused_by_non_mock_io() {
        assert!(
            call(
                "mock_fetch",
                vec![
                    RuntimeValue::String("https://example.invalid".into()),
                    RuntimeValue::String("body".into()),
                ]
            )
            .is_err(),
            "mock_fetch should be refused when the ambient Io isn't a mock"
        );

        let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_net(true)));
        assert!(
            call(
                "mock_fetch",
                vec![
                    RuntimeValue::String("https://example.invalid".into()),
                    RuntimeValue::String("body".into()),
                ]
            )
            .is_err(),
            "mock_fetch should be refused against a real, network-backed Io"
        );
    }
}
