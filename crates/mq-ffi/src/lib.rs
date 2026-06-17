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
use libc::c_void;
use mq_lang::DefaultEngine;
use mq_lang::{Engine, RuntimeValue};
use mq_markdown::{ConversionOptions, convert_html_to_markdown};
use std::ffi::CStr;
use std::ffi::CString;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;

pub type MqContext = c_void;

#[repr(C)]
pub struct MqResult {
    pub values: *mut *mut c_char,
    pub values_len: usize,
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
}

impl From<MqConversionOptions> for ConversionOptions {
    fn from(options: MqConversionOptions) -> Self {
        ConversionOptions {
            extract_scripts_as_code_blocks: options.extract_scripts_as_code_blocks,
            generate_front_matter: options.generate_front_matter,
            use_title_as_h1: options.use_title_as_h1,
        }
    }
}

// Helper function to convert Rust string to C string
fn to_c_string(s: String) -> *mut c_char {
    CString::new(s).map_or_else(|_| ptr::null_mut(), |cs| cs.into_raw())
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
    let mut engine = DefaultEngine::default();
    engine.load_builtin_module();
    let boxed_engine = Box::new(engine);
    Box::into_raw(boxed_engine) as *mut MqContext
}

/// Destroys an mq_lang engine.
#[unsafe(no_mangle)]
pub extern "C" fn mq_destroy(engine_ptr: *mut MqContext) {
    if engine_ptr.is_null() {
        return;
    }
    unsafe {
        let _ = Box::from_raw(engine_ptr as *mut Engine);
    }
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
    if s.is_null() {
        return;
    }

    unsafe {
        let _ = CString::from_raw(s);
    }
}

/// Frees the MqResult structure including its contents.
#[unsafe(no_mangle)]
pub extern "C" fn mq_free_result(result: MqResult) {
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

/// C-compatible optimization level for AST transformations applied before evaluation.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum MqOptimizationLevel {
    None = 0,
    Basic = 1,
    Full = 2,
}

impl From<MqOptimizationLevel> for mq_lang::OptimizationLevel {
    fn from(level: MqOptimizationLevel) -> Self {
        match level {
            MqOptimizationLevel::None => mq_lang::OptimizationLevel::None,
            MqOptimizationLevel::Basic => mq_lang::OptimizationLevel::Basic,
            MqOptimizationLevel::Full => mq_lang::OptimizationLevel::Full,
        }
    }
}

/// Sets the optimization level for AST transformations applied before evaluation.
/// Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub extern "C" fn mq_set_optimization_level(engine_ptr: *mut MqContext, level: MqOptimizationLevel) {
    if engine_ptr.is_null() {
        return;
    }
    let engine = unsafe { &mut *(engine_ptr as *mut Engine) };
    engine.set_optimization_level(level.into());
}

/// Sets the maximum call stack depth for function calls, to guard against
/// runaway recursion in untrusted mq code. Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub extern "C" fn mq_set_max_call_stack_depth(engine_ptr: *mut MqContext, max_call_stack_depth: u32) {
    if engine_ptr.is_null() {
        return;
    }
    let engine = unsafe { &mut *(engine_ptr as *mut Engine) };
    engine.set_max_call_stack_depth(max_call_stack_depth);
}

/// Sets the search paths used to resolve modules loaded via `mq_import_module`
/// or `mq_load_module`. Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_set_search_paths(
    engine_ptr: *mut MqContext,
    paths: *const *const c_char,
    paths_len: usize,
) {
    if engine_ptr.is_null() {
        return;
    }
    let engine = unsafe { &mut *(engine_ptr as *mut Engine) };
    let search_paths = unsafe { c_str_array_to_strings(paths, paths_len) }
        .into_iter()
        .map(PathBuf::from)
        .collect();
    engine.set_search_paths(search_paths);
}

/// Defines a string variable that can be referenced from mq code evaluated
/// afterwards by `mq_eval`, allowing values from the host environment to be
/// injected without building query strings by hand.
/// Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_define_string_value(
    engine_ptr: *mut MqContext,
    name_c: *const c_char,
    value_c: *const c_char,
) {
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
}

/// Imports an external module by name, searched for in the paths configured via
/// `mq_set_search_paths`, making its exported definitions available to subsequent
/// `mq_eval` calls on the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_import_module(engine_ptr: *mut MqContext, module_name_c: *const c_char) -> *mut c_char {
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
}

