//! HTML to Markdown conversion for the `_ex` API.

use crate::error::{FfiError, MqError, MqStatus, run_status};
use crate::{c_str_to_rust_str_slice, to_c_string};
use mq_markdown::{ConversionOptions, convert_html_to_markdown};
use std::os::raw::c_char;
use std::ptr;

/// Opaque HTML to Markdown conversion options, freed with `mq_html_options_free`.
#[derive(Default)]
pub struct MqHtmlOptions(ConversionOptions);

// Shared by `mq_html_to_markdown` and `mq_html_to_markdown_ex`.
pub(crate) unsafe fn convert(html_input_c: *const c_char, options: ConversionOptions) -> Result<String, FfiError> {
    if html_input_c.is_null() {
        return Err(FfiError::invalid_argument("HTML input pointer is null"));
    }
    let html = unsafe { c_str_to_rust_str_slice(html_input_c) }
        .map_err(|_| FfiError::invalid_argument("Invalid UTF-8 sequence in HTML input"))?;

    convert_html_to_markdown(html, options).map_err(|e| {
        FfiError::with_prefix(
            MqStatus::MQ_ERR_INPUT_PARSE,
            "HTML to Markdown conversion error: ",
            e.to_string(),
        )
    })
}

// Applies `f` to the options behind `options`, if it is not null.
unsafe fn update(options: *mut MqHtmlOptions, f: impl FnOnce(&mut ConversionOptions)) {
    if let Some(options) = unsafe { options.as_mut() } {
        f(&mut options.0);
    }
}

/// Creates conversion options with every option off and no base URL. Free them with
/// `mq_html_options_free`.
#[unsafe(no_mangle)]
pub extern "C" fn mq_html_options_new() -> *mut MqHtmlOptions {
    Box::into_raw(Box::default())
}

/// Frees conversion options. Does nothing if `options` is null.
///
/// # Safety
///
/// `options` must be null or a pointer returned by `mq_html_options_new`, and must not be
/// used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_html_options_free(options: *mut MqHtmlOptions) {
    if !options.is_null() {
        drop(unsafe { Box::from_raw(options) });
    }
}

/// Sets whether `<script>` contents are extracted as code blocks. Does nothing if `options`
/// is null.
///
/// # Safety
///
/// `options` must be null or a pointer returned by `mq_html_options_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_html_options_set_extract_scripts_as_code_blocks(options: *mut MqHtmlOptions, value: bool) {
    unsafe { update(options, |o| o.extract_scripts_as_code_blocks = value) };
}

/// Sets whether front matter is generated from the HTML head metadata. Does nothing if
/// `options` is null.
///
/// # Safety
///
/// `options` must be null or a pointer returned by `mq_html_options_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_html_options_set_generate_front_matter(options: *mut MqHtmlOptions, value: bool) {
    unsafe { update(options, |o| o.generate_front_matter = value) };
}

/// Sets whether the HTML title becomes an H1 heading. Does nothing if `options` is null.
///
/// # Safety
///
/// `options` must be null or a pointer returned by `mq_html_options_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_html_options_set_use_title_as_h1(options: *mut MqHtmlOptions, value: bool) {
    unsafe { update(options, |o| o.use_title_as_h1 = value) };
}

/// Sets the base URL for resolving relative `href`/`src` values. The string is copied. A
/// null, empty or non-UTF-8 `base_url` clears it, so `<base href>` is used instead. Does
/// nothing if `options` is null.
///
/// # Safety
///
/// `options` must be null or a pointer returned by `mq_html_options_new`, and `base_url`
/// must be null or a null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_html_options_set_base_url(options: *mut MqHtmlOptions, base_url: *const c_char) {
    let base_url = unsafe { c_str_to_rust_str_slice(base_url) }
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    unsafe { update(options, |o| o.base_url = base_url) };
}

/// Converts HTML to Markdown like `mq_html_to_markdown`, with options from
/// `mq_html_options_new` (null for the defaults), reporting failure as a status plus an
/// `mq_error_t`.
///
/// On success returns `MQ_OK` and stores the Markdown in `*out`, to be freed with
/// `mq_free_string`. On failure stores null in `*out` and, if `err` is non-null, an error
/// in `*err`, to be freed with `mq_error_free`.
///
/// # Safety
///
/// `html_input_c` must be a null-terminated C string, `options` must be null or a pointer
/// returned by `mq_html_options_new`, `out` must be valid for writes and `err` must be null
/// or valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_html_to_markdown_ex(
    html_input_c: *const c_char,
    options: *const MqHtmlOptions,
    out: *mut *mut c_char,
    err: *mut *mut MqError,
) -> MqStatus {
    run_status(err, || {
        if out.is_null() {
            return Err(FfiError::invalid_argument("Output pointer is null"));
        }
        unsafe { *out = ptr::null_mut() };

        let options = unsafe { options.as_ref() }.map(|o| o.0.clone()).unwrap_or_default();
        let markdown = unsafe { convert(html_input_c, options) }?;
        unsafe { *out = to_c_string(markdown) };
        Ok(())
    })
}
