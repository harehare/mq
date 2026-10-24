//! Typed query results for the `_ex` API.

use crate::error::{FfiError, MqError, MqStatus, run_status};
use crate::{MqContext, eval_core, to_c_string, to_cstring};
use mq_lang::RuntimeValue;
use std::ffi::CString;
use std::os::raw::c_char;
use std::ptr;
use std::sync::OnceLock;

/// Opaque list of query results, freed with `mq_values_free`.
pub struct MqValues {
    values: Vec<RuntimeValue>,
    // Strings handed out by the accessors, built on first access.
    cache: Vec<ValueCache>,
}

#[derive(Default)]
struct ValueCache {
    string: OnceLock<CString>,
    node_kind: OnceLock<CString>,
}

impl From<Vec<RuntimeValue>> for MqValues {
    fn from(values: Vec<RuntimeValue>) -> Self {
        Self {
            cache: values.iter().map(|_| ValueCache::default()).collect(),
            values,
        }
    }
}

impl MqValues {
    fn get(&self, index: usize) -> Option<(&RuntimeValue, &ValueCache)> {
        self.values.get(index).zip(self.cache.get(index))
    }
}

/// Type of a result value. A JSON object can be a dict or a markdown node, and a JSON
/// string can be a string, a symbol or bytes, so use this to tell them apart.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum MqType {
    /// No such value: a null handle or an index out of range.
    MQ_TYPE_INVALID = 0,
    /// The mq `None` value.
    MQ_TYPE_NONE,
    /// `true` or `false`.
    MQ_TYPE_BOOL,
    /// A number; see `mq_value_number`.
    MQ_TYPE_NUMBER,
    /// A string.
    MQ_TYPE_STRING,
    /// A symbol.
    MQ_TYPE_SYMBOL,
    /// An array. Use `mq_value_to_json` to read its elements.
    MQ_TYPE_ARRAY,
    /// A dict. Use `mq_value_to_json` to read its entries.
    MQ_TYPE_DICT,
    /// A markdown node; see `mq_value_node_kind` and `mq_value_position`.
    MQ_TYPE_MARKDOWN,
    /// Raw bytes.
    MQ_TYPE_BYTES,
    /// A function, coroutine or other value that has no data form.
    MQ_TYPE_OTHER,
}

impl From<&RuntimeValue> for MqType {
    fn from(value: &RuntimeValue) -> Self {
        match value {
            RuntimeValue::None => MqType::MQ_TYPE_NONE,
            RuntimeValue::Boolean(_) => MqType::MQ_TYPE_BOOL,
            RuntimeValue::Number(_) => MqType::MQ_TYPE_NUMBER,
            RuntimeValue::String(_) => MqType::MQ_TYPE_STRING,
            RuntimeValue::Symbol(_) => MqType::MQ_TYPE_SYMBOL,
            RuntimeValue::Array(_) => MqType::MQ_TYPE_ARRAY,
            RuntimeValue::Dict(_) => MqType::MQ_TYPE_DICT,
            RuntimeValue::Markdown(..) => MqType::MQ_TYPE_MARKDOWN,
            RuntimeValue::Bytes(_) => MqType::MQ_TYPE_BYTES,
            _ => MqType::MQ_TYPE_OTHER,
        }
    }
}

/// Position of a markdown node in the input. Lines and columns start at 1 and columns
/// count UTF-16 code units.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MqPosition {
    /// Line where the node starts.
    pub start_line: u32,
    /// Column where the node starts.
    pub start_column: u32,
    /// Line where the node ends.
    pub end_line: u32,
    /// Column where the node ends.
    pub end_column: u32,
}

/// Evaluates mq code like `mq_eval`, reporting failures as a status plus an `mq_error_t`.
///
/// On success returns `MQ_OK` and stores the results in `*out`, to be freed with
/// `mq_values_free`. On failure stores null in `*out` and, if `err` is non-null, an error
/// in `*err`, to be freed with `mq_error_free`.
///
/// # Safety
///
/// `out` must be valid for writes and `err` must be null or valid for writes. The other
/// arguments are as for `mq_eval`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_eval_ex(
    engine_ptr: *mut MqContext,
    code_c: *const c_char,
    input_c: *const c_char,
    input_format_c: *const c_char,
    out: *mut *mut MqValues,
    err: *mut *mut MqError,
) -> MqStatus {
    run_status(err, || {
        if out.is_null() {
            return Err(FfiError::invalid_argument("Output pointer is null"));
        }
        unsafe { *out = ptr::null_mut() };

        let values = unsafe { eval_core(engine_ptr, code_c, input_c, input_format_c) }?;
        unsafe { *out = Box::into_raw(Box::new(values.into())) };
        Ok(())
    })
}

/// Returns the number of results, or 0 if `values` is null.
///
/// # Safety
///
/// `values` must be null or a pointer returned by `mq_eval_ex` and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_values_len(values: *const MqValues) -> usize {
    unsafe { values.as_ref() }.map_or(0, |v| v.values.len())
}

