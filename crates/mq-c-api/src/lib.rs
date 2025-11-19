use libc::c_void;
use mq_lang::DefaultEngine;
use mq_lang::{Engine, RuntimeValue};
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
}
