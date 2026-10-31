pub(super) mod bytes;
mod char_index;
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
mod xml;

use crate::DictMap;
use crate::arena::Arena;
use crate::ast::TokenId;
use crate::ast::constants;
use crate::error::runtime::RuntimeError;
use crate::ident::all_symbols;
#[cfg(feature = "http")]
use crate::io::HttpRequestSpec;
use crate::io::IoError;
#[cfg(feature = "file-io")]
use crate::io::{FileKind, Io};
use crate::number::{self};
use crate::runtime::builtin::convert::Convert;
#[cfg(feature = "file-io")]
use crate::runtime::reader_handle::{HandleKind, ReaderHandle};
use crate::selector::Selector;
use crate::tarn::VmEnv;
use crate::tarn::interpreter::coroutine;
use crate::{Ident, Shared, SharedCell, Token, get_token, parse_markdown_input, parse_mdx_input};
use base64::Engine;
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};
use csv::ReaderBuilder;
use itertools::Itertools;
use rustc_hash::FxHashMap;
#[cfg(feature = "file-io")]
use rustc_hash::FxHashSet;
use similar::{ChangeTag, TextDiff};
use smallvec::SmallVec;
use std::borrow::Cow;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use self::range::{generate_char_range, generate_multi_char_range, generate_numeric_range};
use self::regex::{
    capture_re, extract_urls, is_match_re, match_re, regex_escape, regex_replace_matches, replace_re, scan_re,
    split_re, split_records_re,
};
use super::json::parse_json_runtime_value;
use super::runtime_value::{self, RuntimeValue};
use mq_markdown;

/// Maximum number of elements allowed in a generated range
pub(super) const MAX_RANGE_SIZE: usize = 1_000_000;
/// Maximum number of bytes a single `read_bytes` call may request from a reader handle
#[cfg(any(feature = "file-io", feature = "http"))]
pub(super) const MAX_READ_BYTES: usize = 64 * 1024 * 1024;

/// Converts a user-supplied number into a bounded allocation size.
///
/// Keeping this check at the builtin boundary ensures a query cannot turn a
/// floating-point value into an unbounded `usize` and trigger an allocator
/// abort before the VM has an opportunity to enforce its deadline.
fn bounded_size(value: &number::Number, maximum: usize, parameter: &str) -> Result<usize, Error> {
    let raw = value.value();
    if !raw.is_finite() || !value.is_int() || raw < 0.0 || raw > maximum as f64 {
        return Err(Error::Runtime(format!(
            "{parameter} must be a finite, non-negative integer no greater than {maximum}"
        )));
    }
    Ok(raw as usize)
}

/// Validates an array or string index which may grow its target collection.
///
/// Negative indices have historically been cast to `usize` by these mutators, which saturates
/// them to zero. Preserve that public behavior while still rejecting non-finite, fractional,
/// and excessively large positive values before they can cause an oversized allocation.
fn bounded_growth_index(value: &number::Number, operation: &str) -> Result<usize, Error> {
    let raw = value.value();
    if !raw.is_finite() || !value.is_int() || raw > (MAX_RANGE_SIZE - 1) as f64 {
        return Err(Error::Runtime(format!(
            "{operation} index must be a finite integer no greater than {}",
            MAX_RANGE_SIZE - 1
        )));
    }
    Ok(raw.max(0.0) as usize)
}

/// Converts a user-supplied number into an index without permitting lossy or
/// overflowing conversion to `usize`.
fn checked_index(value: &number::Number, operation: &str) -> Result<usize, Error> {
    let raw = value.value();
    if !raw.is_finite() || !value.is_int() || raw < 0.0 || raw > usize::MAX as f64 {
        return Err(Error::Runtime(format!(
            "{operation} index must be a finite, non-negative integer"
        )));
    }
    Ok(raw as usize)
}

type FunctionName = String;
type ErrorArgs = Vec<RuntimeValue>;
type SharedEnv = VmEnv;
pub type Args = SmallVec<[RuntimeValue; 2]>;

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
#[mq_macros::mq_fn(name = "partial", params = Range(2, u8::MAX))]
fn partial_impl(
    ident: &Ident,
    runtime_value: &RuntimeValue,
    mut args: Args,
    _: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    // `f | partial(a, b)`: when the first argument is not a function, the piped one is applied.
    if !matches!(args.first(), Some(RuntimeValue::Closure(_))) && matches!(runtime_value, RuntimeValue::Closure(_)) {
        args.insert(0, runtime_value.clone());
    }
    if args.len() < 2 {
        return Err(invalid_arity(ident, 2, args.len()));
    }
    let fn_value = args.remove(0);
    let provided = args;

    match fn_value {
        RuntimeValue::Closure(vc) => {
            let total_params = vc.chunks[vc.chunk_index as usize].param_shape.bindings.len();
            let already_bound = vc.bound_args.len();
            if already_bound + provided.len() >= total_params {
                return Err(Error::InvalidNumberOfArguments(
                    ident.to_string(),
                    total_params as u8,
                    (already_bound + provided.len()) as u8 + 1,
                ));
            }
            let mut vc = vc;
            Shared::make_mut(&mut vc).bound_args.extend(provided);
            Ok(RuntimeValue::Closure(vc))
        }
        other => Err(Error::InvalidTypes(ident.to_string(), vec![other])),
    }
}

#[mq_macros::mq_fn(name = "halt", params = Fixed(1))]
fn halt_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(exit_code)] => Err(Error::Halt(exit_code.value() as i32)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "error", params = Fixed(1))]
fn error_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(message)] => Err(Error::UserDefined(message.to_string())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "print", params = Fixed(1))]
fn print_impl(ident: &Ident, current_value: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => {
            io_context::current()
                .write_stdout_line(&a.to_string())
                .map_err(|e| Error::Runtime(e.to_string()))?;
            Ok(current_value.clone())
        }
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "stderr", params = Fixed(1))]
fn stderr_impl(ident: &Ident, current_value: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => {
            io_context::current()
                .write_stderr_line(&a.to_string())
                .map_err(|e| Error::Runtime(e.to_string()))?;
            Ok(current_value.clone())
        }
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "type", params = Fixed(1))]
fn type_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.first() {
        Some(value) => Ok(value.name().to_string().into()),
        None => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "close", params = Fixed(1))]
fn close_impl(ident: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Coroutine(handle)] => {
            if coroutine::close(handle) {
                Ok(RuntimeValue::Coroutine(Shared::clone(handle)))
            } else {
                Err(Error::Runtime(format!("{ident}: cannot close a running coroutine")))
            }
        }
        #[cfg(any(feature = "file-io", feature = "http"))]
        [RuntimeValue::ReaderHandle(handle)] => {
            handle.close();
            Ok(RuntimeValue::ReaderHandle(Shared::clone(handle)))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "status", params = Fixed(1))]
fn status_impl(ident: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Coroutine(handle)] => Ok(RuntimeValue::Symbol(Ident::new(coroutine::status_name(handle)))),
        #[cfg(any(feature = "file-io", feature = "http"))]
        [RuntimeValue::ReaderHandle(handle)] => Ok(RuntimeValue::Symbol(Ident::new(if handle.is_open() {
            "open"
        } else {
            "closed"
        }))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "flatten", params = Fixed(1))]
fn flatten_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(arrays)] => Ok(convert::flatten(Shared::unwrap_or_clone(std::mem::take(arrays))).into()),
        [a] => Ok(std::mem::take(a)),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "convert", params = Fixed(2))]
fn convert_impl(ident: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [input, convert_value] => Convert::try_from(convert_value).map(|convert| convert.convert(input)),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "from_date", params = Fixed(1))]
fn from_date_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(date_str)] => convert::from_date(date_str),
        [RuntimeValue::Markdown(node_value, _)] => convert::from_date(node_value.value().as_str()),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

/// Formats a Unix timestamp (seconds) as a date string using the given strftime format.
#[mq_macros::mq_fn(name = "strftime", params = Fixed(2))]
fn strftime_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(secs), RuntimeValue::String(fmt)] => {
            let secs_val = secs.value() as i64;
            DateTime::from_timestamp(secs_val, 0)
                .map(|dt| RuntimeValue::String(Shared::new(dt.format(fmt.as_str()).to_string())))
                .ok_or_else(|| Error::Runtime(format!("strftime: invalid timestamp: {}", secs_val)))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 3, args.len())),
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
        _ => Err(invalid_arity(ident, 3, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "base64d_bytes", params = Fixed(1))]
fn base64d_bytes_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::base64d_bytes(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| convert::base64d_bytes(md.value().as_str()))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "base64urld_bytes", params = Fixed(1))]
fn base64urld_bytes_impl(
    ident: &Ident,
    _: &RuntimeValue,
    mut args: Args,
    _: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => convert::base64urld_bytes(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| convert::base64urld_bytes(md.value().as_str()))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "uuid", params = None)]
fn uuid_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(uuid::Uuid::new_v4().to_string())))
}

#[mq_macros::mq_fn(name = "uuid_v7", params = None)]
fn uuid_v7_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(uuid::Uuid::now_v7().to_string())))
}

#[mq_macros::mq_fn(name = "uuid_v4", params = None)]
fn uuid_v4_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::String(Shared::new(uuid::Uuid::new_v4().to_string())))
}

/// Generates a pseudo-random `f64` in `[0, 1)`. Not cryptographically secure. If `seed` is
/// given, the result is a deterministic function of it instead of the OS entropy source.
#[mq_macros::mq_fn(name = "rand", params = Range(0, 1))]
fn rand_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [] => Ok(RuntimeValue::Number(random::next_f64(None).into())),
        [RuntimeValue::Number(seed)] if seed.is_int() => Ok(RuntimeValue::Number(
            random::next_f64(Some(seed.to_int() as u64)).into(),
        )),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 0, args.len())),
    }
}

