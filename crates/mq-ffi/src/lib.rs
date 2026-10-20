//! C API for integrating mq functionality into C applications.
//!
//! This crate provides a Foreign Function Interface (FFI) wrapper around the mq language engine,
//! allowing C and C++ applications to evaluate mq queries against various input formats.
//!
//! # Features
//!
//! - Create and manage mq engine instances
//! - Evaluate mq code with markdown, MDX, HTML, or plain text input
//! - Memory-safe API with proper ownership handling
//! - Support for multiple input formats: markdown, MDX, HTML, and plain text
//!
//! # Basic Usage
//!
//! ```c
//! // Create an engine
//! MqContext* engine = mq_create();
//!
//! // Evaluate some code
//! MqResult result = mq_eval(
//!     engine,
//!     ".h",                   // code
//!     "# Hello, world!",      // input
//!     "markdown"              // input format
//! );
//!
//! // Check for errors
//! if (result.error_msg != NULL) {
//!     printf("Error: %s\n", result.error_msg);
//! } else {
//!     // Process results
//!     for (size_t i = 0; i < result.values_len; i++) {
//!         printf("%s\n", result.values[i]);
//!     }
//! }
//!
//! // Clean up
//! mq_free_result(result);
//! mq_destroy(engine);
//! ```
//!
//! # Memory Management
//!
//! The API follows these memory management rules:
//!
//! - `mq_create()` allocates an engine that must be freed with `mq_destroy()`
//! - `mq_eval()` returns an `MqResult` that must be freed with `mq_free_result()`
//! - Individual strings can be freed with `mq_free_string()` if needed
//! - Always free resources in reverse order of allocation
//!
//! # Input Formats
//!
//! Supported input formats:
//! - `"markdown"` - Standard markdown format
//! - `"mdx"` - Markdown with JSX support
//! - `"html"` - HTML content converted to markdown
//! - `"text"` - Plain text, split by lines
//!
//! # Panic Safety
//!
//! Every `extern "C"` function in this crate catches panics at its boundary
//! (via [`std::panic::catch_unwind`]) and converts them into the function's
//! normal error channel instead of letting them unwind into the calling C/C++
//! code. This only works if the library is built with an unwinding panic
//! strategy; build it with `cargo build --profile release-ffi` rather than
//! `--release` (see the crate's `README.md`), since the workspace's default
//! `release` profile sets `panic = "abort"`.
//!
use libc::c_void;
use mq_lang::DefaultEngine;
use mq_lang::{Engine, RuntimeValue};
use mq_markdown::{ConversionOptions, convert_html_to_markdown};
use std::ffi::CStr;
use std::ffi::CString;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;

/// Opaque handle to an mq engine.
pub type MqContext = c_void;

#[repr(C)]
/// Result of a query: `values` on success, `error_msg` on failure.
pub struct MqResult {
    /// Array of `values_len` result strings.
    pub values: *mut *mut c_char,
    /// Number of entries in `values`.
    pub values_len: usize,
    /// Error message, or null on success.
    pub error_msg: *mut c_char,
}

/// C-compatible conversion options for HTML to Markdown conversion.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MqConversionOptions {
    /// Extract script tags as code blocks
    pub extract_scripts_as_code_blocks: bool,
    /// Generate front matter from HTML head metadata
    pub generate_front_matter: bool,
    /// Use HTML title tag as H1 heading
    pub use_title_as_h1: bool,
    /// Base URL for resolving relative `href`/`src` values; null falls back to `<base href>`.
    pub base_url: *const c_char,
}

impl From<MqConversionOptions> for ConversionOptions {
    fn from(options: MqConversionOptions) -> Self {
        let base_url = if options.base_url.is_null() {
            None
        } else {
            unsafe { c_str_to_rust_str_slice(options.base_url) }
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        };
        ConversionOptions {
            extract_scripts_as_code_blocks: options.extract_scripts_as_code_blocks,
            generate_front_matter: options.generate_front_matter,
            use_title_as_h1: options.use_title_as_h1,
            base_url,
        }
    }
}

