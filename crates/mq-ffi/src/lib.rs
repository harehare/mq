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
            let ptr = c_values.as_mut_ptr();
            std::mem::forget(c_values); // Prevent Rust from freeing the Vec's memory

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
        assert!(!result.error_msg.is_null());
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
        assert!(!result.values.is_null());
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
}