/// Loads an external module by name, searched for in the paths configured via
/// `mq_set_search_paths`, making its exported definitions available to subsequent
/// `mq_eval` calls on the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_load_module(engine_ptr: *mut MqContext, module_name_c: *const c_char) -> *mut c_char {
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
}

/// Replaces the HTTP resolver's domain allowlist used when importing modules
/// over HTTP(S) via `mq_import_module` / `mq_load_module`. An empty list restricts
/// access to the built-in default domain only; it does not open up all URLs.
/// Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_set_http_allowed_domains(
    engine_ptr: *mut MqContext,
    domains: *const *const c_char,
    domains_len: usize,
) {
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
}

/// Clears locally-cached HTTP module files, forcing a re-fetch of all cached
/// modules on the next import. Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub extern "C" fn mq_clear_http_cache(engine_ptr: *mut MqContext) -> *mut c_char {
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
}

/// Clears all HTTP module cache including versioned modules and lock files.
/// Has no effect if `engine_ptr` is null.
#[unsafe(no_mangle)]
pub extern "C" fn mq_clear_http_cache_all(engine_ptr: *mut MqContext) -> *mut c_char {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a C string for testing
    fn make_c_string(s: &str) -> *const c_char {
        CString::new(s).unwrap().into_raw()
    }

    // Helper function to convert C string back to Rust string for assertions
    unsafe fn c_string_to_rust_string(ptr: *mut c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let c_str = unsafe { CStr::from_ptr(ptr) };
        c_str.to_string_lossy().into_owned()
    }

    #[test]
    fn test_engine_create_and_destroy() {
        // Test engine creation
        let engine = mq_create();
        assert!(!engine.is_null());

        // Test engine destruction
        mq_destroy(engine);

        // Test destroying null engine (should not crash)
        mq_destroy(ptr::null_mut());
    }

    #[test]
    fn test_eval_with_null_engine() {
        let code = make_c_string(".h");
        let input = make_c_string("test");
        let format = make_c_string("text");

        let result = unsafe { mq_eval(ptr::null_mut(), code, input, format) };

        assert!(result.values.is_null());
        assert_eq!(result.values_len, 0);
        assert!(!result.error_msg.is_null());

        let error_msg = unsafe { c_string_to_rust_string(result.error_msg) };
        assert_eq!(error_msg, "Engine pointer is null");

        mq_free_result(result);

        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_eval_with_text_input() {
        let engine = mq_create();
        let code = make_c_string(r#"select(contains("line"))"#);
        let input = make_c_string("# line1\n## line2\n### line3");
        let format = make_c_string("text");
        let result = unsafe { mq_eval(engine, code, input, format) };
        assert!(result.error_msg.is_null());
        assert!(!result.values.is_null());
        assert_eq!(result.values_len, 3);

        // Verify the values
        unsafe {
            let values_slice = std::slice::from_raw_parts(result.values, result.values_len);
            let value1 = c_string_to_rust_string(values_slice[0]);
            let value2 = c_string_to_rust_string(values_slice[1]);
            let value3 = c_string_to_rust_string(values_slice[2]);

            assert_eq!(value1, "# line1");
            assert_eq!(value2, "## line2");
            assert_eq!(value3, "### line3");
        }

        mq_free_result(result);
        mq_destroy(engine);

        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_eval_with_markdown_input() {
        let engine = mq_create();
        let code = make_c_string(".h");
        let input = make_c_string("# Header\n\nSome text\n\n## Subheader");
        let format = make_c_string("markdown");

        let result = unsafe { mq_eval(engine, code, input, format) };

        assert!(result.error_msg.is_null());
        assert!(!result.values.is_null());
        assert!(result.values_len > 0);

        mq_free_result(result);
        mq_destroy(engine);

        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_eval_with_invalid_code() {
        let engine = mq_create();
        let code = make_c_string("invalid_function()");
        let input = make_c_string("test");
        let format = make_c_string("text");

        let result = unsafe { mq_eval(engine, code, input, format) };

        assert!(result.values.is_null());
        assert_eq!(result.values_len, 0);
        assert!(!result.error_msg.is_null());

        let error_msg = unsafe { c_string_to_rust_string(result.error_msg) };
        assert!(error_msg.contains("Error evaluating query"));

        mq_free_result(result);
        mq_destroy(engine);

        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_eval_with_unsupported_format() {
        let engine = mq_create();
        let code = make_c_string(".h");
        let input = make_c_string("test");
        let format = make_c_string("json");

        let result = unsafe { mq_eval(engine, code, input, format) };

        assert!(result.values.is_null());
        assert_eq!(result.values_len, 0);
        assert!(!result.error_msg.is_null());

        let error_msg = unsafe { c_string_to_rust_string(result.error_msg) };
        assert!(error_msg.contains("Unsupported input format: json"));

        mq_free_result(result);
        mq_destroy(engine);

        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_eval_with_null_parameters() {
        let engine = mq_create();

        // Test with null code
        let result = unsafe { mq_eval(engine, ptr::null(), make_c_string("test"), make_c_string("text")) };
        assert!(result.error_msg.is_null());
        mq_free_result(result);

        // Test with null input
        let result = unsafe { mq_eval(engine, make_c_string(".a"), ptr::null(), make_c_string("text")) };
        assert!(!result.error_msg.is_null());
        mq_free_result(result);

        // Test with null format
        let result = unsafe { mq_eval(engine, make_c_string(".a"), make_c_string("test"), ptr::null()) };
        assert!(!result.error_msg.is_null());
        mq_free_result(result);

        mq_destroy(engine);
    }

    #[test]
    fn test_format_case_insensitive() {
        let engine = mq_create();
        let code = make_c_string(".h");
        let input = make_c_string("test");

        // Test uppercase format
        let format_upper = make_c_string("TEXT");
        let result = unsafe { mq_eval(engine, code, input, format_upper) };
        assert!(result.error_msg.is_null());
        mq_free_result(result);

        // Test mixed case format
        let format_mixed = make_c_string("MarkDown");
        let input_md = make_c_string("# Test");
        let result = unsafe { mq_eval(engine, code, input_md, format_mixed) };
        assert!(result.error_msg.is_null());
        mq_free_result(result);

        mq_destroy(engine);
        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(input_md as *mut c_char);
            mq_free_string(format_upper as *mut c_char);
            mq_free_string(format_mixed as *mut c_char);
        }
    }

    #[test]
    fn test_free_functions() {
        unsafe {
            // Test freeing null string (should not crash)
            mq_free_string(ptr::null_mut());

            // Test freeing valid string
            let test_string = to_c_string("test".to_string());
            mq_free_string(test_string);
        }

        // Test freeing empty result
        let empty_result = MqResult {
            values: ptr::null_mut(),
            values_len: 0,
            error_msg: ptr::null_mut(),
        };
        mq_free_result(empty_result);
    }

    #[test]
    fn test_empty_input() {
        let engine = mq_create();
        let code = make_c_string(".h");
        let input = make_c_string("");
        let format = make_c_string("text");

        let result = unsafe { mq_eval(engine, code, input, format) };

        assert!(result.error_msg.is_null());
        assert!(result.values.is_null());
        assert_eq!(result.values_len, 0);

        mq_free_result(result);
        mq_destroy(engine);

        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_filter_query() {
        let engine = mq_create();
        let code = make_c_string("select(gt(len(), 5))");
        let input = make_c_string("short\nthis is a longer line\ntest\nanother long line here");
        let format = make_c_string("text");

        let result = unsafe { mq_eval(engine, code, input, format) };

        assert!(result.error_msg.is_null());
        assert!(!result.values.is_null());
        assert_eq!(result.values_len, 4); // Only lines longer than 5 characters

        mq_free_result(result);
        mq_destroy(engine);

        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_map_query() {
        let engine = mq_create();
        let code = make_c_string("len()");
        let input = make_c_string("a\nbb\nccc");
        let format = make_c_string("text");

        let result = unsafe { mq_eval(engine, code, input, format) };

        assert!(result.error_msg.is_null());
        assert!(!result.values.is_null());
        assert_eq!(result.values_len, 3);

        unsafe {
            let values_slice = std::slice::from_raw_parts(result.values, result.values_len);
            let len1 = c_string_to_rust_string(values_slice[0]);
            let len2 = c_string_to_rust_string(values_slice[1]);
            let len3 = c_string_to_rust_string(values_slice[2]);

            assert_eq!(len1, "1");
            assert_eq!(len2, "2");
            assert_eq!(len3, "3");
        }

        mq_free_result(result);
        mq_destroy(engine);

        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_c_str_to_rust_str_slice() {
        // Test with valid C string
        let c_string = CString::new("test").unwrap();
        let c_ptr = c_string.as_ptr();
        let result = unsafe { c_str_to_rust_str_slice(c_ptr) };
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test");

        // Test with null pointer
        let result = unsafe { c_str_to_rust_str_slice(ptr::null()) };
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_to_c_string() {
        // Test with valid string
        let rust_string = "test".to_string();
        let c_ptr = to_c_string(rust_string);
        assert!(!c_ptr.is_null());

        let result = unsafe { CStr::from_ptr(c_ptr).to_str().unwrap() };
        assert_eq!(result, "test");

        unsafe {
            mq_free_string(c_ptr);
        }

        // Test with string containing null bytes (should return null)
        let invalid_string = "test\0with\0nulls".to_string();
        let c_ptr = to_c_string(invalid_string);
        assert!(c_ptr.is_null());
    }

    #[test]
    fn test_html_to_markdown_simple() {
        let html = make_c_string("<p>Hello, World!</p>");
        let options = MqConversionOptions::default();
        let mut error_msg: *mut c_char = ptr::null_mut();

        let result = unsafe { mq_html_to_markdown(html, options, &mut error_msg) };

        assert!(error_msg.is_null());
        assert!(!result.is_null());

        let markdown_str = unsafe { c_string_to_rust_string(result) };
        assert_eq!(markdown_str.trim(), "Hello, World!");

        unsafe {
            mq_free_string(result);
            mq_free_string(html as *mut c_char);
        }
    }

    #[test]
    fn test_html_to_markdown_with_options() {
        let html = make_c_string("<html><head><title>Test Page</title></head><body><p>Content</p></body></html>");
        let options = MqConversionOptions {
            extract_scripts_as_code_blocks: false,
            generate_front_matter: true,
            use_title_as_h1: true,
        };
        let mut error_msg: *mut c_char = ptr::null_mut();

        let result = unsafe { mq_html_to_markdown(html, options, &mut error_msg) };

        assert!(error_msg.is_null());
        assert!(!result.is_null());

        let markdown_str = unsafe { c_string_to_rust_string(result) };
        assert!(markdown_str.contains("Test Page"));
        assert!(markdown_str.contains("Content"));

        unsafe {
            mq_free_string(result);
            mq_free_string(html as *mut c_char);
        }
    }

    #[test]
    fn test_html_to_markdown_null_input() {
        let options = MqConversionOptions::default();
        let mut error_msg: *mut c_char = ptr::null_mut();

        let result = unsafe { mq_html_to_markdown(ptr::null(), options, &mut error_msg) };

        assert!(result.is_null());
        assert!(!error_msg.is_null());

        let error_str = unsafe { c_string_to_rust_string(error_msg) };
        assert_eq!(error_str, "HTML input pointer is null");

        unsafe {
            mq_free_string(error_msg);
        }
    }

    #[test]
    fn test_html_to_markdown_empty_input() {
        let html = make_c_string("");
        let options = MqConversionOptions::default();
        let mut error_msg: *mut c_char = ptr::null_mut();

        let result = unsafe { mq_html_to_markdown(html, options, &mut error_msg) };

        assert!(error_msg.is_null());
        assert!(!result.is_null());

        let markdown_str = unsafe { c_string_to_rust_string(result) };
        assert_eq!(markdown_str, "");

        unsafe {
            mq_free_string(result);
            mq_free_string(html as *mut c_char);
        }
    }

    #[test]
    fn test_html_to_markdown_complex_html() {
        let html = make_c_string(
            r#"<html>
                <body>
                    <h1>Title</h1>
                    <p>Paragraph 1</p>
                    <ul>
                        <li>Item 1</li>
                        <li>Item 2</li>
                    </ul>
                </body>
            </html>"#,
        );
        let options = MqConversionOptions::default();
        let mut error_msg: *mut c_char = ptr::null_mut();

        let result = unsafe { mq_html_to_markdown(html, options, &mut error_msg) };

        assert!(error_msg.is_null());
        assert!(!result.is_null());

        let markdown_str = unsafe { c_string_to_rust_string(result) };
        assert!(markdown_str.contains("# Title"));
        assert!(markdown_str.contains("Paragraph 1"));
        assert!(markdown_str.contains("Item 1"));
        assert!(markdown_str.contains("Item 2"));

        unsafe {
            mq_free_string(result);
            mq_free_string(html as *mut c_char);
        }
    }

    #[test]
    fn test_conversion_options_default() {
        let options = MqConversionOptions::default();
        assert!(!options.extract_scripts_as_code_blocks);
        assert!(!options.generate_front_matter);
        assert!(!options.use_title_as_h1);
    }

    #[test]
    fn test_conversion_options_into() {
        let mq_options = MqConversionOptions {
            extract_scripts_as_code_blocks: true,
            generate_front_matter: true,
            use_title_as_h1: true,
        };

        let rust_options: ConversionOptions = mq_options.into();
        assert!(rust_options.extract_scripts_as_code_blocks);
        assert!(rust_options.generate_front_matter);
        assert!(rust_options.use_title_as_h1);
    }

    #[test]
    fn test_mq_version() {
        let version_c = mq_version();
        assert!(!version_c.is_null());
        let version = unsafe { CStr::from_ptr(version_c) }.to_str().unwrap();
        assert!(!version.is_empty());
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_mq_version_is_stable_across_calls() {
        // The pointer must remain valid and comparable across multiple calls,
        // since callers are told not to free it.
        let first = unsafe { CStr::from_ptr(mq_version()) }.to_str().unwrap();
        let second = unsafe { CStr::from_ptr(mq_version()) }.to_str().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn test_optimization_level_conversion() {
        assert!(matches!(
            mq_lang::OptimizationLevel::from(MqOptimizationLevel::None),
            mq_lang::OptimizationLevel::None
        ));
        assert!(matches!(
            mq_lang::OptimizationLevel::from(MqOptimizationLevel::Basic),
            mq_lang::OptimizationLevel::Basic
        ));
        assert!(matches!(
            mq_lang::OptimizationLevel::from(MqOptimizationLevel::Full),
            mq_lang::OptimizationLevel::Full
        ));
    }

    #[test]
    fn test_set_optimization_level() {
        let engine = mq_create();
        mq_set_optimization_level(engine, MqOptimizationLevel::None);
        mq_set_optimization_level(engine, MqOptimizationLevel::Basic);
        mq_set_optimization_level(engine, MqOptimizationLevel::Full);

        // Evaluation must still succeed after switching optimization levels.
        let code = make_c_string("len()");
        let input = make_c_string("abc");
        let format = make_c_string("text");
        let result = unsafe { mq_eval(engine, code, input, format) };
        assert!(result.error_msg.is_null());
        mq_free_result(result);
        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }

        // Should not crash when the engine pointer is null.
        mq_set_optimization_level(ptr::null_mut(), MqOptimizationLevel::None);

        mq_destroy(engine);
    }

    #[test]
    fn test_set_max_call_stack_depth() {
        let engine = mq_create();
        mq_set_max_call_stack_depth(engine, 16);

        // Should not crash when the engine pointer is null.
        mq_set_max_call_stack_depth(ptr::null_mut(), 16);

        mq_destroy(engine);
    }

    #[test]
    fn test_set_max_call_stack_depth_enforced() {
        let engine = mq_create();
        mq_set_max_call_stack_depth(engine, 2);

        let code = make_c_string("def rec(): rec(); rec()");
        let input = make_c_string("test");
        let format = make_c_string("text");
        let result = unsafe { mq_eval(engine, code, input, format) };

        assert!(!result.error_msg.is_null());

        mq_free_result(result);
        mq_destroy(engine);
        unsafe {
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_define_string_value_and_use_in_eval() {
        let engine = mq_create();
        let name = make_c_string("greeting");
        let value = make_c_string("hello");

        unsafe {
            mq_define_string_value(engine, name, value);
        }

        let code = make_c_string("greeting");
        let input = make_c_string("test");
        let format = make_c_string("text");
        let result = unsafe { mq_eval(engine, code, input, format) };

        assert!(result.error_msg.is_null());
        assert_eq!(result.values_len, 1);
        unsafe {
            let values_slice = std::slice::from_raw_parts(result.values, result.values_len);
            assert_eq!(c_string_to_rust_string(values_slice[0]), "hello");
        }

        mq_free_result(result);
        mq_destroy(engine);
        unsafe {
            mq_free_string(name as *mut c_char);
            mq_free_string(value as *mut c_char);
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_load_module_with_search_paths() {
        use std::fs::File;
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let module_path = temp_dir.join("mq_ffi_test_module.mq");
        let mut file = File::create(&module_path).unwrap();
        file.write_all(b"def double(x): x * 2;").unwrap();

        let engine = mq_create();
        let temp_dir_c = make_c_string(temp_dir.to_str().unwrap());
        let paths: [*const c_char; 1] = [temp_dir_c];

        unsafe {
            mq_set_search_paths(engine, paths.as_ptr(), paths.len());
        }

        let module_name = make_c_string("mq_ffi_test_module");
        let error_msg = unsafe { mq_load_module(engine, module_name) };
        assert!(error_msg.is_null());

        std::fs::remove_file(&module_path).ok();
        mq_destroy(engine);
        unsafe {
            mq_free_string(temp_dir_c as *mut c_char);
            mq_free_string(module_name as *mut c_char);
        }
    }

    #[test]
    fn test_import_module_missing_returns_error() {
        let engine = mq_create();
        let module_name = make_c_string("nonexistent_module_for_test");

        let error_msg = unsafe { mq_import_module(engine, module_name) };
        assert!(!error_msg.is_null());

        unsafe {
            mq_free_string(error_msg);
            mq_free_string(module_name as *mut c_char);
        }
        mq_destroy(engine);
    }

    #[test]
    fn test_import_module_null_engine() {
        let module_name = make_c_string("anything");
        let error_msg = unsafe { mq_import_module(ptr::null_mut(), module_name) };
        assert!(!error_msg.is_null());

        let error_str = unsafe { c_string_to_rust_string(error_msg) };
        assert_eq!(error_str, "Engine pointer is null");

        unsafe {
            mq_free_string(error_msg);
            mq_free_string(module_name as *mut c_char);
        }
    }

    #[test]
    fn test_load_module_null_engine() {
        let module_name = make_c_string("anything");
        let error_msg = unsafe { mq_load_module(ptr::null_mut(), module_name) };
        assert!(!error_msg.is_null());

        let error_str = unsafe { c_string_to_rust_string(error_msg) };
        assert_eq!(error_str, "Engine pointer is null");

        unsafe {
            mq_free_string(error_msg);
            mq_free_string(module_name as *mut c_char);
        }
    }

    #[test]
    fn test_load_module_missing_returns_error() {
        let engine = mq_create();
        let module_name = make_c_string("nonexistent_module_for_test_load");

        let error_msg = unsafe { mq_load_module(engine, module_name) };
        assert!(!error_msg.is_null());

        unsafe {
            mq_free_string(error_msg);
            mq_free_string(module_name as *mut c_char);
        }
        mq_destroy(engine);
    }

    #[test]
    fn test_import_module_with_search_paths() {
        use std::fs::File;
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let module_path = temp_dir.join("mq_ffi_test_import_module.mq");
        let mut file = File::create(&module_path).unwrap();
        file.write_all(b"def triple(x): x * 3;").unwrap();

        let engine = mq_create();
        let temp_dir_c = make_c_string(temp_dir.to_str().unwrap());
        let paths: [*const c_char; 1] = [temp_dir_c];

        unsafe {
            mq_set_search_paths(engine, paths.as_ptr(), paths.len());
        }

        let module_name = make_c_string("mq_ffi_test_import_module");
        let error_msg = unsafe { mq_import_module(engine, module_name) };
        assert!(error_msg.is_null());

        // Imported modules are namespaced, unlike `mq_load_module` which defines
        // functions directly in the calling scope.
        let code = make_c_string("mq_ffi_test_import_module::triple(2)");
        let input = make_c_string("test");
        let format = make_c_string("text");
        let result = unsafe { mq_eval(engine, code, input, format) };
        assert!(result.error_msg.is_null());
        unsafe {
            let values_slice = std::slice::from_raw_parts(result.values, result.values_len);
            assert_eq!(c_string_to_rust_string(values_slice[0]), "6");
        }

        std::fs::remove_file(&module_path).ok();
        mq_free_result(result);
        mq_destroy(engine);
        unsafe {
            mq_free_string(temp_dir_c as *mut c_char);
            mq_free_string(module_name as *mut c_char);
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    #[test]
    fn test_set_search_paths_null_engine_does_not_crash() {
        let path = make_c_string("/tmp");
        let paths: [*const c_char; 1] = [path];
        unsafe {
            mq_set_search_paths(ptr::null_mut(), paths.as_ptr(), paths.len());
            mq_free_string(path as *mut c_char);
        }
    }

    #[test]
    fn test_set_search_paths_empty_does_not_crash() {
        let engine = mq_create();
        unsafe {
            mq_set_search_paths(engine, ptr::null(), 0);
        }
        mq_destroy(engine);
    }

    #[test]
    fn test_define_string_value_null_engine_does_not_crash() {
        let name = make_c_string("x");
        let value = make_c_string("y");
        unsafe {
            mq_define_string_value(ptr::null_mut(), name, value);
            mq_free_string(name as *mut c_char);
            mq_free_string(value as *mut c_char);
        }
    }

    #[test]
    fn test_define_string_value_invalid_utf8_does_not_crash() {
        let engine = mq_create();
        let invalid = [0x66, 0x6f, 0x80, 0x6f, 0x00]; // "fo\x80o"
        let value = make_c_string("y");

        unsafe {
            mq_define_string_value(engine, invalid.as_ptr() as *const c_char, value);
            mq_free_string(value as *mut c_char);
        }

        mq_destroy(engine);
    }

    #[test]
    fn test_define_string_value_overwrites_previous() {
        let engine = mq_create();
        let name = make_c_string("v");
        let first = make_c_string("first");
        let second = make_c_string("second");

        unsafe {
            mq_define_string_value(engine, name, first);
            mq_define_string_value(engine, name, second);
        }

        let code = make_c_string("v");
        let input = make_c_string("test");
        let format = make_c_string("text");
        let result = unsafe { mq_eval(engine, code, input, format) };
        assert!(result.error_msg.is_null());
        unsafe {
            let values_slice = std::slice::from_raw_parts(result.values, result.values_len);
            assert_eq!(c_string_to_rust_string(values_slice[0]), "second");
        }

        mq_free_result(result);
        mq_destroy(engine);
        unsafe {
            mq_free_string(name as *mut c_char);
            mq_free_string(first as *mut c_char);
            mq_free_string(second as *mut c_char);
            mq_free_string(code as *mut c_char);
            mq_free_string(input as *mut c_char);
            mq_free_string(format as *mut c_char);
        }
    }

    // These run regardless of the `http-import` feature: the symbols are always
    // exported (so linking against `mq.h` succeeds either way), but their
    // behavior differs depending on whether HTTP module support was compiled in.

    #[test]
    fn test_set_http_allowed_domains_does_not_crash() {
        let engine = mq_create();
        let domain = make_c_string("example.com");
        let domains: [*const c_char; 1] = [domain];

        unsafe {
            mq_set_http_allowed_domains(engine, domains.as_ptr(), domains.len());
            mq_set_http_allowed_domains(engine, ptr::null(), 0);
            mq_set_http_allowed_domains(ptr::null_mut(), domains.as_ptr(), domains.len());
            mq_free_string(domain as *mut c_char);
        }

        mq_destroy(engine);
    }

    #[test]
    fn test_clear_http_cache() {
        let engine = mq_create();
        let error_msg = mq_clear_http_cache(engine);

        if cfg!(feature = "http-import") {
            assert!(error_msg.is_null());
        } else {
            assert!(!error_msg.is_null());
            unsafe { mq_free_string(error_msg) };
        }

        // Should not crash when the engine pointer is null.
        let error_msg_null = mq_clear_http_cache(ptr::null_mut());
        if !error_msg_null.is_null() {
            unsafe { mq_free_string(error_msg_null) };
        }

        mq_destroy(engine);
    }

    #[test]
    fn test_clear_http_cache_all() {
        let engine = mq_create();
        let error_msg = mq_clear_http_cache_all(engine);

        if cfg!(feature = "http-import") {
            assert!(error_msg.is_null());
        } else {
            assert!(!error_msg.is_null());
            unsafe { mq_free_string(error_msg) };
        }

        // Should not crash when the engine pointer is null.
        let error_msg_null = mq_clear_http_cache_all(ptr::null_mut());
        if !error_msg_null.is_null() {
            unsafe { mq_free_string(error_msg_null) };
        }

        mq_destroy(engine);
    }
}