// Helper function to convert Rust string to C string
fn to_c_string(s: String) -> *mut c_char {
    CString::new(s).map_or_else(|_| ptr::null_mut(), |cs| cs.into_raw())
}

// Extracts a human-readable message from a caught panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

// Converts a caught panic payload into a C string error message.
fn panic_to_c_string(payload: Box<dyn std::any::Any + Send>) -> *mut c_char {
    to_c_string(format!("Internal error: {}", panic_message(&*payload)))
}

// Runs `f` at a panic boundary, discarding any panic it raises.
fn guard(f: impl FnOnce() + std::panic::UnwindSafe) {
    let _ = std::panic::catch_unwind(f);
}

// Runs `f` at a panic boundary, converting a panic into a C-string error message.
fn guard_c_string(f: impl FnOnce() -> *mut c_char + std::panic::UnwindSafe) -> *mut c_char {
    std::panic::catch_unwind(f).unwrap_or_else(panic_to_c_string)
}

// Helper function to convert C string to Rust string slice
unsafe fn c_str_to_rust_str_slice<'a>(s: *const c_char) -> Result<&'a str, std::str::Utf8Error> {
    if s.is_null() {
        // This case should ideally be handled by the caller or return an error.
        // For now, returning an empty string if null to avoid panics,
        // but robust error handling would be better.
        return Ok("");
    }
    unsafe { CStr::from_ptr(s).to_str() }
}

// Helper function to convert a C array of C strings into a Vec<String>.
// Invalid UTF-8 entries are skipped rather than aborting the whole conversion.
unsafe fn c_str_array_to_strings(items: *const *const c_char, items_len: usize) -> Vec<String> {
    if items.is_null() || items_len == 0 {
        return Vec::new();
    }

    let item_ptrs = unsafe { std::slice::from_raw_parts(items, items_len) };
    item_ptrs
        .iter()
        .filter_map(|&p| unsafe { c_str_to_rust_str_slice(p) }.ok())
        .map(|s| s.to_string())
        .collect()
}

/// Creates a new mq_lang engine.
/// The caller is responsible for destroying the engine using `mq_destroy`.
#[unsafe(no_mangle)]
pub extern "C" fn mq_create() -> *mut MqContext {
    std::panic::catch_unwind(|| {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        let boxed_engine = Box::new(engine);
        Box::into_raw(boxed_engine) as *mut MqContext
    })
    .unwrap_or(ptr::null_mut())
}

/// Destroys an mq_lang engine.
#[unsafe(no_mangle)]
pub extern "C" fn mq_destroy(engine_ptr: *mut MqContext) {
    guard(move || {
        if engine_ptr.is_null() {
            return;
        }
        unsafe {
            let _ = Box::from_raw(engine_ptr as *mut Engine);
        }
    });
}

/// Evaluates mq code with the given input.
/// The caller is responsible for freeing the result using `mq_free_result`.
///
/// # Safety
///
/// This function is unsafe because it dereferences raw pointers. The caller must ensure:
/// - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`
/// - `code_c` must be a valid pointer to a null-terminated C string
/// - `input_c` must be a valid pointer to a null-terminated C string
/// - `input_format_c` must be a valid pointer to a null-terminated C string
/// - All string pointers must remain valid for the duration of this function call
/// - The returned `MqResult` must be freed using `mq_free_result` to avoid memory leaks
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_eval(
    engine_ptr: *mut MqContext,
    code_c: *const c_char,
    input_c: *const c_char,
    input_format_c: *const c_char, // "markdown" or "mdx" or "text"
) -> MqResult {
    std::panic::catch_unwind(move || unsafe { mq_eval_inner(engine_ptr, code_c, input_c, input_format_c) })
        .unwrap_or_else(|payload| MqResult {
            values: ptr::null_mut(),
            values_len: 0,
            error_msg: panic_to_c_string(payload),
        })
}