/// Generates a pseudo-random integer uniformly distributed in `[min, max]` (inclusive). If
/// `seed` is given, the result is a deterministic function of it instead of the OS entropy
/// source.
#[mq_macros::mq_fn(name = "rand_int", params = Range(2, 3))]
fn rand_int_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(min), RuntimeValue::Number(max)] if min.is_int() && max.is_int() => {
            let (min_i, max_i) = (min.to_int(), max.to_int());
            random::next_range_i64(min_i, max_i, None)
                .map(|n| RuntimeValue::Number(n.into()))
                .ok_or_else(|| Error::Runtime(format!("rand_int: min ({min_i}) must be <= max ({max_i})")))
        }
        [
            RuntimeValue::Number(min),
            RuntimeValue::Number(max),
            RuntimeValue::Number(seed),
        ] if min.is_int() && max.is_int() && seed.is_int() => {
            let (min_i, max_i) = (min.to_int(), max.to_int());
            random::next_range_i64(min_i, max_i, Some(seed.to_int() as u64))
                .map(|n| RuntimeValue::Number(n.into()))
                .ok_or_else(|| Error::Runtime(format!("rand_int: min ({min_i}) must be <= max ({max_i})")))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

/// Returns a new array containing the same elements as `arr` in a uniformly random order. If
/// `seed` is given, the permutation is a deterministic function of it instead of the OS entropy
/// source.
#[mq_macros::mq_fn(name = "shuffle", params = Range(1, 2))]
fn shuffle_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(arr)] => {
            let mut arr = std::mem::take(arr);
            random::shuffle(runtime_value::array_mut(&mut arr), None);
            Ok(RuntimeValue::Array(arr))
        }
        [RuntimeValue::Array(arr), RuntimeValue::Number(seed)] if seed.is_int() => {
            let mut arr = std::mem::take(arr);
            random::shuffle(runtime_value::array_mut(&mut arr), Some(seed.to_int() as u64));
            Ok(RuntimeValue::Array(arr))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

/// Returns `n` elements sampled from `arr` without replacement, in random order. If `seed` is
/// given, the sample is a deterministic function of it instead of the OS entropy source.
#[mq_macros::mq_fn(name = "sample", params = Range(2, 3))]
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
            Ok(RuntimeValue::Array(Shared::new(random::sample(arr, n, None))))
        }
        [
            RuntimeValue::Array(arr),
            RuntimeValue::Number(n),
            RuntimeValue::Number(seed),
        ] if n.is_int() && n.value() >= 0.0 && seed.is_int() => {
            let n = n.to_int() as usize;
            if n > arr.len() {
                return Err(Error::Runtime(format!(
                    "sample: n ({n}) must not exceed the array length ({})",
                    arr.len()
                )));
            }
            Ok(RuntimeValue::Array(Shared::new(random::sample(
                arr,
                n,
                Some(seed.to_int() as u64),
            ))))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

/// Returns a random string of `len` characters, each independently chosen (with
/// replacement) from `charset`. If `seed` is given, the result is a deterministic function of
/// it instead of the OS entropy source.
#[mq_macros::mq_fn(name = "random_string", params = Range(2, 3))]
fn random_string_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(len), RuntimeValue::String(charset)] if len.is_int() && len.value() >= 0.0 => {
            let len = bounded_size(len, MAX_RANGE_SIZE, "random_string length")?;
            let charset: Vec<char> = charset.chars().collect();
            random::next_string(len, &charset, None)
                .map(|s| RuntimeValue::String(s.into()))
                .ok_or_else(|| Error::Runtime("random_string: charset must not be empty".to_string()))
        }
        [
            RuntimeValue::Number(len),
            RuntimeValue::String(charset),
            RuntimeValue::Number(seed),
        ] if len.is_int() && len.value() >= 0.0 && seed.is_int() => {
            let len = bounded_size(len, MAX_RANGE_SIZE, "random_string length")?;
            let charset: Vec<char> = charset.chars().collect();
            random::next_string(len, &charset, Some(seed.to_int() as u64))
                .map(|s| RuntimeValue::String(s.into()))
                .ok_or_else(|| Error::Runtime("random_string: charset must not be empty".to_string()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "to_hex", params = Fixed(1))]
fn to_hex_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b)] => convert::to_hex(b),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "hexdump", params = Fixed(1))]
fn hexdump_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b)] => convert::hexdump(b),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "utf8", params = Fixed(1))]
fn utf8_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b)] => convert::utf8(b),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "decode", params = Fixed(2))]
fn decode_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b), RuntimeValue::String(label)] => convert::decode(b, label),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "encode", params = Fixed(2))]
fn encode_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(text), RuntimeValue::String(label)] => convert::encode(text, label),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
            Ok(RuntimeValue::Bytes(Shared::new(
                b1.iter().zip(b2.iter()).map(|(a, b)| a ^ b).collect(),
            )))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
            Ok(RuntimeValue::Bytes(Shared::new(
                b1.iter().zip(b2.iter()).map(|(a, b)| a & b).collect(),
            )))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
            Ok(RuntimeValue::Bytes(Shared::new(
                b1.iter().zip(b2.iter()).map(|(a, b)| a | b).collect(),
            )))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "bnot", params = Fixed(1))]
fn bnot_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Bytes(b)] => Ok(RuntimeValue::Bytes(Shared::new(b.iter().map(|x| !x).collect()))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[cfg(feature = "html-to-markdown")]
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "to_html", params = Fixed(1))]
fn to_html_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [a] => convert::to_html(a).map_err(|_| Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "markdown_escape", params = Fixed(2))]
fn markdown_escape_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s), RuntimeValue::String(context)] => {
            convert::MarkdownEscapeContext::try_from(context.as_str())
                .and_then(|context| convert::markdown_escape(s, context))
        }
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(context)] => {
            let context = convert::MarkdownEscapeContext::try_from(context.as_str())?;
            node.markdown_node()
                .map(|md| {
                    convert::markdown_escape(md.value().as_str(), context).and_then(|o| match o {
                        RuntimeValue::String(s) => {
                            let mut updated = node.update_markdown_value(&s);
                            updated.strip_updated_text_position();
                            Ok(updated)
                        }
                        a => Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()])),
                    })
                })
                .unwrap_or_else(|| Ok(RuntimeValue::NONE))
        }
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "to_markdown_string", params = Fixed(1))]
fn to_markdown_string_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    convert::to_markdown_string(args.into_vec())
}

#[mq_macros::mq_fn(name = "to_string", params = Fixed(1))]
fn to_string_impl(ident: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.first() {
        Some(value) => convert::to_string(value),
        None => Err(invalid_arity(ident, 1, args.len())),
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
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Bytes(Shared::new(
            Shared::unwrap_or_clone(std::mem::take(s)).into_bytes(),
        ))),
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
            Ok(RuntimeValue::Bytes(bytes.into()))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "to_text", params = Fixed(1))]
fn to_text_impl(ident: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.first() {
        Some(value) => convert::to_text(value),
        None => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "ends_with", params = Fixed(2))]
fn ends_with_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, env: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(md.value().ends_with(&**s).into()))
            .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.ends_with(&**s2).into()),
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => Ok(b1.ends_with(b2).into()),
        [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
            .last()
            .map_or(Ok(RuntimeValue::FALSE), |o| {
                eval_builtin(
                    o,
                    ident,
                    smallvec::smallvec![RuntimeValue::String(std::mem::take(s))],
                    env,
                )
            })
            .unwrap_or(RuntimeValue::FALSE)),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "starts_with", params = Fixed(2))]
fn starts_with_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, env: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(md.value().starts_with(&**s).into()))
            .unwrap_or_else(|| Ok(RuntimeValue::FALSE)),
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.starts_with(&**s2).into()),
        [RuntimeValue::Bytes(b1), RuntimeValue::Bytes(b2)] => Ok(b1.starts_with(b2).into()),
        [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array
            .first()
            .map_or(Ok(RuntimeValue::FALSE), |o| {
                eval_builtin(
                    o,
                    ident,
                    smallvec::smallvec![RuntimeValue::String(std::mem::take(s))],
                    env,
                )
            })
            .unwrap_or(RuntimeValue::FALSE)),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::FALSE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "_regex_replace_matches", params = Fixed(2))]
fn regex_replace_matches_impl(
    ident: &Ident,
    _: &RuntimeValue,
    mut args: Args,
    _: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s), RuntimeValue::String(pattern)] => regex_replace_matches(s, pattern),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(pattern)] => node
            .markdown_node()
            .map(|md| regex_replace_matches(&md.value(), pattern))
            .unwrap_or_else(|| regex_replace_matches("", pattern)),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "regex_escape", params = Fixed(1))]
fn regex_escape_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(regex_escape(s)),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| match regex_escape(md.value().as_str()) {
                RuntimeValue::String(escaped) => Ok(node.update_markdown_value(&escaped)),
                _ => unreachable!("regex_escape always returns a String"),
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 3, args.len())),
    }
}

#[mq_macros::mq_fn(name = "replace", params = Fixed(3))]
fn replace_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [
            RuntimeValue::String(s1),
            RuntimeValue::String(s2),
            RuntimeValue::String(s3),
        ] => Ok(s1.replace(&**s2, &*s3).into()),
        [
            node @ RuntimeValue::Markdown(_, _),
            RuntimeValue::String(s1),
            RuntimeValue::String(s2),
        ] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().replace(&**s1, &*s2).as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [
            RuntimeValue::Bytes(s1),
            RuntimeValue::Bytes(s2),
            RuntimeValue::Bytes(s3),
        ] => {
            let nlen = s2.len();

            if nlen == 0 {
                return Ok(RuntimeValue::Bytes(std::mem::take(s1)));
            }

            let mut out = Vec::new();
            let mut i = 0;

            while i + nlen <= s1.len() {
                if s1[i..i + nlen] == s2.as_slice()[..] {
                    out.extend_from_slice(s3);
                    i += nlen;
                } else {
                    out.push(s1[i]);
                    i += 1;
                }
            }

            out.extend_from_slice(&s1[i..]);
            Ok(RuntimeValue::Bytes(Shared::new(out)))
        }
        [RuntimeValue::None, RuntimeValue::String(_), RuntimeValue::String(_)] => Ok(RuntimeValue::NONE),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => Err(invalid_arity(ident, 3, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 3, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "trim", params = Fixed(1))]
fn trim_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(s.trim().to_string().into()),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().trim())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::Bytes(b)] => {
            let start = b.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(b.len());
            let end = b
                .iter()
                .rposition(|c| !c.is_ascii_whitespace())
                .map(|i| i + 1)
                .unwrap_or(0);
            Ok(RuntimeValue::Bytes(Shared::new(
                b.get(start..end).unwrap_or(&[]).to_vec(),
            )))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "ltrim", params = Fixed(1))]
fn ltrim_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(s.trim_start().to_string().into()),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().trim_start())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::Bytes(b)] => {
            let start = b.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(b.len());
            Ok(RuntimeValue::Bytes(Shared::new(b[start..].to_vec())))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "rtrim", params = Fixed(1))]
fn rtrim_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(s.trim_end().to_string().into()),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(md.value().trim_end())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::Bytes(b)] => {
            let end = b
                .iter()
                .rposition(|c| !c.is_ascii_whitespace())
                .map(|i| i + 1)
                .unwrap_or(0);
            Ok(RuntimeValue::Bytes(Shared::new(b[..end].to_vec())))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

/// Full Unicode case folding, unlike `downcase`'s simple lowercase (e.g. German `ß` -> `ss`).
#[mq_macros::mq_fn(name = "casefold", params = Fixed(1))]
fn casefold_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(caseless::default_case_fold_str(md.value().as_str()).as_str())))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::String(s)] => Ok(caseless::default_case_fold_str(s).into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

fn normalize_unicode(caller: &str, s: &str, form: &str) -> Result<String, Error> {
    match form.to_ascii_lowercase().as_str() {
        "nfc" => Ok(s.nfc().collect()),
        "nfd" => Ok(s.nfd().collect()),
        "nfkc" => Ok(s.nfkc().collect()),
        "nfkd" => Ok(s.nfkd().collect()),
        _ => Err(Error::Runtime(format!(
            "{caller}: invalid normalization form `{form}`, expected one of \"nfc\", \"nfd\", \"nfkc\", \"nfkd\""
        ))),
    }
}

/// Normalizes to a Unicode form: `"nfc"`, `"nfd"`, `"nfkc"`, or `"nfkd"`.
#[mq_macros::mq_fn(name = "unicode_normalize", params = Fixed(2))]
fn unicode_normalize_impl(
    ident: &Ident,
    _: &RuntimeValue,
    mut args: Args,
    _: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s), RuntimeValue::String(form)] => {
            Ok(normalize_unicode("unicode_normalize", s, form)?.into())
        }
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(form)] => node
            .markdown_node()
            .map(|md| {
                Ok(node
                    .update_markdown_value(normalize_unicode("unicode_normalize", md.value().as_str(), form)?.as_str()))
            })
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "update", params = Fixed(2))]
fn update_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

/// Resolves slice bounds against `len`: negative values count from the end and the result
/// is clamped to `0..=len`. The returned range is empty when `end <= start`.
fn slice_bounds(start: number::Number, end: number::Number, len: usize) -> std::ops::Range<usize> {
    let clamp = |n: number::Number| {
        let n = n.value() as isize;
        if n < 0 {
            (len as isize + n).max(0) as usize
        } else {
            (n as usize).min(len)
        }
    };
    let (start, end) = (clamp(start), clamp(end));
    if end <= start { start..start } else { start..end }
}

