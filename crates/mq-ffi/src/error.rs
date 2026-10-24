//! Structured errors for the `_ex` API.

use crate::{panic_message, to_cstring};
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};
use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::ptr;

/// Outcome of an `_ex` call. `MQ_OK` is zero; every other value is an error.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum MqStatus {
    /// Success.
    MQ_OK = 0,
    /// A null pointer, invalid UTF-8 or unsupported input format.
    MQ_ERR_INVALID_ARGUMENT,
    /// The input document could not be parsed.
    MQ_ERR_INPUT_PARSE,
    /// The query has a syntax error.
    MQ_ERR_SYNTAX,
    /// The query failed while running.
    MQ_ERR_RUNTIME,
    /// A module could not be resolved or loaded.
    MQ_ERR_MODULE,
    /// The timeout set by `mq_set_timeout_ms` was exceeded.
    MQ_ERR_TIMEOUT,
    /// The query called `halt`; see `mq_error_exit_code`.
    MQ_ERR_HALT,
    /// The library was built without the requested feature.
    MQ_ERR_UNSUPPORTED,
    /// A panic was caught at the API boundary.
    MQ_ERR_INTERNAL,
}

/// Position of an error in its source.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MqLocation {
    /// Line, starting at 1.
    pub line: u32,
    /// Column in characters, starting at 1.
    pub column: u32,
    /// Byte offset from the start of the source.
    pub offset: usize,
    /// Length of the span in bytes.
    pub length: usize,
}

/// Opaque error details, freed with `mq_error_free`.
pub struct MqError {
    status: MqStatus,
    message: CString,
    rendered: CString,
    help: Option<CString>,
    source_name: CString,
    location: Option<MqLocation>,
    exit_code: Option<i32>,
}

/// An error on its way out of the library, before it is turned into either a legacy
/// message string or an `MqError`.
pub(crate) struct FfiError {
    pub(crate) status: MqStatus,
    // What the legacy `char *` APIs put in front of `message`.
    legacy_prefix: &'static str,
    message: String,
    detail: Option<Box<mq_lang::Error>>,
}

impl FfiError {
    pub(crate) fn new(status: MqStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            legacy_prefix: "",
            message: message.into(),
            detail: None,
        }
    }

    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(MqStatus::MQ_ERR_INVALID_ARGUMENT, message)
    }

    pub(crate) fn with_prefix(status: MqStatus, legacy_prefix: &'static str, message: String) -> Self {
        Self {
            legacy_prefix,
            ..Self::new(status, message)
        }
    }

    pub(crate) fn input_parse(legacy_prefix: &'static str, message: String) -> Self {
        Self::with_prefix(MqStatus::MQ_ERR_INPUT_PARSE, legacy_prefix, message)
    }

    pub(crate) fn from_eval(error: Box<mq_lang::Error>) -> Self {
        Self::from_engine("Error evaluating query: ", error)
    }

    // `legacy_prefix` is what the pre-`_ex` APIs put in front of the message.
    pub(crate) fn from_engine(legacy_prefix: &'static str, error: Box<mq_lang::Error>) -> Self {
        let status = if error.exit_code().is_some() {
            MqStatus::MQ_ERR_HALT
        } else if error.is_timeout() {
            MqStatus::MQ_ERR_TIMEOUT
        } else if error.is_module_error() {
            MqStatus::MQ_ERR_MODULE
        } else if error.is_compile_error() {
            MqStatus::MQ_ERR_SYNTAX
        } else {
            MqStatus::MQ_ERR_RUNTIME
        };
        Self {
            status,
            legacy_prefix,
            message: error.to_string(),
            detail: Some(error),
        }
    }

    pub(crate) fn from_panic(payload: &(dyn std::any::Any + Send)) -> Self {
        Self {
            legacy_prefix: "Internal error: ",
            ..Self::new(MqStatus::MQ_ERR_INTERNAL, panic_message(payload))
        }
    }

    /// The message the pre-`_ex` APIs report for this error.
    pub(crate) fn legacy_message(&self) -> String {
        format!("{}{}", self.legacy_prefix, self.message)
    }
}

impl From<FfiError> for MqError {
    fn from(error: FfiError) -> Self {
        let Some(detail) = error.detail else {
            let message = to_cstring(error.message);
            return Self {
                status: error.status,
                rendered: message.clone(),
                message,
                help: None,
                source_name: CString::default(),
                location: None,
                exit_code: None,
            };
        };

        Self {
            status: error.status,
            message: to_cstring(error.message),
            rendered: to_cstring(render(&detail)),
            help: detail.help().map(|help| to_cstring(help.to_string())),
            source_name: to_cstring(detail.source_code.name().to_string()),
            location: locate(&detail),
            exit_code: detail.exit_code(),
        }
    }
}