unsafe fn mq_eval_inner(
    engine_ptr: *mut MqContext,
    code_c: *const c_char,
    input_c: *const c_char,
    input_format_c: *const c_char,
) -> MqResult {
    if engine_ptr.is_null() {
        return MqResult {
            values: ptr::null_mut(),
            values_len: 0,
            error_msg: to_c_string("Engine pointer is null".to_string()),
        };
    }
    let engine = unsafe { &mut *(engine_ptr as *mut Engine) };

    let code = match unsafe { c_str_to_rust_str_slice(code_c) } {
        Ok(s) => s,
        Err(_) => {
            return MqResult {
                values: ptr::null_mut(),
                values_len: 0,
                error_msg: to_c_string("Invalid UTF-8 sequence in code".to_string()),
            };
        }
    };

    if input_c.is_null() {
        return MqResult {
            values: ptr::null_mut(),
            values_len: 0,
            error_msg: to_c_string("Input pointer is null".to_string()),
        };
    }

    let input_str = match unsafe { c_str_to_rust_str_slice(input_c) } {
        Ok(s) => s,
        Err(_) => {
            return MqResult {
                values: ptr::null_mut(),
                values_len: 0,
                error_msg: to_c_string("Invalid UTF-8 sequence in input".to_string()),
            };
        }
    };

    if input_format_c.is_null() {
        return MqResult {
            values: ptr::null_mut(),
            values_len: 0,
            error_msg: to_c_string("Input format pointer is null".to_string()),
        };
    }

    let input_format_str = match unsafe { c_str_to_rust_str_slice(input_format_c) } {
        Ok(s) => s.to_lowercase(),
        Err(_) => {
            return MqResult {
                values: ptr::null_mut(),
                values_len: 0,
                error_msg: to_c_string("Invalid UTF-8 sequence in input_format".to_string()),
            };
        }
    };

    let mq_input_values: Vec<RuntimeValue> = match input_format_str.as_str() {
        "text" => mq_lang::parse_text_input(input_str).unwrap(),
        "markdown" => match mq_lang::parse_markdown_input(input_str) {
            Ok(v) => v,
            Err(e) => {
                return MqResult {
                    values: ptr::null_mut(),
                    values_len: 0,
                    error_msg: to_c_string(format!("Markdown parsing error: {}", e)),
                };
            }
        },
        "mdx" => match mq_lang::parse_mdx_input(input_str) {
            Ok(v) => v,
            Err(e) => {
                return MqResult {
                    values: ptr::null_mut(),
                    values_len: 0,
                    error_msg: to_c_string(format!("Markdown parsing error: {}", e)),
                };
            }
        },
        "html" => match mq_lang::parse_html_input(input_str) {
            Ok(v) => v,
            Err(e) => {
                return MqResult {
                    values: ptr::null_mut(),
                    values_len: 0,
                    error_msg: to_c_string(format!("Html parsing error: {}", e)),
                };
            }
        },
        _ => {
            return MqResult {
                values: ptr::null_mut(),
                values_len: 0,
                error_msg: to_c_string(format!("Unsupported input format: {}", input_format_str)),
            };
        }
    };

    match engine.eval(code, mq_input_values.into_iter()) {
        Ok(result_values) => {
            let mut c_values: Vec<*mut c_char> = Vec::new();
            let values_len = result_values.len();

            for value in result_values {
                // For now, convert all values to their string representation.
                // More sophisticated type handling could be added later.
                c_values.push(to_c_string(value.to_string()));
            }

            let ptr = if c_values.is_empty() {
                ptr::null_mut()
            } else {
                let p = c_values.as_mut_ptr();
                std::mem::forget(c_values); // Prevent Rust from freeing the Vec's memory
                p
            };

            MqResult {
                values: ptr,
                values_len,
                error_msg: ptr::null_mut(),
            }
        }
        Err(e) => MqResult {
            values: ptr::null_mut(),
            values_len: 0,
            error_msg: to_c_string(format!("Error evaluating query: {}", e)),
        },
    }
}