/// Slices `s` by char positions with `slice_bounds` semantics.
fn slice_str(s: &Shared<String>, start: number::Number, end: number::Number) -> &str {
    let range = slice_bounds(start, end, char_index::char_len(s));
    &s[char_index::char_to_byte(s, range.start)..char_index::char_to_byte(s, range.end)]
}

/// Returns the char of `s` at a possibly negative `index`, or `None` when out of range.
pub(crate) fn char_at(s: &Shared<String>, index: number::Number) -> Option<char> {
    let i = resolve_index(index, char_index::char_len(s))?;
    s[char_index::char_to_byte(s, i)..].chars().next()
}

#[mq_macros::mq_fn(name = "slice", params = Fixed(3))]
fn slice_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    // Like `get`, a non-integer bound yields none.
    if let [
        RuntimeValue::String(_) | RuntimeValue::Array(_) | RuntimeValue::Markdown(_, _) | RuntimeValue::Bytes(_),
        RuntimeValue::Number(start),
        RuntimeValue::Number(end),
    ] = args.as_slice()
        && !(start.is_int() && end.is_int())
    {
        return Ok(RuntimeValue::NONE);
    }

    match args.as_mut_slice() {
        [
            RuntimeValue::String(s),
            RuntimeValue::Number(start),
            RuntimeValue::Number(end),
        ] => Ok(slice_str(s, *start, *end).into()),
        [
            RuntimeValue::Array(arrays),
            RuntimeValue::Number(start),
            RuntimeValue::Number(end),
        ] => {
            let range = slice_bounds(*start, *end, arrays.len());
            Ok(RuntimeValue::Array(Shared::new(arrays[range].to_vec())))
        }
        [
            node @ RuntimeValue::Markdown(_, _),
            RuntimeValue::Number(start),
            RuntimeValue::Number(end),
        ] => node
            .markdown_node()
            .map(|md| Ok(node.update_markdown_value(slice_str(&Shared::new(md.value().to_string()), *start, *end))))
            .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
        [
            RuntimeValue::Bytes(b),
            RuntimeValue::Number(start),
            RuntimeValue::Number(end),
        ] => {
            let range = slice_bounds(*start, *end, b.len());
            Ok(RuntimeValue::Bytes(Shared::new(b[range].to_vec())))
        }
        [RuntimeValue::None, RuntimeValue::Number(_), RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => Err(invalid_arity(ident, 3, args.len())),
    }
}

#[mq_macros::mq_fn(name = "pow", params = Fixed(2))]
fn pow_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(base), RuntimeValue::Number(exp)] => {
            if base.is_int() && exp.is_int() && exp.value() >= 0.0 {
                match (base.value() as i64).checked_pow(exp.value() as u32) {
                    Some(result) => Ok(RuntimeValue::Number(result.into())),
                    None => Ok(RuntimeValue::Number(base.value().powf(exp.value()).into())),
                }
            } else {
                Ok(RuntimeValue::Number(base.value().powf(exp.value()).into()))
            }
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "ln", params = Fixed(1))]
fn ln_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ln().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "log10", params = Fixed(1))]
fn log10_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().log10().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "sqrt", params = Fixed(1))]
fn sqrt_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().sqrt().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "exp", params = Fixed(1))]
fn exp_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().exp().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "log", params = Fixed(1))]
fn log_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ln().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "sin", params = Fixed(1))]
fn sin_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().sin().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "cos", params = Fixed(1))]
fn cos_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().cos().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "tan", params = Fixed(1))]
fn tan_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().tan().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "asin", params = Fixed(1))]
fn asin_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().asin().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "acos", params = Fixed(1))]
fn acos_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().acos().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "atan", params = Fixed(1))]
fn atan_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().atan().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "sinh", params = Fixed(1))]
fn sinh_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().sinh().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "cosh", params = Fixed(1))]
fn cosh_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().cosh().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "tanh", params = Fixed(1))]
fn tanh_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().tanh().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "log2", params = Fixed(1))]
fn log2_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().log2().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "cbrt", params = Fixed(1))]
fn cbrt_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().cbrt().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "atan2", params = Fixed(2))]
fn atan2_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(a), RuntimeValue::Number(b)] => {
            Ok(RuntimeValue::Number(a.value().atan2(b.value()).into()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "hypot", params = Fixed(2))]
fn hypot_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(a), RuntimeValue::Number(b)] => {
            Ok(RuntimeValue::Number(a.value().hypot(b.value()).into()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

/// Converts a byte offset into `s` to a Unicode scalar value offset, matching `slice` and `len`.
fn char_offset(s: &str, byte_offset: usize) -> usize {
    if s.is_ascii() {
        byte_offset
    } else {
        s[..byte_offset].chars().count()
    }
}

fn char_find(s: &str, needle: &str) -> i64 {
    s.find(needle).map_or(-1, |b| char_offset(s, b) as i64)
}

fn char_rfind(s: &str, needle: &str) -> i64 {
    s.rfind(needle).map_or(-1, |b| char_offset(s, b) as i64)
}

#[mq_macros::mq_fn(name = "index", params = Fixed(2))]
fn index_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(char_find(s1, s2).into())),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(RuntimeValue::Number(char_find(md.value().as_str(), s).into())))
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "contains", params = Fixed(2))]
fn contains_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(s1.contains(s2.as_str()).into()),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => {
            Ok(node.markdown_node().is_some_and(|md| md.value().contains(&**s)).into())
        }
        [RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(needle)] => {
            Ok((!needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle.as_slice())).into())
        }
        [RuntimeValue::Array(array), v] => Ok(array.iter().any(|o| o == v).into()),
        [RuntimeValue::Dict(map), RuntimeValue::String(key)] => {
            Ok(map.get(key.as_str()).is_some_and(|v| !v.is_none()).into())
        }
        [RuntimeValue::Dict(map), RuntimeValue::Symbol(key)] => Ok(map.get(key).is_some_and(|v| !v.is_none()).into()),
        [RuntimeValue::None, _] | [RuntimeValue::Dict(_), _] => Ok(RuntimeValue::FALSE),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "indices", params = Fixed(2))]
fn indices_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    fn string_indices(haystack: &str, needle: &str) -> Vec<RuntimeValue> {
        if needle.is_empty() {
            return Vec::new();
        }

        let ascii = haystack.is_ascii();
        let (mut last_byte, mut last_char) = (0, 0);

        haystack
            .match_indices(needle)
            .map(|(i, _)| {
                let pos = if ascii {
                    i
                } else {
                    last_char += haystack[last_byte..i].chars().count();
                    last_byte = i;
                    last_char
                };
                RuntimeValue::Number((pos as i64).into())
            })
            .collect()
    }

    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
            Ok(RuntimeValue::Array(Shared::new(string_indices(s1, s2))))
        }
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => Ok(RuntimeValue::Array(Shared::new(
            node.markdown_node()
                .map(|md| string_indices(md.value().as_str(), s))
                .unwrap_or_default(),
        ))),
        [RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(needle)] => {
            let nlen = needle.len();
            let mut positions = Vec::new();

            if nlen > 0 {
                let mut i = 0;
                while i + nlen <= haystack.len() {
                    if haystack[i..i + nlen] == needle.as_slice()[..] {
                        positions.push(RuntimeValue::Number((i as i64).into()));
                        i += nlen;
                    } else {
                        i += 1;
                    }
                }
            }

            Ok(RuntimeValue::Array(Shared::new(positions)))
        }
        [RuntimeValue::Array(array), v] => Ok(RuntimeValue::Array(Shared::new(
            array
                .iter()
                .enumerate()
                .filter(|(_, o)| *o == v)
                .map(|(i, _)| RuntimeValue::Number((i as i64).into()))
                .collect(),
        ))),
        [RuntimeValue::None, _] => Ok(RuntimeValue::Array(Shared::new(Vec::new()))),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "len", params = Fixed(1))]
fn len_impl(ident: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a @ (RuntimeValue::Coroutine(_) | RuntimeValue::WeakCoroutine(_))] => {
            Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()]))
        }
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.chars().count().into())),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| Ok(RuntimeValue::Number(md.value().chars().count().into())))
            .unwrap_or_else(|| Ok(RuntimeValue::Number(0.into()))),
        [a] => Ok(RuntimeValue::Number(a.len().into())),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "utf8bytelen", params = Fixed(1))]
fn utf8bytelen_impl(ident: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a @ (RuntimeValue::Coroutine(_) | RuntimeValue::WeakCoroutine(_))] => {
            Err(Error::InvalidTypes(ident.to_string(), vec![a.clone()]))
        }
        [a] => Ok(RuntimeValue::Number(a.len().into())),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

/// Splits into extended grapheme clusters (user-perceived characters), unlike `explode`'s per-codepoint split.
#[mq_macros::mq_fn(name = "graphemes", params = Fixed(1))]
fn graphemes_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(Shared::new(
            s.graphemes(true).map(RuntimeValue::from).collect::<Vec<_>>(),
        ))),
        [node @ RuntimeValue::Markdown(_, _)] => Ok(RuntimeValue::Array(Shared::new(
            node.markdown_node()
                .map(|md| md.value().graphemes(true).map(RuntimeValue::from).collect::<Vec<_>>())
                .unwrap_or_default(),
        ))),
        _ => Ok(RuntimeValue::empty_array()),
    }
}

/// Counts extended grapheme clusters, unlike `len`'s codepoint count.
#[mq_macros::mq_fn(name = "grapheme_len", params = Fixed(1))]
fn grapheme_len_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Number(s.graphemes(true).count().into())),
        [node @ RuntimeValue::Markdown(_, _)] => Ok(RuntimeValue::Number(
            node.markdown_node()
                .map(|md| md.value().graphemes(true).count())
                .unwrap_or(0)
                .into(),
        )),
        _ => Ok(RuntimeValue::Number(0.into())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
                .map(|node| RuntimeValue::Markdown(Shared::new(node), None))
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "rindex", params = Fixed(2))]
fn rindex_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => Ok(RuntimeValue::Number(char_rfind(s1, s2).into())),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| Ok(RuntimeValue::Number(char_rfind(md.value().as_str(), s).into())))
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
            let index = checked_index(n, "del")?;
            let mut array = std::mem::take(array);
            let values = runtime_value::array_mut(&mut array);
            if index >= values.len() {
                return Err(Error::Runtime(format!(
                    "del index {index} is out of bounds for array of length {}",
                    values.len()
                )));
            }
            values.remove(index);
            Ok(RuntimeValue::Array(array))
        }
        [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
            let index = checked_index(n, "del")?;
            let mut s = std::mem::take(s).chars().collect::<Vec<_>>();
            if index >= s.len() {
                return Err(Error::Runtime(format!(
                    "del index {index} is out of bounds for string of length {}",
                    s.len()
                )));
            }
            s.remove(index);
            Ok(s.into_iter().collect::<String>().into())
        }
        [RuntimeValue::None, RuntimeValue::Number(_)] => Ok(RuntimeValue::NONE),
        [RuntimeValue::Dict(dict), RuntimeValue::String(key)] => {
            let mut dict = std::mem::take(dict);
            // A key that was never interned cannot be in the dict, so skip the copy-on-write.
            if let Some(key) = Ident::lookup(key) {
                runtime_value::dict_mut(&mut dict).remove(&key);
            }
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "join", params = Fixed(2))]
fn join_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array), RuntimeValue::Bytes(sep)]
            if !array.is_empty() && array.iter().all(|v| matches!(v, RuntimeValue::Bytes(_))) =>
        {
            let mut out = Vec::new();
            for (i, v) in array.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(sep);
                }
                if let RuntimeValue::Bytes(b) = v {
                    out.extend_from_slice(b);
                }
            }
            Ok(RuntimeValue::Bytes(Shared::new(out)))
        }
        [RuntimeValue::Array(array), RuntimeValue::String(s)] => Ok(array.iter().join(s).into()),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
            runtime_value::bytes_mut(&mut v).reverse();
            Ok(RuntimeValue::Bytes(v))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "sort", params = Fixed(1))]