/// Returns result `index` rendered as text, the same text `mq_eval` returns. The string is
/// owned by `values` and valid until `mq_values_free`. Returns null if `index` is out of
/// range.
///
/// # Safety
///
/// See `mq_values_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_value_string(values: *const MqValues, index: usize) -> *const c_char {
    unsafe { values.as_ref() }
        .and_then(|values| values.get(index))
        .map_or(ptr::null(), |(value, cache)| {
            cache.string.get_or_init(|| to_cstring(value.to_string())).as_ptr()
        })
}

/// Returns the type of result `index`, or `MQ_TYPE_INVALID` if there is no such result.
///
/// # Safety
///
/// See `mq_values_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_value_type(values: *const MqValues, index: usize) -> MqType {
    unsafe { values.as_ref() }
        .and_then(|values| values.get(index))
        .map_or(MqType::MQ_TYPE_INVALID, |(value, _)| value.into())
}

/// Stores a number result in `out` and returns true, or returns false if the result is not
/// a number.
///
/// # Safety
///
/// `out` must be null or valid for writes. See also `mq_values_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_value_number(values: *const MqValues, index: usize, out: *mut f64) -> bool {
    let number = unsafe { values.as_ref() }
        .and_then(|values| values.get(index))
        .and_then(|(value, _)| match value {
            RuntimeValue::Number(n) => Some(n.value()),
            _ => None,
        });
    store(number, out)
}

/// Stores a boolean result in `out` and returns true, or returns false if the result is not
/// a boolean.
///
/// # Safety
///
/// `out` must be null or valid for writes. See also `mq_values_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_value_bool(values: *const MqValues, index: usize, out: *mut bool) -> bool {
    let boolean = unsafe { values.as_ref() }
        .and_then(|values| values.get(index))
        .and_then(|(value, _)| match value {
            RuntimeValue::Boolean(b) => Some(*b),
            _ => None,
        });
    store(boolean, out)
}

/// Returns the kind of a markdown result, such as `h1`, `code` or `list`, or null if the
/// result is not a markdown node. Owned by `values`.
///
/// # Safety
///
/// See `mq_values_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_value_node_kind(values: *const MqValues, index: usize) -> *const c_char {
    unsafe { values.as_ref() }
        .and_then(|values| values.get(index))
        .and_then(|(value, cache)| match value {
            RuntimeValue::Markdown(node, _) => {
                Some(cache.node_kind.get_or_init(|| to_cstring(node.name().to_string())))
            }
            _ => None,
        })
        .map_or(ptr::null(), |kind| kind.as_ptr())
}

/// Stores the input position of a markdown result in `out` and returns true, or returns
/// false if the result has none, for example because it is not a markdown node or was
/// built by the query.
///
/// # Safety
///
/// `out` must be null or valid for writes. See also `mq_values_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_value_position(values: *const MqValues, index: usize, out: *mut MqPosition) -> bool {
    let position = unsafe { values.as_ref() }
        .and_then(|values| values.get(index))
        .and_then(|(value, _)| value.position())
        .map(|p| MqPosition {
            start_line: saturate(p.start.line),
            start_column: saturate(p.start.column),
            end_line: saturate(p.end.line),
            end_column: saturate(p.end.column),
        });
    store(position, out)
}

/// Returns result `index` as a JSON string, to be freed with `mq_free_string`, or null if
/// there is no such result. Markdown nodes become objects, bytes base64 strings, and
/// functions `null`. Use `mq_value_type` to tell values apart that share a JSON form.
///
/// # Safety
///
/// See `mq_values_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_value_to_json(values: *const MqValues, index: usize) -> *mut c_char {
    unsafe { values.as_ref() }
        .and_then(|values| values.get(index))
        .map_or(ptr::null_mut(), |(value, _)| {
            to_c_string(value.clone().to_json_value().to_string())
        })
}

/// Returns all results as one JSON array string, to be freed with `mq_free_string`. See
/// `mq_value_to_json`. Returns null if `values` is null.
///
/// # Safety
///
/// See `mq_values_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_values_to_json(values: *const MqValues) -> *mut c_char {
    unsafe { values.as_ref() }.map_or(ptr::null_mut(), |values| {
        let array = values.values.iter().map(|v| v.clone().to_json_value()).collect();
        to_c_string(serde_json::Value::Array(array).to_string())
    })
}

fn saturate(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

// Writes `value` to `out` if both exist.
fn store<T>(value: Option<T>, out: *mut T) -> bool {
    match (value, unsafe { out.as_mut() }) {
        (Some(value), Some(out)) => {
            *out = value;
            true
        }
        _ => false,
    }
}

/// Frees results. Does nothing if `values` is null.
///
/// # Safety
///
/// `values` must be null or a pointer returned by `mq_eval_ex`, and must not be used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_values_free(values: *mut MqValues) {
    if !values.is_null() {
        drop(unsafe { Box::from_raw(values) });
    }
}