/// Frees a C string allocated by Rust.
///
/// # Safety
///
/// This function is unsafe because it dereferences a raw pointer. The caller must ensure:
/// - `s` must be a valid pointer to a C string previously allocated by Rust using `CString::into_raw()`
/// - `s` must not be used after calling this function (use-after-free protection)
/// - This function must only be called once per pointer (double-free protection)
/// - If `s` is null, the function safely returns without performing any operations
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_free_string(s: *mut c_char) {
    guard(move || {
        if s.is_null() {
            return;
        }

        unsafe {
            let _ = CString::from_raw(s);
        }
    });
}

/// Frees the MqResult structure including its contents.
#[unsafe(no_mangle)]
pub extern "C" fn mq_free_result(result: MqResult) {
    guard(move || {
        if !result.error_msg.is_null() {
            unsafe {
                mq_free_string(result.error_msg);
            }
        }

        if !result.values.is_null() {
            unsafe {
                // Reconstruct the Vec from the raw parts to properly deallocate it
                // along with its elements.
                let values_vec = Vec::from_raw_parts(result.values, result.values_len, result.values_len);
                for value_ptr in values_vec {
                    if !value_ptr.is_null() {
                        // This was already a CString, so free it with mq_free_string
                        mq_free_string(value_ptr);
                    }
                }
                // The Vec itself is dropped here, freeing the memory it owned for the pointers.
            }
        }
    });
}

/// Converts HTML to Markdown with the given conversion options.
/// Returns a C string containing the markdown output, or NULL on error.
/// The caller is responsible for freeing the result using `mq_free_string`.
///
/// # Safety
///
/// This function is unsafe because it dereferences raw pointers. The caller must ensure:
/// - `html_input_c` must be a valid pointer to a null-terminated C string
/// - `error_msg` must be a valid pointer to a location where an error message pointer can be stored
/// - The string pointer must remain valid for the duration of this function call
/// - The returned C string must be freed using `mq_free_string` to avoid memory leaks
/// - If an error occurs, the function returns NULL and sets `*error_msg` to an error message
///
/// # Example
///
/// ```c
/// char* error_msg = NULL;
/// MqConversionOptions options = {
///     .extract_scripts_as_code_blocks = false,
///     .generate_front_matter = true,
///     .use_title_as_h1 = true
/// };
///
/// char* markdown = mq_html_to_markdown(
///     "<html><head><title>Hello</title></head><body><p>World</p></body></html>",
///     options,
///     &error_msg
/// );
///
/// if (markdown == NULL) {
///     printf("Error: %s\n", error_msg);
///     mq_free_string(error_msg);
/// } else {
///     printf("%s\n", markdown);
///     mq_free_string(markdown);
/// }
/// ```
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_html_to_markdown(
    html_input_c: *const c_char,
    options: MqConversionOptions,
    error_msg: *mut *mut c_char,
) -> *mut c_char {
    std::panic::catch_unwind(move || unsafe { mq_html_to_markdown_inner(html_input_c, options, error_msg) })
        .unwrap_or_else(|payload| {
            if !error_msg.is_null() {
                unsafe {
                    *error_msg = panic_to_c_string(payload);
                }
            }
            ptr::null_mut()
        })
}