fn sort_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => {
            let mut vec = std::mem::take(array);
            runtime_value::array_mut(&mut vec).sort_by(|a, b| a.sort_cmp(b));

            let vec = Shared::unwrap_or_clone(vec)
                .into_iter()
                .map(|v| match v {
                    RuntimeValue::Markdown(mut node, s) => {
                        runtime_value::markdown_mut(&mut node).set_position(None);
                        RuntimeValue::Markdown(node, s)
                    }
                    _ => v,
                })
                .collect();
            Ok(RuntimeValue::Array(Shared::new(vec)))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "_sort_by_impl", params = Fixed(1))]
fn _sort_by_impl_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => {
            let mut vec = std::mem::take(array);
            runtime_value::array_mut(&mut vec).sort_by(|a, b| match (a, b) {
                (RuntimeValue::Array(a1), RuntimeValue::Array(a2)) => a1.first().unwrap().sort_cmp(a2.first().unwrap()),
                _ => unreachable!("_sort_by_impl should only be called with an array of arrays"),
            });
            let vec = Shared::unwrap_or_clone(vec)
                .into_iter()
                .map(|v| match v {
                    RuntimeValue::Array(mut arr) if arr.len() >= 2 => {
                        if let RuntimeValue::Markdown(node, s) = &arr[1] {
                            let mut new_node = node.clone();
                            runtime_value::markdown_mut(&mut new_node).set_position(None);

                            runtime_value::array_mut(&mut arr)[1] = RuntimeValue::Markdown(new_node, *s);
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "compact", params = Fixed(1))]
fn compact_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Array(array)] => Ok(RuntimeValue::Array(Shared::new(
            Shared::unwrap_or_clone(std::mem::take(array))
                .into_iter()
                .filter(|v| !v.is_none())
                .collect::<Vec<_>>(),
        ))),
        [a] => Ok(std::mem::take(a)),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        [RuntimeValue::Bytes(haystack), RuntimeValue::Bytes(needle)] => {
            let nlen = needle.len();

            if nlen == 0 {
                return Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Bytes(
                    std::mem::take(haystack),
                )])));
            }

            let mut result = Vec::new();
            let mut start = 0;
            let mut i = 0;

            while i + nlen <= haystack.len() {
                if haystack[i..i + nlen] == needle.as_slice()[..] {
                    result.push(RuntimeValue::Bytes(Shared::new(haystack[start..i].to_vec())));
                    i += nlen;
                    start = i;
                } else {
                    i += 1;
                }
            }

            result.push(RuntimeValue::Bytes(Shared::new(haystack[start..].to_vec())));
            Ok(RuntimeValue::Array(Shared::new(result)))
        }
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "split_records", params = Fixed(2))]
fn split_records_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => split_records_re(s1, s2),
        [node @ RuntimeValue::Markdown(_, _), RuntimeValue::String(s)] => node
            .markdown_node()
            .map(|md| split_records_re(md.value().as_str(), s))
            .unwrap_or_else(|| Ok(RuntimeValue::empty_array())),
        [RuntimeValue::None, RuntimeValue::String(_)] => Ok(RuntimeValue::empty_array()),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "extract_urls", params = Fixed(1))]
fn extract_urls_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => extract_urls(s),
        [node @ RuntimeValue::Markdown(_, _)] => node
            .markdown_node()
            .map(|md| extract_urls(md.value().as_str()))
            .unwrap_or_else(|| Ok(RuntimeValue::empty_array())),
        [RuntimeValue::None] => Ok(RuntimeValue::empty_array()),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "ceil", params = Fixed(1))]
fn ceil_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().ceil().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "floor", params = Fixed(1))]
fn floor_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().floor().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "round", params = Fixed(1))]
fn round_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().round().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "trunc", params = Fixed(1))]
fn trunc_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().trunc().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "abs", params = Fixed(1))]
fn abs_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(n.value().abs().into())),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

pub(crate) fn add_in_place(target: &mut RuntimeValue, rhs: &RuntimeValue) -> Result<bool, Error> {
    match (target, rhs) {
        (RuntimeValue::String(s), RuntimeValue::String(suffix)) => runtime_value::string_mut(s).push_str(suffix),
        (RuntimeValue::String(s), RuntimeValue::Number(n)) => runtime_value::string_mut(s).push_str(&n.to_string()),
        (RuntimeValue::Array(array), RuntimeValue::Array(other)) => {
            let total_size = array.len().saturating_add(other.len());
            if total_size > MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "array concatenation size {total_size} exceeds maximum allowed size of {MAX_RANGE_SIZE}"
                )));
            }
            runtime_value::array_mut(array).extend_from_slice(other);
        }
        (RuntimeValue::Array(array), elem) => {
            let total_size = array.len().saturating_add(1);
            if total_size > MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "array size {total_size} exceeds maximum allowed size of {MAX_RANGE_SIZE}"
                )));
            }
            runtime_value::array_mut(array).push(elem.clone());
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[mq_macros::mq_fn(name = "add", params = Fixed(2))]
fn add_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s1), RuntimeValue::String(s2)] => {
            runtime_value::string_mut(s1).push_str(s2);
            Ok(std::mem::take(s1).into())
        }
        [RuntimeValue::String(s), RuntimeValue::Number(n)] => {
            runtime_value::string_mut(s).push_str(n.to_string().as_str());
            Ok(std::mem::take(s).into())
        }
        [RuntimeValue::Number(n), RuntimeValue::String(s)] => Ok(format!("{}{}", n, s).into()),
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
            runtime_value::bytes_mut(&mut result).extend_from_slice(b2);
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "sub", params = Fixed(2))]
fn sub_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 - *n2).into()),
        [a, b] => match (convert::to_number(a)?, convert::to_number(b)?) {
            (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 - n2).into()),
            _ => Err(Error::InvalidTypes(
                ident.to_string(),
                vec![std::mem::take(a), std::mem::take(b)],
            )),
        },
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "div", params = Fixed(2))]
fn div_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
                ident.to_string(),
                vec![std::mem::take(a), std::mem::take(b)],
            )),
        },
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "mul", params = Fixed(2))]
fn mul_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 * *n2).into()),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "mod", params = Fixed(2))]
fn mod_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n1), RuntimeValue::Number(n2)] => Ok((*n1 % *n2).into()),
        [a, b] => match (convert::to_number(a)?, convert::to_number(b)?) {
            (RuntimeValue::Number(n1), RuntimeValue::Number(n2)) => Ok((n1 % n2).into()),
            _ => Err(Error::InvalidTypes(
                ident.to_string(),
                vec![std::mem::take(a), std::mem::take(b)],
            )),
        },
        _ => Err(invalid_arity(ident, 2, args.len())),
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
fn not_impl(ident: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] => Ok((!a.is_truthy()).into()),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "attr", params = Fixed(2))]
fn attr_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::String(attr)] => {
            Ok(node.attr(attr).map(Into::into).unwrap_or(RuntimeValue::NONE))
        }
        [RuntimeValue::Array(nodes), RuntimeValue::String(attr)] => Ok(nodes
            .iter()
            .flat_map(|node| match node {
                RuntimeValue::Markdown(node, _) => {
                    let value = node.attr(attr).map(Into::into).unwrap_or(RuntimeValue::NONE);

                    match value {
                        RuntimeValue::Array(arr) => Shared::unwrap_or_clone(arr),
                        RuntimeValue::None => Vec::new(),
                        v => vec![v],
                    }
                }
                a => vec![a.clone()],
            })
            .collect::<Vec<_>>()
            .into()),
        [a, ..] => Ok(std::mem::take(a)),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "set_attr", params = Fixed(3))]
fn set_attr_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
                            RuntimeValue::String(Shared::new(attr.to_string())),
                            std::mem::take(value),
                        ],
                    ));
                }
            };
            runtime_value::markdown_mut(&mut new_node).set_attr(attr, value);
            Ok(RuntimeValue::Markdown(new_node, selector.take()))
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => Err(invalid_arity(ident, 3, args.len())),
    }
}