// miette's graphical report without colors, so it is safe to print anywhere.
fn render(error: &mq_lang::Error) -> String {
    let mut out = String::new();
    GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
        .render_report(&mut out, error)
        .map_or_else(|_| error.to_string(), |()| out)
}

// Runtime errors raised without a token have no meaningful position.
fn locate(error: &mq_lang::Error) -> Option<MqLocation> {
    if error.cause.token().is_none() && !error.is_compile_error() {
        return None;
    }

    let source = error.source_code.inner();
    let mut offset = error.location.offset().min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;

    Some(MqLocation {
        line: line as u32,
        column: column as u32,
        offset,
        length: error.location.len(),
    })
}

/// Runs `f` at a panic boundary and reports its failure through `err`, which may be null.
pub(crate) fn run_status(err: *mut *mut MqError, f: impl FnOnce() -> Result<(), FfiError>) -> MqStatus {
    if !err.is_null() {
        unsafe { *err = ptr::null_mut() };
    }

    match std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|payload| Err(FfiError::from_panic(&*payload))) {
        Ok(()) => MqStatus::MQ_OK,
        Err(error) => {
            let status = error.status;
            if !err.is_null() {
                unsafe { *err = Box::into_raw(Box::new(error.into())) };
            }
            status
        }
    }
}

// Borrows the string stored in `error` for as long as `error` lives.
unsafe fn with_error<T>(error: *const MqError, default: T, f: impl FnOnce(&MqError) -> T) -> T {
    match unsafe { error.as_ref() } {
        Some(error) => f(error),
        None => default,
    }
}

/// Returns the error's status, or `MQ_ERR_INVALID_ARGUMENT` if `error` is null.
///
/// # Safety
///
/// `error` must be null or a pointer returned through an `err` out-parameter and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_error_status(error: *const MqError) -> MqStatus {
    unsafe { with_error(error, MqStatus::MQ_ERR_INVALID_ARGUMENT, |e| e.status) }
}

/// Returns the short error message, e.g. `Unexpected token '|'`. The string is owned by
/// `error` and valid until `mq_error_free`. Returns null if `error` is null.
///
/// # Safety
///
/// See `mq_error_status`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_error_message(error: *const MqError) -> *const c_char {
    unsafe { with_error(error, ptr::null(), |e| e.message.as_ptr()) }
}

/// Returns the message with the offending source excerpt, laid out for a terminal and
/// without colors. Owned by `error`. Equal to the message when there is no source.
///
/// # Safety
///
/// See `mq_error_status`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_error_rendered(error: *const MqError) -> *const c_char {
    unsafe { with_error(error, ptr::null(), |e| e.rendered.as_ptr()) }
}

/// Returns a hint for fixing the error, or null if there is none. Owned by `error`.
///
/// # Safety
///
/// See `mq_error_status`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_error_help(error: *const MqError) -> *const c_char {
    unsafe {
        with_error(error, ptr::null(), |e| {
            e.help.as_ref().map_or(ptr::null(), |h| h.as_ptr())
        })
    }
}

/// Returns the name of the module the error occurred in, or an empty string for the query
/// itself. Owned by `error`.
///
/// # Safety
///
/// See `mq_error_status`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_error_source_name(error: *const MqError) -> *const c_char {
    unsafe { with_error(error, ptr::null(), |e| e.source_name.as_ptr()) }
}

/// Stores the error position in `out` and returns true, or returns false if the error has
/// none.
///
/// # Safety
///
/// `out` must be null or valid for writes. See also `mq_error_status`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_error_location(error: *const MqError, out: *mut MqLocation) -> bool {
    let location = unsafe { with_error(error, None, |e| e.location) };
    match (location, unsafe { out.as_mut() }) {
        (Some(location), Some(out)) => {
            *out = location;
            true
        }
        _ => false,
    }
}

/// Stores the exit code passed to `halt` in `out` and returns true, or returns false if
/// the query did not halt.
///
/// # Safety
///
/// `out` must be null or valid for writes. See also `mq_error_status`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_error_exit_code(error: *const MqError, out: *mut i32) -> bool {
    let exit_code = unsafe { with_error(error, None, |e| e.exit_code) };
    match (exit_code, unsafe { out.as_mut() }) {
        (Some(code), Some(out)) => {
            *out = code;
            true
        }
        _ => false,
    }
}

/// Frees an error. Does nothing if `error` is null.
///
/// # Safety
///
/// `error` must be null or a pointer returned through an `err` out-parameter, and must not
/// be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_error_free(error: *mut MqError) {
    if !error.is_null() {
        drop(unsafe { Box::from_raw(error) });
    }
}