unsafe fn mq_html_to_markdown_inner(
    html_input_c: *const c_char,
    options: MqConversionOptions,
    error_msg: *mut *mut c_char,
) -> *mut c_char {
    // Initialize error_msg to NULL
    if !error_msg.is_null() {
        unsafe {
            *error_msg = ptr::null_mut();
        }
    }

    // Validate input pointer
    if html_input_c.is_null() {
        if !error_msg.is_null() {
            unsafe {
                *error_msg = to_c_string("HTML input pointer is null".to_string());
            }
        }
        return ptr::null_mut();
    }

    // Convert C string to Rust string
    let html_input_str = match unsafe { c_str_to_rust_str_slice(html_input_c) } {
        Ok(s) => s,
        Err(_) => {
            if !error_msg.is_null() {
                unsafe {
                    *error_msg = to_c_string("Invalid UTF-8 sequence in HTML input".to_string());
                }
            }
            return ptr::null_mut();
        }
    };

    // Convert options and call the conversion function
    let rust_options: ConversionOptions = options.into();
    match convert_html_to_markdown(html_input_str, rust_options) {
        Ok(markdown) => to_c_string(markdown),
        Err(e) => {
            if !error_msg.is_null() {
                unsafe {
                    *error_msg = to_c_string(format!("HTML to Markdown conversion error: {}", e));
                }
            }
            ptr::null_mut()
        }
    }
}
/// Returns the mq-ffi library version as a static, null-terminated string.
#[unsafe(no_mangle)]
pub extern "C" fn mq_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Sets the maximum call stack depth for function calls, to guard against
/// runaway recursion in untrusted mq code. Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub extern "C" fn mq_set_max_call_stack_depth(engine_ptr: *mut MqContext, max_call_stack_depth: u32) {
    guard(move || {
        if engine_ptr.is_null() {
            return;
        }
        let engine = unsafe { &mut *(engine_ptr as *mut Engine) };
        engine.set_max_call_stack_depth(max_call_stack_depth);
    });
}

/// Sets the search paths used to resolve modules loaded via `mq_import_module`
/// or `mq_load_module`. Has no effect if `engine_ptr` is null.
///
/// # Safety
///
/// This function is unsafe because it dereferences raw pointers. The caller must ensure:
/// - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
/// - `paths` must be a valid pointer to an array of `paths_len` null-terminated C strings, or null if `paths_len` is 0
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_set_search_paths(
    engine_ptr: *mut MqContext,
    paths: *const *const c_char,
    paths_len: usize,
) {
    guard(move || {
        if engine_ptr.is_null() {
            return;
        }
        let engine = unsafe { &mut *(engine_ptr as *mut Engine) };
        let search_paths = unsafe { c_str_array_to_strings(paths, paths_len) }
            .into_iter()
            .map(PathBuf::from)
            .collect();
        engine.set_search_paths(search_paths);
    });
}

/// Defines a string variable that can be referenced from mq code evaluated
/// afterwards by `mq_eval`, allowing values from the host environment to be
/// injected without building query strings by hand.
/// Has no effect if `engine_ptr` is null.
///
/// # Safety
///
/// This function is unsafe because it dereferences raw pointers. The caller must ensure:
/// - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
/// - `name_c` must be a valid pointer to a null-terminated C string
/// - `value_c` must be a valid pointer to a null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_define_string_value(
    engine_ptr: *mut MqContext,
    name_c: *const c_char,
    value_c: *const c_char,
) {
    guard(move || {
        if engine_ptr.is_null() {
            return;
        }
        let engine = unsafe { &mut *(engine_ptr as *mut Engine) };

        let name = match unsafe { c_str_to_rust_str_slice(name_c) } {
            Ok(s) => s,
            Err(_) => return,
        };
        let value = match unsafe { c_str_to_rust_str_slice(value_c) } {
            Ok(s) => s,
            Err(_) => return,
        };

        engine.define_string_value(name, value);
    });
}

/// Imports an external module by name, searched for in the paths configured via
/// `mq_set_search_paths`, making its exported definitions available to subsequent
/// `mq_eval` calls on the same engine.
///
/// # Safety
///
/// This function is unsafe because it dereferences raw pointers. The caller must ensure:
/// - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
/// - `module_name_c` must be a valid pointer to a null-terminated C string
/// - The returned pointer, if non-null, must be freed with `mq_free_string`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_import_module(engine_ptr: *mut MqContext, module_name_c: *const c_char) -> *mut c_char {
    guard_c_string(move || {
        if engine_ptr.is_null() {
            return to_c_string("Engine pointer is null".to_string());
        }
        let engine = unsafe { &mut *(engine_ptr as *mut Engine) };

        let module_name = match unsafe { c_str_to_rust_str_slice(module_name_c) } {
            Ok(s) => s,
            Err(_) => return to_c_string("Invalid UTF-8 sequence in module_name".to_string()),
        };

        match engine.import_module(module_name) {
            Ok(()) => ptr::null_mut(),
            Err(e) => to_c_string(format!("Error importing module: {}", e)),
        }
    })
}