#[mq_macros::mq_fn(name = "set_children", params = Fixed(2))]
fn set_children_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, selector), RuntimeValue::Array(children)] => {
            let mut new_node = std::mem::take(node);
            let children = children
                .iter()
                .map(|child| match child {
                    RuntimeValue::Markdown(node, _) => (**node).clone(),
                    value => value.to_string().into(),
                })
                .collect();
            runtime_value::markdown_mut(&mut new_node).set_children(children);
            Ok(RuntimeValue::Markdown(new_node, selector.take()))
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "to_code", params = Fixed(2))]
fn to_code_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a, RuntimeValue::String(lang)] if !a.is_none() => Ok(mq_markdown::Node::Code(mq_markdown::Code {
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
                depth: mq_markdown::HeadingDepth::saturating((*depth).value() as i64),
                values: node.node_values(),
                position: None,
            })
            .into())
        }
        [a, RuntimeValue::Number(depth)] if !a.is_none() => Ok(mq_markdown::Node::Heading(mq_markdown::Heading {
            depth: mq_markdown::HeadingDepth::saturating((*depth).value() as i64),
            values: vec![a.to_string().into()],
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_hr", params = Fixed(0))]
fn to_hr_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule {
        marker: None,
        position: None,
    })
    .into())
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
                Some(mq_markdown::Title::new((&(**title)).into()))
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
        [a] if !a.is_none() => Ok(mq_markdown::Node::Math(mq_markdown::Math {
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
        [a] if !a.is_none() => Ok(mq_markdown::Node::MathInline(mq_markdown::MathInline {
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

/// Returns a heading's depth (1-6), or 0 for any other node, without allocating a name string.
#[mq_macros::mq_fn(name = "_md_heading_level", params = Fixed(1))]
fn md_heading_level_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    let level = match args.as_slice() {
        [RuntimeValue::Markdown(node, _)] => match &**node {
            mq_markdown::Node::Heading(mq_markdown::Heading { depth, .. }) => depth.get(),
            _ => 0,
        },
        _ => 0,
    };
    Ok(RuntimeValue::Number(number::Number::from(level)))
}

#[mq_macros::mq_fn(name = "set_list_ordered", params = Fixed(2))]
fn set_list_ordered_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::Boolean(ordered)]
            if matches!(**node, mq_markdown::Node::List(_)) =>
        {
            let ordered = *ordered;
            if let mq_markdown::Node::List(list) = runtime_value::markdown_mut(node) {
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
            fold: None,
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
                fold: None,
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

#[mq_macros::mq_fn(name = "to_footnote", params = Fixed(2))]
fn to_footnote_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::Markdown(node, _), RuntimeValue::String(ident)] => {
            Ok(mq_markdown::Node::Footnote(mq_markdown::Footnote {
                ident: ident.to_string(),
                values: node.node_values(),
                position: None,
            })
            .into())
        }
        [a, RuntimeValue::String(ident)] if !a.is_none() => Ok(mq_markdown::Node::Footnote(mq_markdown::Footnote {
            ident: ident.to_string(),
            values: vec![a.to_string().into()],
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_footnote_ref", params = Fixed(1))]
fn to_footnote_ref_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [RuntimeValue::String(ident)] => Ok(mq_markdown::Node::FootnoteRef(mq_markdown::FootnoteRef {
            ident: ident.to_string(),
            label: Some(ident.to_string()),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_definition", params = Fixed(3))]
fn to_definition_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [
            RuntimeValue::String(url),
            RuntimeValue::String(ident),
            RuntimeValue::String(title),
        ] => Ok(mq_markdown::Node::Definition(mq_markdown::Definition {
            url: mq_markdown::Url::new(url.to_string()),
            title: if title.is_empty() {
                None
            } else {
                Some(mq_markdown::Title::new(title.to_string()))
            },
            ident: ident.to_string(),
            label: Some(ident.to_string()),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_md_html", params = Fixed(1))]
fn to_md_html_impl(_: &Ident, _: &RuntimeValue, args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_slice() {
        [a] if !a.is_none() => Ok(mq_markdown::Node::Html(mq_markdown::Html {
            value: a.to_string(),
            position: None,
        })
        .into()),
        _ => Ok(RuntimeValue::NONE),
    }
}

#[mq_macros::mq_fn(name = "to_break", params = Fixed(0))]
fn to_break_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(mq_markdown::Node::Break(mq_markdown::Break { position: None }).into())
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
                marker: None,
                values: node.node_values(),
                index: 0,
                ordered: false,
                level: level.value() as u8,
                checked: None,
                start: None,
                spread: false,
                position: None,
            })
            .into())
        }
        [a, RuntimeValue::Number(level)] if !a.is_none() => Ok(mq_markdown::Node::List(mq_markdown::List {
            marker: None,
            values: vec![a.to_string().into()],
            index: 0,
            ordered: false,
            level: level.value() as u8,
            checked: None,
            start: None,
            spread: false,
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
        Shared::new(mq_markdown::Node::TableRow(mq_markdown::TableRow {
            values,
            position: None,
        })),
        None,
    ))
}

#[mq_macros::mq_fn(name = "to_md_table_cell", params = Fixed(3))]
fn to_md_table_cell_impl(
    ident: &Ident,
    _: &RuntimeValue,
    mut args: Args,
    _: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [value, RuntimeValue::Number(row), RuntimeValue::Number(column)] => Ok(RuntimeValue::Markdown(
            Shared::new(mq_markdown::Node::TableCell(mq_markdown::TableCell {
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
        _ => Err(invalid_arity(ident, 3, args.len())),
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
fn get_title_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Markdown(node, _)]
            if matches!(**node, mq_markdown::Node::Definition(_) | mq_markdown::Node::Link(_)) =>
        {
            match runtime_value::markdown_mut(node) {
                mq_markdown::Node::Definition(mq_markdown::Definition { title, .. })
                | mq_markdown::Node::Link(mq_markdown::Link { title, .. }) => std::mem::take(title)
                    .map(|t| Ok(RuntimeValue::String(Shared::new(t.to_value()))))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE)),
                _ => unreachable!(),
            }
        }
        [RuntimeValue::Markdown(node, _)] if matches!(**node, mq_markdown::Node::Image(_)) => {
            if let mq_markdown::Node::Image(mq_markdown::Image { title, .. }) = runtime_value::markdown_mut(node) {
                std::mem::take(title)
                    .map(|t| Ok(RuntimeValue::String(t.into())))
                    .unwrap_or_else(|| Ok(RuntimeValue::NONE))
            } else {
                unreachable!()
            }
        }
        [_] => Ok(RuntimeValue::NONE),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
            if let mq_markdown::Node::List(list) = runtime_value::markdown_mut(node) {
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
            match runtime_value::markdown_mut(node) {
                mq_markdown::Node::Definition(def) => {
                    return Ok(mq_markdown::Node::Definition(mq_markdown::Definition {
                        label: Some(s.to_owned().to_string()),
                        ..std::mem::take(def)
                    })
                    .into());
                }
                mq_markdown::Node::ImageRef(image_ref) => {
                    return Ok(mq_markdown::Node::ImageRef(mq_markdown::ImageRef {
                        label: if **s == image_ref.ident {
                            None
                        } else {
                            Some(s.to_owned().to_string())
                        },
                        ..std::mem::take(image_ref)
                    })
                    .into());
                }
                mq_markdown::Node::LinkRef(link_ref) => {
                    return Ok(mq_markdown::Node::LinkRef(mq_markdown::LinkRef {
                        label: if **s == link_ref.ident {
                            None
                        } else {
                            Some(s.to_owned().to_string())
                        },
                        ..std::mem::take(link_ref)
                    })
                    .into());
                }
                mq_markdown::Node::Footnote(footnote) => {
                    return Ok(mq_markdown::Node::Footnote(mq_markdown::Footnote {
                        ident: s.to_owned().to_string(),
                        ..std::mem::take(footnote)
                    })
                    .into());
                }
                mq_markdown::Node::FootnoteRef(footnote_ref) => {
                    return Ok(mq_markdown::Node::FootnoteRef(mq_markdown::FootnoteRef {
                        label: Some(s.to_owned().to_string()),
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
            if let mq_markdown::Node::Code(code) = runtime_value::markdown_mut(node) {
                let lang = std::mem::take(lang);
                let mut new_code = std::mem::take(code);
                new_code.lang = if lang.is_empty() { None } else { Some(lang.to_string()) };
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
        let mut dict = DictMap::default();
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

/// Resolves a possibly negative index against `len`. Returns `None` when it is not an integer
/// or falls outside `0..len`.
pub(crate) fn resolve_index(index: number::Number, len: usize) -> Option<usize> {
    if !index.is_int() {
        return None;
    }
    let idx = index.value() as isize;
    let resolved = if idx < 0 { len as isize + idx } else { idx };
    usize::try_from(resolved).ok()
}

#[mq_macros::mq_fn(name = "get", params = Fixed(2))]
fn get_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        // Read-only: avoid `dict_mut`/`array_mut`'s `make_mut` deep clone of a shared map.
        [RuntimeValue::Dict(map), RuntimeValue::String(key)] => {
            Ok(map.get(key.as_str()).cloned().unwrap_or(RuntimeValue::NONE))
        }
        [RuntimeValue::Dict(map), RuntimeValue::Symbol(key)] => Ok(map.get(key).cloned().unwrap_or(RuntimeValue::NONE)),
        [RuntimeValue::Array(array), RuntimeValue::Number(index)] => Ok(resolve_index(*index, array.len())
            .and_then(|i| array.get(i))
            .cloned()
            .unwrap_or(RuntimeValue::NONE)),
        [RuntimeValue::String(s), RuntimeValue::Number(n)] => match char_at(s, *n) {
            Some(o) => Ok(o.to_string().into()),
            None => Ok(RuntimeValue::NONE),
        },
        [RuntimeValue::Markdown(node, _), RuntimeValue::Number(i)] => {
            let len = node.value().chars().count();
            match resolve_index(*i, len).and_then(runtime_value::Selector::index) {
                Some(sel) => Ok(RuntimeValue::Markdown(std::mem::take(node), Some(sel))),
                None => Ok(RuntimeValue::NONE),
            }
        }
        [RuntimeValue::None, _] | [_, RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a, b] => Err(Error::Runtime(format!(
            "cannot index {} with {} (arrays, strings and markdown take a number, dicts take a string or symbol)",
            a.name(),
            b.name()
        ))),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

/// Reports whether a dict key or array index is present, unlike `get` whose `None` result
/// also means "no key". Native replacement for the old pure-mq `has`; same semantics
/// (non-dict/array subjects, or an out-of-range/negative array index, are just `false`).
#[mq_macros::mq_fn(name = "has", params = Fixed(2))]
fn has_impl(_: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Dict(map), RuntimeValue::String(key)] => Ok(map.contains_key(key.as_str()).into()),
        [RuntimeValue::Dict(map), RuntimeValue::Symbol(key)] => Ok(map.contains_key(key).into()),
        [RuntimeValue::Array(array), RuntimeValue::Number(index)] => {
            let idx = index.value();
            Ok((index.is_int() && idx >= 0.0 && (idx as usize) < array.len()).into())
        }
        _ => Ok(false.into()),
    }
}

/// Applies `set(target, key, value)` to `target` in place.
///
/// Returns `Ok(false)`, leaving `target` and `value` untouched, when the operand types are not
/// supported. Every error is raised before `target` is modified.
pub(crate) fn set_in_place(
    target: &mut RuntimeValue,
    key: &RuntimeValue,
    value: &mut RuntimeValue,
) -> Result<bool, Error> {
    match (target, key) {
        (RuntimeValue::Dict(map), RuntimeValue::String(key)) => {
            runtime_value::dict_mut(map).insert(Ident::new(key), std::mem::take(value));
        }
        (RuntimeValue::Dict(map), RuntimeValue::Symbol(key)) => {
            runtime_value::dict_mut(map).insert(*key, std::mem::take(value));
        }
        (RuntimeValue::Array(array), RuntimeValue::Number(index)) => {
            let index = bounded_growth_index(index, "set")?;
            let array = runtime_value::array_mut(array);
            if index >= array.len() {
                array.resize(index + 1, RuntimeValue::NONE);
            }
            array[index] = std::mem::take(value);
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[mq_macros::mq_fn(name = "set", params = Fixed(3))]
fn set_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [target, key, value] => {
            if set_in_place(target, key, value)? {
                Ok(std::mem::take(target))
            } else {
                Err(Error::InvalidTypes(
                    ident.to_string(),
                    vec![std::mem::take(target), key.clone(), std::mem::take(value)],
                ))
            }
        }
        _ => Err(invalid_arity(ident, 3, args.len())),
    }
}

#[mq_macros::mq_fn(name = "keys", params = Fixed(1))]
fn keys_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Dict(map)] => {
            let keys = map
                .keys()
                .map(|k| RuntimeValue::String(Shared::new(k.to_string())))
                .collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(Shared::new(keys)))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "entries", params = Fixed(1))]
fn entries_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Dict(map)] => {
            let entries = map
                .iter()
                .map(|(k, v)| {
                    RuntimeValue::Array(Shared::new(vec![
                        RuntimeValue::String(Shared::new(k.to_string())),
                        v.to_owned(),
                    ]))
                })
                .collect::<Vec<RuntimeValue>>();
            Ok(RuntimeValue::Array(Shared::new(entries)))
        }
        [RuntimeValue::None] => Ok(RuntimeValue::NONE),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "insert", params = Fixed(3))]
fn insert_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        // Insert into array at index
        [RuntimeValue::Array(array), RuntimeValue::Number(index), value] => {
            let mut new_array = std::mem::take(array);
            let idx = bounded_growth_index(index, "insert")?;
            let array_mut = runtime_value::array_mut(&mut new_array);
            if array_mut.len() >= MAX_RANGE_SIZE {
                return Err(Error::Runtime(format!(
                    "insert would exceed maximum array size of {MAX_RANGE_SIZE}"
                )));
            }
            if idx > array_mut.len() {
                array_mut.resize(idx, RuntimeValue::NONE);
            }
            array_mut.insert(idx, std::mem::take(value));
            Ok(RuntimeValue::Array(new_array))
        }
        // Insert into string at index
        [RuntimeValue::String(s), RuntimeValue::Number(index), value] => {
            let mut chars: Vec<char> = s.chars().collect();
            let idx = bounded_growth_index(index, "insert")?;
            let insert_str = value.to_string();
            if idx > chars.len() {
                chars.resize(idx, ' ');
            }
            for (i, c) in insert_str.chars().enumerate() {
                chars.insert(idx + i, c);
            }
            let result: String = chars.into_iter().collect();
            Ok(RuntimeValue::String(result.into()))
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
        _ => Err(invalid_arity(ident, 3, args.len())),
    }
}

#[mq_macros::mq_fn(name = "negate", params = Fixed(1))]
fn negate_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Number(-(*n))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "intern", params = Fixed(1))]
fn intern_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(Shared::new(Ident::new(s).to_string()))),
        [a] => Ok(RuntimeValue::String(Shared::new(
            Ident::new(&a.to_string()).to_string(),
        ))),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "nan", params = None)]
fn nan_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Number(number::NAN))
}

#[mq_macros::mq_fn(name = "is_nan", params = Fixed(1))]
fn is_nan_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(n)] => Ok(RuntimeValue::Boolean(n.is_nan())),
        [_] => Ok(RuntimeValue::FALSE),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "infinite", params = None)]
fn infinite_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    Ok(RuntimeValue::Number(number::INFINITE))
}

#[mq_macros::mq_fn(name = "coalesce", params = Fixed(2))]
fn coalesce_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [a, b] => {
            if a.is_none() {
                Ok(std::mem::take(b))
            } else {
                Ok(std::mem::take(a))
            }
        }
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "input", params = None)]
fn input_impl(_: &Ident, _: &RuntimeValue, _: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    let line = io_context::current()
        .read_stdin_line()
        .map_err(|e| Error::Runtime(e.to_string()))?;
    Ok(RuntimeValue::String(line.into()))
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "to_mdx", params = Fixed(1))]
fn to_mdx_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::Array(Shared::new(
            parse_mdx_input(s).map_err(|e| Error::Runtime(format!("Failed to parse mdx: {}", e)))?,
        ))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
                let map: DictMap = headers
                    .iter()
                    .enumerate()
                    .map(|(i, k)| {
                        (
                            Ident::new(k),
                            RuntimeValue::String(Shared::new(record.get(i).unwrap_or("").to_string())),
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
                let arr: Vec<RuntimeValue> = record
                    .iter()
                    .map(|v| RuntimeValue::String(Shared::new(v.to_string())))
                    .collect();
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "_json_parse", params = Fixed(1))]
fn _json_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            parse_json_runtime_value(s).map_err(|e| Error::Runtime(format!("Failed to parse JSON: {e}")))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "_toon_stringify", params = Fixed(1))]
fn _toon_stringify_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [value] => {
            let json_value = std::mem::take(value).to_json_value();
            let toon_str = toon_format::encode_default(&json_value)
                .map_err(|e| Error::Runtime(format!("Failed to encode TOON: {}", e)))?;
            Ok(RuntimeValue::String(toon_str.into()))
        }
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