/// Loads an external module by name, searched for in the paths configured via
/// `mq_set_search_paths`, making its exported definitions available to subsequent
/// `mq_eval` calls on the same engine.
///
/// # Safety
///
/// This function is unsafe because it dereferences raw pointers. The caller must ensure:
/// - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
/// - `module_name_c` must be a valid pointer to a null-terminated C string
/// - The returned pointer, if non-null, must be freed with `mq_free_string`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_load_module(engine_ptr: *mut MqContext, module_name_c: *const c_char) -> *mut c_char {
    guard_c_string(move || {
        if engine_ptr.is_null() {
            return to_c_string("Engine pointer is null".to_string());
        }
        let engine = unsafe { &mut *(engine_ptr as *mut Engine) };

        let module_name = match unsafe { c_str_to_rust_str_slice(module_name_c) } {
            Ok(s) => s,
            Err(_) => return to_c_string("Invalid UTF-8 sequence in module_name".to_string()),
        };

        match engine.load_module(module_name) {
            Ok(()) => ptr::null_mut(),
            Err(e) => to_c_string(format!("Error loading module: {}", e)),
        }
    })
}

/// Replaces the HTTP resolver's domain allowlist used when importing modules
/// over HTTP(S) via `mq_import_module` / `mq_load_module`. An empty list restricts
/// access to the built-in default domain only; it does not open up all URLs.
/// Has no effect if `engine_ptr` is null.
///
/// # Safety
///
/// This function is unsafe because it dereferences raw pointers. The caller must ensure:
/// - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
/// - `domains` must be a valid pointer to an array of `domains_len` null-terminated C strings, or null if `domains_len` is 0
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_set_http_allowed_domains(
    engine_ptr: *mut MqContext,
    domains: *const *const c_char,
    domains_len: usize,
) {
    guard(move || {
        #[cfg(feature = "http-import")]
        {
            if engine_ptr.is_null() {
                return;
            }
            let engine = unsafe { &mut *(engine_ptr as *mut Engine) };
            let domains = unsafe { c_str_array_to_strings(domains, domains_len) };
            engine.set_http_allowed_domains(domains);
        }
        #[cfg(not(feature = "http-import"))]
        {
            let _ = (engine_ptr, domains, domains_len);
        }
    });
}

/// Clears locally-cached HTTP module files, forcing a re-fetch of all cached
/// modules on the next import. Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub extern "C" fn mq_clear_http_cache(engine_ptr: *mut MqContext) -> *mut c_char {
    guard_c_string(move || {
        #[cfg(feature = "http-import")]
        {
            if engine_ptr.is_null() {
                return ptr::null_mut();
            }
            let engine = unsafe { &mut *(engine_ptr as *mut Engine) };
            match engine.clear_http_cache() {
                Ok(()) => ptr::null_mut(),
                Err(e) => to_c_string(format!("Error clearing HTTP cache: {}", e)),
            }
        }
        #[cfg(not(feature = "http-import"))]
        {
            let _ = engine_ptr;
            to_c_string("This library was built without the http-import feature".to_string())
        }
    })
}

/// Clears all HTTP module cache including versioned modules and lock files.
/// Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub extern "C" fn mq_clear_http_cache_all(engine_ptr: *mut MqContext) -> *mut c_char {
    guard_c_string(move || {
        #[cfg(feature = "http-import")]
        {
            if engine_ptr.is_null() {
                return ptr::null_mut();
            }
            let engine = unsafe { &mut *(engine_ptr as *mut Engine) };
            match engine.clear_http_cache_all() {
                Ok(()) => ptr::null_mut(),
                Err(e) => to_c_string(format!("Error clearing HTTP cache: {}", e)),
            }
        }
        #[cfg(not(feature = "http-import"))]
        {
            let _ = engine_ptr;
            to_c_string("This library was built without the http-import feature".to_string())
        }
    })
}

#[cfg(test)]
mod tests;