/// Converts via `toml::Value` rather than `serde_json::Value`, which would turn
/// `inf`/`nan` into null and leak the private datetime wrapper. Datetimes become strings.
fn toml_value_to_runtime_value(value: toml::Value) -> RuntimeValue {
    match value {
        toml::Value::String(s) => RuntimeValue::String(Shared::new(s)),
        toml::Value::Integer(i) => RuntimeValue::Number((i as f64).into()),
        toml::Value::Float(f) => RuntimeValue::Number(f.into()),
        toml::Value::Boolean(b) => RuntimeValue::Boolean(b),
        toml::Value::Datetime(d) => RuntimeValue::String(Shared::new(d.to_string())),
        toml::Value::Array(arr) => {
            RuntimeValue::Array(Shared::new(arr.into_iter().map(toml_value_to_runtime_value).collect()))
        }
        toml::Value::Table(table) => {
            let mut map = DictMap::default();
            for (k, v) in table {
                map.insert(Ident::new(&k), toml_value_to_runtime_value(v));
            }
            RuntimeValue::Dict(Shared::new(map))
        }
    }
}

#[mq_macros::mq_fn(name = "_toml_parse", params = Fixed(1))]
fn _toml_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => {
            let value: toml::Value =
                toml::from_str(s).map_err(|e| Error::Runtime(format!("Failed to parse TOML: {}", e)))?;
            Ok(toml_value_to_runtime_value(value))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "_cbor_stringify", params = Fixed(1))]
fn _cbor_stringify_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [value] => {
            let cbor_value = std::mem::take(value).to_cbor_value();
            let mut buf = Vec::new();
            ciborium::into_writer(&cbor_value, &mut buf)
                .map_err(|e| Error::Runtime(format!("Failed to serialize CBOR: {}", e)))?;
            Ok(RuntimeValue::Bytes(buf.into()))
        }
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "_xml_parse", params = Fixed(1))]
fn _xml_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(xml_str)] => xml::parse_xml(xml_str),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[cfg(feature = "css-selector")]
#[mq_macros::mq_fn(name = "_html_parse", params = Fixed(1))]
fn _html_parse_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(html_str)] => Ok(css::parse_html(html_str)),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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

/// `env()` returns all allowed env vars as a dict; `env(name)` returns one, or `None` if unset.
#[mq_macros::mq_fn(name = "env", params = Range(0, 1))]
fn env_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [] => {
            let mut dict = DictMap::default();
            for (name, value) in io_context::current()
                .env_vars()
                .map_err(|e| Error::Runtime(format!("Failed to read environment variables: {}", e)))?
            {
                dict.insert(Ident::new(&name), RuntimeValue::String(value.into()));
            }
            Ok(dict.into())
        }
        [RuntimeValue::String(name)] => match io_context::current().env_var(name) {
            Ok(value) => Ok(RuntimeValue::String(value.into())),
            Err(IoError::NotFound(_)) => Ok(RuntimeValue::NONE),
            Err(e) => Err(Error::Runtime(format!("Failed to read env var {}: {}", name, e))),
        },
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 0, args.len())),
    }
}

#[mq_macros::mq_fn(name = "shift_left", params = Fixed(2))]
fn shift_left_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::Number(v), RuntimeValue::Number(n)] => v
            .to_int()
            .checked_shl(n.value() as u32)
            .map(|result| RuntimeValue::Number(result.into()))
            .ok_or_else(|| Error::Runtime("Shift amount is too large".to_string())),
        [RuntimeValue::String(v), RuntimeValue::Number(n)] => {
            let shift_amount = n.to_int().max(0) as usize;
            let shifted: String = v.chars().skip(shift_amount).collect();
            Ok(RuntimeValue::String(shifted.into()))
        }
        [RuntimeValue::Array(arr), v] => {
            runtime_value::array_mut(arr).push(std::mem::take(v));
            Ok(RuntimeValue::Array(std::mem::take(arr)))
        }
        [RuntimeValue::Markdown(node, selector), RuntimeValue::Number(n)] => {
            if let mq_markdown::Node::Heading(heading) = runtime_value::markdown_mut(node) {
                let shift_amount = n.to_int().max(0).min(u8::MAX as i64) as u8;

                heading.depth = heading.depth.saturating_sub(shift_amount);
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[mq_macros::mq_fn(name = "shift_right", params = Fixed(2))]
fn shift_right_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
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
                Ok(RuntimeValue::String(Shared::new(String::new())))
            } else {
                let keep = char_len - shift_amount;
                let result: String = v.chars().take(keep).collect();
                Ok(RuntimeValue::String(result.into()))
            }
        }
        [v, RuntimeValue::Array(arr)] => {
            runtime_value::array_mut(arr).insert(0, std::mem::take(v));
            Ok(RuntimeValue::Array(std::mem::take(arr)))
        }
        [RuntimeValue::Markdown(node, selector), RuntimeValue::Number(n)] => {
            if let mq_markdown::Node::Heading(heading) = runtime_value::markdown_mut(node) {
                let shift_amount = n.to_int().max(0).min(u8::MAX as i64) as u8;

                if let Some(depth) = heading.depth.checked_add(shift_amount) {
                    heading.depth = depth;
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

fn build_char_inline_diff(s1: &str, s2: &str) -> (Vec<RuntimeValue>, Vec<RuntimeValue>) {
    let char_diff = TextDiff::from_chars(s1, s2);
    let mut del_inline: Vec<RuntimeValue> = Vec::new();
    let mut ins_inline: Vec<RuntimeValue> = Vec::new();
    for c in char_diff.iter_all_changes() {
        let val = RuntimeValue::String(Shared::new(c.value().to_string()));
        match c.tag() {
            ChangeTag::Delete => {
                let mut m = DictMap::default();
                m.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("delete".into())));
                m.insert(Ident::new("value"), val);
                del_inline.push(RuntimeValue::Dict(Shared::new(m)));
            }
            ChangeTag::Insert => {
                let mut m = DictMap::default();
                m.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("insert".into())));
                m.insert(Ident::new("value"), val);
                ins_inline.push(RuntimeValue::Dict(Shared::new(m)));
            }
            ChangeTag::Equal => {
                for inline in [&mut del_inline, &mut ins_inline] {
                    let mut m = DictMap::default();
                    m.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("equal".into())));
                    m.insert(
                        Ident::new("value"),
                        RuntimeValue::String(Shared::new(c.value().to_string())),
                    );
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
                        let mut del_map = DictMap::default();
                        del_map.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("delete".into())));
                        del_map.insert(Ident::new("value"), old_val.clone());
                        del_map.insert(Ident::new("inline"), RuntimeValue::Array(Shared::new(del_inline)));
                        result.push(RuntimeValue::Dict(Shared::new(del_map)));
                        let mut ins_map = DictMap::default();
                        ins_map.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("insert".into())));
                        ins_map.insert(Ident::new("value"), new_val.clone());
                        ins_map.insert(Ident::new("inline"), RuntimeValue::Array(Shared::new(ins_inline)));
                        result.push(RuntimeValue::Dict(Shared::new(ins_map)));
                    } else {
                        let mut del_map = DictMap::default();
                        del_map.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("delete".into())));
                        del_map.insert(Ident::new("value"), old_val.clone());
                        result.push(RuntimeValue::Dict(Shared::new(del_map)));
                        let mut ins_map = DictMap::default();
                        ins_map.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("insert".into())));
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
                    let mut map = DictMap::default();
                    map.insert(Ident::new("tag"), RuntimeValue::String(Shared::new(tag_str.into())));
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
                    let mut del_map = DictMap::default();
                    del_map.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("delete".into())));
                    del_map.insert(
                        Ident::new("value"),
                        RuntimeValue::String(Shared::new(old_val.to_string())),
                    );
                    del_map.insert(Ident::new("inline"), RuntimeValue::Array(Shared::new(del_inline)));
                    result.push(RuntimeValue::Dict(Shared::new(del_map)));
                    let mut ins_map = DictMap::default();
                    ins_map.insert(Ident::new("tag"), RuntimeValue::String(Shared::new("insert".into())));
                    ins_map.insert(
                        Ident::new("value"),
                        RuntimeValue::String(Shared::new(new_val.to_string())),
                    );
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
                    let mut map = DictMap::default();
                    map.insert(Ident::new("tag"), RuntimeValue::String(Shared::new(tag_str.into())));
                    map.insert(Ident::new("value"), RuntimeValue::String(val.into()));
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
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(Shared::new(path::basename(s)))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "dirname", params = Fixed(1))]
fn dirname_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(Shared::new(path::dirname(s)))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "extname", params = Fixed(1))]
fn extname_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(Shared::new(path::extname(s)))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "stem", params = Fixed(1))]
fn stem_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(s)] => Ok(RuntimeValue::String(Shared::new(path::stem(s)))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[mq_macros::mq_fn(name = "path_join", params = Fixed(2))]
fn path_join_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(base), RuntimeValue::String(component)] => {
            path::path_join(base, component).map(|s| RuntimeValue::String(s.into()))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
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
            .map(|s| RuntimeValue::String(s.into()))
            .map_err(|e| Error::Runtime(format!("Failed to read file {}: {}", path, e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "file_info", params = Fixed(1))]
fn file_info_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => {
            let metadata = io_context::current()
                .metadata(std::path::Path::new(path.as_str()))
                .map_err(|e| Error::Runtime(format!("Failed to get info for {}: {}", path, e)))?;

            let mut record = DictMap::default();
            record.insert(Ident::new("path"), RuntimeValue::String(path.clone()));
            record.insert(
                Ident::new("kind"),
                RuntimeValue::String(Shared::new(metadata.kind.as_str().to_string())),
            );
            record.insert(
                Ident::new("size"),
                RuntimeValue::Number((metadata.size as usize).into()),
            );
            record.insert(
                Ident::new("modified"),
                metadata
                    .modified
                    .map(|secs| RuntimeValue::Number(secs.into()))
                    .unwrap_or(RuntimeValue::NONE),
            );

            Ok(RuntimeValue::Dict(Shared::new(record)))
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
            .map(|b| RuntimeValue::Bytes(b.into()))
            .map_err(|e| Error::Runtime(format!("Failed to read file {}: {}", path, e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

/// Opens `path` for incremental reading and returns a file handle. Requires the ambient
/// [`Io`]'s read permission (see [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "open_file", params = Fixed(1))]
fn open_file_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => io_context::current()
            .open_read(std::path::Path::new(path.as_str()))
            .map(|reader| {
                RuntimeValue::ReaderHandle(Shared::new(ReaderHandle::new(
                    HandleKind::File,
                    path.to_string(),
                    reader,
                )))
            })
            .map_err(|e| Error::Runtime(format!("Failed to open file {}: {}", path, e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

/// Reads the next line from a reader handle without its terminator, or `None` at end of file.
#[cfg(any(feature = "file-io", feature = "http"))]
#[mq_macros::mq_fn(name = "read_line", params = Fixed(1))]
fn read_line_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::ReaderHandle(handle)] => handle
            .read_line()
            .map(|line| {
                line.map(|l| RuntimeValue::String(l.into()))
                    .unwrap_or(RuntimeValue::NONE)
            })
            .map_err(|e| Error::Runtime(format!("Failed to read {}: {}", handle.path(), e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

/// Reads up to `size` bytes from a reader handle, or `None` at end of file.
#[cfg(any(feature = "file-io", feature = "http"))]
#[mq_macros::mq_fn(name = "read_bytes", params = Fixed(2))]
fn read_bytes_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::ReaderHandle(handle), RuntimeValue::Number(size)] => {
            let raw_size = size.value();
            let size = bounded_size(size, MAX_READ_BYTES, "read_bytes size")?;
            if size == 0 {
                return Err(Error::Runtime(format!(
                    "{ident}: size must be a positive integer, got {raw_size}"
                )));
            }
            handle
                .read_bytes(size)
                .map(|bytes| {
                    bytes
                        .map(|b| RuntimeValue::Bytes(b.into()))
                        .unwrap_or(RuntimeValue::NONE)
                })
                .map_err(|e| Error::Runtime(format!("Failed to read {}: {}", handle.path(), e)))
        }
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        _ => Err(invalid_arity(ident, 2, args.len())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

/// Creates the directory `path` and any missing parents, like `mkdir -p`. Succeeds if it
/// already exists. Requires the ambient [`Io`]'s write permission (see [`io_context`]).
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "mkdir", params = Fixed(1))]
fn mkdir_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(path)] => io_context::current()
            .create_dir_all(std::path::Path::new(path.as_str()))
            .map(|()| RuntimeValue::NONE)
            .map_err(|e| Error::Runtime(format!("Failed to create directory {}: {}", path, e))),
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        _ => Err(invalid_arity(ident, 1, args.len())),
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
fn embed_images_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [a @ RuntimeValue::Markdown(_, _)] => embed_image(a, "."),
        [a @ RuntimeValue::Markdown(_, _), RuntimeValue::String(base_dir)] => {
            let base_dir = std::mem::take(base_dir);
            embed_image(a, &base_dir)
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[cfg(feature = "file-io")]
fn embed_image(arg: &mut RuntimeValue, base_dir: &str) -> Result<RuntimeValue, Error> {
    let RuntimeValue::Markdown(node, _) = arg else {
        unreachable!()
    };
    let mq_markdown::Node::Image(image) = runtime_value::markdown_mut(node) else {
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
fn extract_images_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [a @ RuntimeValue::Markdown(_, _), RuntimeValue::String(dir)] => {
            let dir = std::mem::take(dir);
            extract_image(a, &dir)
        }
        [a, ..] => Ok(std::mem::take(a)),
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[cfg(feature = "file-io")]
fn extract_image(arg: &mut RuntimeValue, dir: &str) -> Result<RuntimeValue, Error> {
    let RuntimeValue::Markdown(node, _) = arg else {
        unreachable!()
    };
    let mq_markdown::Node::Image(image) = runtime_value::markdown_mut(node) else {
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

/// Opens an HTTPS request with the given method (`"get"`/`:get`, `"post"`/`:post`, etc.) and
/// returns the response body as a streaming reader handle instead of buffering it, so a large
/// or slow response can be processed line by line as it arrives. `body`/`headers` behave the
/// same as [`http_impl`]. The connection is closed by `close(handle)`, or automatically when
/// the handle (or a coroutine holding it) is closed, finishes, or is dropped. Requires the
/// ambient [`Io`]'s net permission (see [`io_context`]).
#[cfg(feature = "http")]
#[mq_macros::mq_fn(name = "open_http", params = Range(2, 4))]
fn open_http_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [
            method @ (RuntimeValue::Symbol(_) | RuntimeValue::String(_)),
            RuntimeValue::String(url),
        ] => http::open_stream(method, url, None, None),
        [
            method @ (RuntimeValue::Symbol(_) | RuntimeValue::String(_)),
            RuntimeValue::String(url),
            RuntimeValue::String(body),
        ] => http::open_stream(method, url, Some(body), None),
        [
            method @ (RuntimeValue::Symbol(_) | RuntimeValue::String(_)),
            RuntimeValue::String(url),
            RuntimeValue::Dict(headers),
        ] => http::open_stream(method, url, None, Some(headers)),
        [
            method @ (RuntimeValue::Symbol(_) | RuntimeValue::String(_)),
            RuntimeValue::String(url),
            RuntimeValue::String(body),
            RuntimeValue::Dict(headers),
        ] => http::open_stream(method, url, Some(body), Some(headers)),
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
                bodies.into_iter().map(|s| RuntimeValue::String(s.into())).collect(),
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
            RuntimeValue::String(url) => Ok(url.to_string()),
            other => Err(Error::Runtime(format!("http_all: `url` must be a string, got {other}"))),
        })?;
    let method = match fields.get(&Ident::from("method")) {
        Some(RuntimeValue::Symbol(method)) => method.to_string(),
        Some(RuntimeValue::String(method)) => method.to_string(),
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
        Some(RuntimeValue::String(body)) => Some(body.to_string()),
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
                RuntimeValue::String(value) => Ok((name.to_string(), value.to_string())),
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
        _ => Err(invalid_arity(ident, 2, args.len())),
    }
}

#[cfg(feature = "process-io")]
fn runtime_args_to_strings(command: &str, arr: &[RuntimeValue]) -> Result<Vec<String>, Error> {
    arr.iter()
        .map(|v| match v {
            RuntimeValue::String(s) => Ok(s.to_string()),
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
            .map(|s| RuntimeValue::String(s.into()))
            .map_err(|e| Error::Runtime(format!("Failed to execute {}: {}", command, e))),
        [RuntimeValue::String(command), RuntimeValue::Array(arr)] => {
            let cmd_args = runtime_args_to_strings(command, arr)?;
            io_context::current()
                .execute(command, &cmd_args)
                .map(|s| RuntimeValue::String(s.into()))
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
            let value: toml::Value = toml::from_str(&toml_node.value)
                .map_err(|e| Error::Runtime(format!("Failed to parse TOML frontmatter in {}: {}", path, e)))?;
            (toml_value_to_runtime_value(value), &nodes[1..])
        }
        _ => (RuntimeValue::NONE, &nodes[..]),
    };

    let title = nodes
        .iter()
        .find(|node| matches!(node, mq_markdown::Node::Heading(_)))
        .map(|node| RuntimeValue::String(Shared::new(node.value())))
        .unwrap_or(RuntimeValue::NONE);

    let content = RuntimeValue::Array(Shared::new(
        body_nodes.iter().cloned().map(RuntimeValue::from).collect(),
    ));

    let mut record = DictMap::default();
    record.insert(Ident::new("path"), RuntimeValue::String(path.into()));
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

/// Shared recursive directory walker backing both `collection` and `walk_files`. It detects symlink
/// cycles (via `ancestors`, keyed by canonical path), skips hidden and gitignored entries when
/// `respect_gitignore` is set, and descends into symlinked directories only if `follow_symlink_dirs`
/// is set. Every non-directory entry that survives filtering and an existence check (guarding
/// against broken symlinks) is reported to `visit_file` as `(path, rel_path)`: `path` is the full
/// path as read from `io.read_dir`, and `rel_path` is relative to the directory the walk started
/// from. Callers apply their own inclusion predicate from inside `visit_file`.
#[cfg(feature = "file-io")]
struct DirWalk<'a> {
    io: &'a dyn Io,
    respect_gitignore: bool,
    follow_symlink_dirs: bool,
    ancestors: FxHashSet<std::path::PathBuf>,
    gitignore_stack: Vec<ignore::gitignore::Gitignore>,
}

#[cfg(feature = "file-io")]
impl<'a> DirWalk<'a> {
    fn new(io: &'a dyn Io, respect_gitignore: bool, follow_symlink_dirs: bool) -> Self {
        Self {
            io,
            respect_gitignore,
            follow_symlink_dirs,
            ancestors: FxHashSet::default(),
            gitignore_stack: Vec::new(),
        }
    }

    fn walk(
        &mut self,
        dir: &std::path::Path,
        rel_dir: &std::path::Path,
        visit_file: &mut dyn FnMut(&std::path::Path, &std::path::Path),
    ) -> Result<(), Error> {
        // Also rejects a symlinked root, not just a symlinked subdirectory found during recursion.
        if !self.follow_symlink_dirs && matches!(self.io.metadata(dir).map(|m| m.kind), Ok(FileKind::Symlink)) {
            return Ok(());
        }

        let canonical = self.io.canonicalize(dir);
        if !self.ancestors.insert(canonical.clone()) {
            return Ok(());
        }

        let pushed_gitignore = self.respect_gitignore
            && load_gitignore(self.io, dir)
                .map(|gi| self.gitignore_stack.push(gi))
                .is_some();

        let entries = self
            .io
            .read_dir(dir)
            .map_err(|e| Error::Runtime(format!("Failed to read directory {}: {}", dir.display(), e)))?;

        for (path, is_dir) in entries {
            // Keep as `OsStr`: a non-UTF-8 filename is valid on Unix and must still be walked.
            let Some(file_name) = path.file_name() else {
                continue;
            };

            if self.respect_gitignore {
                let is_hidden = file_name.to_string_lossy().starts_with('.');
                if is_hidden || is_gitignored(&self.gitignore_stack, &path, is_dir) {
                    continue;
                }
            }

            let rel_path = if rel_dir.as_os_str().is_empty() {
                std::path::PathBuf::from(file_name)
            } else {
                rel_dir.join(file_name)
            };

            if is_dir {
                self.walk(&path, &rel_path, visit_file)?;
            } else if self.io.exists(&path).unwrap_or(false) {
                visit_file(&path, &rel_path);
            }
        }

        if pushed_gitignore {
            self.gitignore_stack.pop();
        }
        self.ancestors.remove(&canonical);
        Ok(())
    }
}

#[cfg(feature = "file-io")]
fn collection_impl_inner(dir: &str, respect_gitignore: bool) -> Result<RuntimeValue, Error> {
    let io = io_context::current();
    let mut paths = Vec::new();

    DirWalk::new(io.as_ref(), respect_gitignore, true).walk(
        std::path::Path::new(dir),
        std::path::Path::new(""),
        &mut |path, _rel| {
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
            {
                paths.push(path.to_path_buf());
            }
        },
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
        _ => Err(invalid_arity(ident, 1, args.len())),
    }
}

#[cfg(feature = "file-io")]
fn walk_files_impl_inner(
    root: &str,
    pattern: &str,
    respect_gitignore: bool,
    follow_symlinks: bool,
) -> Result<RuntimeValue, Error> {
    let glob_pattern = glob::Pattern::new(pattern)
        .map_err(|e| Error::Runtime(format!("walk_files: invalid pattern {:?}: {}", pattern, e)))?;
    let match_options = glob::MatchOptions {
        require_literal_separator: true,
        ..glob::MatchOptions::new()
    };

    let io = io_context::current();
    let mut out = Vec::new();

    DirWalk::new(io.as_ref(), respect_gitignore, follow_symlinks).walk(
        std::path::Path::new(root),
        std::path::Path::new(""),
        &mut |_path, rel| {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if glob_pattern.matches_with(&rel_str, match_options) {
                out.push(rel_str);
            }
        },
    )?;

    out.sort();

    Ok(RuntimeValue::Array(Shared::new(
        out.into_iter().map(|p| RuntimeValue::String(p.into())).collect(),
    )))
}

/// Recursively enumerates filesystem entries under `root` whose root-relative path (using
/// `/` as the separator regardless of platform) matches the glob `pattern` (default
/// `"**"`, i.e. every file), returning a stable-sorted array of those relative path
/// strings. Unlike `collection`, this reads neither file contents nor Markdown; it is a
/// general-purpose enumeration primitive for reports that mix non-Markdown files,
/// size/mtime-based inventories, or incremental processing, without escaping to a shell
/// `find`. Requires the ambient [`Io`]'s read permission (see [`io_context`]).
///
/// An optional third argument is a dict of options:
/// - `respect_gitignore` (default `false`): skip dotfiles/dot-directories and any path
///   matched by a `.gitignore`, same semantics as `collection`.
/// - `follow_symlinks` (default `false`): whether to descend into symlinked directories.
///   A symlinked *file* is always included if it matches the pattern; only directory
///   traversal is affected, and symlink cycles are always broken regardless of this
///   option.
#[cfg(feature = "file-io")]
#[mq_macros::mq_fn(name = "walk_files", params = Range(1, 3))]
fn walk_files_impl(ident: &Ident, _: &RuntimeValue, mut args: Args, _: &SharedEnv) -> Result<RuntimeValue, Error> {
    match args.as_mut_slice() {
        [RuntimeValue::String(root)] => walk_files_impl_inner(root.as_str(), "**", false, false),
        [RuntimeValue::String(root), RuntimeValue::String(pattern)] => {
            walk_files_impl_inner(root.as_str(), pattern.as_str(), false, false)
        }
        [
            RuntimeValue::String(root),
            RuntimeValue::String(pattern),
            RuntimeValue::Dict(options),
        ] => {
            let respect_gitignore = matches!(options.get("respect_gitignore"), Some(RuntimeValue::Boolean(true)));
            let follow_symlinks = matches!(options.get("follow_symlinks"), Some(RuntimeValue::Boolean(true)));
            walk_files_impl_inner(root.as_str(), pattern.as_str(), respect_gitignore, follow_symlinks)
        }
        [a] => Err(Error::InvalidTypes(ident.to_string(), vec![std::mem::take(a)])),
        [a, b] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b)],
        )),
        [a, b, c] => Err(Error::InvalidTypes(
            ident.to_string(),
            vec![std::mem::take(a), std::mem::take(b), std::mem::take(c)],
        )),
        _ => Err(invalid_arity(ident, 1, args.len())),
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

thread_local! {
    static BUILTIN_LOOKUP_CACHE: std::cell::RefCell<FxHashMap<Ident, Option<&'static BuiltinFunction>>> =
        std::cell::RefCell::new(FxHashMap::default());
}

pub fn get_builtin_functions(name: &Ident) -> Option<&'static BuiltinFunction> {
    BUILTIN_LOOKUP_CACHE.with(|cache| {
        if let Some(found) = cache.borrow().get(name) {
            return *found;
        }
        let result = name.resolve_with(get_builtin_functions_by_str);
        cache.borrow_mut().insert(*name, result);
        result
    })
}

mq_macros::builtin_dispatch! {
    PARTIAL,
    HALT,
    ERROR,
    PRINT,
    STDERR,
    TYPE,
    CLOSE,
    STATUS,
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
    BASE64D_BYTES,
    BASE64URLD_BYTES,
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
    #[cfg(feature = "html-to-markdown")]
    FROM_HTML,
    TO_HTML,
    HTML_ESCAPE,
    HTML_UNESCAPE,
    MARKDOWN_ESCAPE,
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
    HEXDUMP,
    UTF8,
    DECODE,
    ENCODE,
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
    _REGEX_REPLACE_MATCHES,
    REGEX_ESCAPE,
    DOWNCASE,
    ASCII_DOWNCASE,
    CASEFOLD,
    UNICODE_NORMALIZE,
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
    LOG,
    SQRT,
    EXP,
    SIN,
    COS,
    TAN,
    ASIN,
    ACOS,
    ATAN,
    SINH,
    COSH,
    TANH,
    LOG2,
    CBRT,
    ATAN2,
    HYPOT,
    INDEX,
    INDICES,
    CONTAINS,
    LEN,
    UTF8BYTELEN,
    GRAPHEMES,
    GRAPHEME_LEN,
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
    SPLIT_RECORDS,
    EXTRACT_URLS,
    CEIL,
    FLOOR,
    ROUND,
    TRUNC,
    ABS,
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
    _MD_HEADING_LEVEL,
    SET_LIST_ORDERED,
    TO_STRONG,
    TO_EM,
    TO_BLOCKQUOTE,
    TO_DELETE,
    TO_CALLOUT,
    TO_FOOTNOTE,
    TO_FOOTNOTE_REF,
    TO_DEFINITION,
    TO_MD_HTML,
    TO_BREAK,
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
    HAS,
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
    #[internal]
    IS_DEBUG_MODE,
    ENV,
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
    FILE_INFO,
    #[cfg(feature = "file-io")]
    READ_FILE_BYTES,
    #[cfg(feature = "file-io")]
    OPEN_FILE,
    #[cfg(any(feature = "file-io", feature = "http"))]
    READ_LINE,
    #[cfg(any(feature = "file-io", feature = "http"))]
    READ_BYTES,
    #[cfg(feature = "file-io")]
    COLLECTION,
    #[cfg(feature = "file-io")]
    WALK_FILES,
    #[cfg(feature = "file-io")]
    WRITE_FILE,
    #[cfg(feature = "file-io")]
    MKDIR,
    #[cfg(feature = "file-io")]
    EMBED_IMAGES,
    #[cfg(feature = "file-io")]
    EXTRACT_IMAGES,
    #[cfg(feature = "http")]
    HTTP,
    #[cfg(feature = "http")]
    OPEN_HTTP,
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
    #[cfg(feature = "css-selector")]
    _HTML_PARSE,
}

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("")]
    InvalidBase64String(#[from] base64::DecodeError),
    #[error("")]
    NotDefined(FunctionName, Vec<String>),
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
    InvalidConvert(String),
    #[error("")]
    Halt(i32),
}

impl Error {
    #[cold]
    pub fn to_runtime_error(&self, token_id: TokenId, token_arena: Shared<SharedCell<Arena<Token>>>) -> RuntimeError {
        match self {
            Error::UserDefined(message) => RuntimeError::UserDefined {
                message: message.to_owned(),
                token: get_token(&token_arena, token_id),
            },
            Error::InvalidBase64String(e) => {
                RuntimeError::InvalidBase64String(get_token(&token_arena, token_id), e.to_string())
            }
            Error::NotDefined(name, candidates) => RuntimeError::NotDefined(
                get_token(&token_arena, token_id),
                name.clone(),
                candidates.clone().into(),
            ),
            Error::InvalidDateTimeFormat(msg) => {
                RuntimeError::DateTimeFormatError(get_token(&token_arena, token_id), msg.clone())
            }
            Error::InvalidTypes(name, args) => RuntimeError::InvalidTypes {
                token: get_token(&token_arena, token_id),
                name: name.clone(),
                args: args.iter().map(|o| o.name().into()).collect::<Vec<_>>(),
            },
            Error::InvalidNumberOfArguments(name, expected, got) => RuntimeError::InvalidNumberOfArguments {
                token: get_token(&token_arena, token_id),
                name: name.clone(),
                expected: usize::from(*expected),
                actual: usize::from(*got),
            },
            Error::InvalidRegularExpression(regex) => {
                RuntimeError::InvalidRegularExpression(get_token(&token_arena, token_id), regex.clone())
            }
            Error::Runtime(msg) => RuntimeError::Runtime(get_token(&token_arena, token_id), msg.clone()),
            Error::ZeroDivision => RuntimeError::ZeroDivision(get_token(&token_arena, token_id)),
            Error::InvalidConvert(format) => {
                RuntimeError::InvalidConvert(get_token(&token_arena, token_id), format.clone())
            }
            Error::Halt(code) => RuntimeError::Halt(*code),
        }
    }
}
#[inline(always)]
pub fn eval_builtin(
    runtime_value: &RuntimeValue,
    ident: &Ident,
    args: Args,
    env: &SharedEnv,
) -> Result<RuntimeValue, Error> {
    get_builtin_functions(ident).map_or_else(
        || {
            let candidates = env.defined_names();

            Err(Error::NotDefined(ident.to_string(), candidates))
        },
        |f| eval_resolved_builtin(f, runtime_value, ident, args, env),
    )
}

/// Runs an already resolved builtin, checking the argument count and filling in the implicit
/// `self` argument when exactly one is missing.
#[inline]
pub fn eval_resolved_builtin(
    f: &BuiltinFunction,
    runtime_value: &RuntimeValue,
    ident: &Ident,
    args: Args,
    env: &SharedEnv,
) -> Result<RuntimeValue, Error> {
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
}

/// Error for a builtin that received an argument count its `ParamNum` should have rejected.
fn invalid_arity(name: impl ToString, expected: u8, got: usize) -> Error {
    Error::InvalidNumberOfArguments(name.to_string(), expected, got as u8)
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
            RuntimeValue::String(s) => vec![(**s).clone()],
            RuntimeValue::Array(arr) => arr
                .iter()
                .filter_map(|v| {
                    if let RuntimeValue::String(s) = v {
                        Some((**s).clone())
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
                depths.contains(&depth.get())
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
        Selector::Link => {
            let urls = collect_string_values(args);

            if urls.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::Link(mq_markdown::Link { url, .. }) = node {
                urls.iter().any(|u| u == url.as_str())
            } else {
                false
            }
        }
        Selector::Image => {
            let urls = collect_string_values(args);

            if urls.is_empty() {
                return eval_selector(node, selector);
            }

            if let mq_markdown::Node::Image(mq_markdown::Image { url, .. }) = node {
                urls.iter().any(|u| u == url)
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
                    RuntimeValue::Number(n) => n.is_int() && n.value() >= 0.0 && *list_index == n.value() as usize,
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
                            Some(RuntimeValue::Number(n)) => {
                                n.is_int() && n.value() >= 0.0 && actual == n.value() as usize
                            }
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
                return Ok(RuntimeValue::Bytes(Shared::new(vec![])));
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
            Ok(RuntimeValue::Bytes(repeated.into()))
        }
        RuntimeValue::None => Ok(RuntimeValue::NONE),
        _ => Err(Error::InvalidTypes(
            constants::builtins::MUL.to_string(),
            vec![std::mem::take(value), RuntimeValue::Number(n.into())],
        )),
    }
}

#[cfg(test)]
mod tests;
