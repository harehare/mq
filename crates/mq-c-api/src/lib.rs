use mq_lang::{Engine, Value, Values};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use libc::c_void;
use std::str::FromStr;

// Opaque pointer type for the engine
pub type MQEngine = c_void;

#[repr(C)]
pub struct MQResult {
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
    CStr::from_ptr(s).to_str()
}


/// Creates a new mq_lang engine.
/// The caller is responsible for destroying the engine using `mq_engine_destroy`.
#[no_mangle]
pub extern "C" fn mq_engine_create() -> *mut MQEngine {
    let mut engine = Engine::default();
    engine.load_builtin_module(); // Load built-in functions, similar to Python bindings
    let boxed_engine = Box::new(engine);
    Box::into_raw(boxed_engine) as *mut MQEngine
}

/// Destroys an mq_lang engine.
#[no_mangle]
pub extern "C" fn mq_engine_destroy(engine_ptr: *mut MQEngine) {
    if engine_ptr.is_null() {
        return;
    }
    unsafe {
        let _ = Box::from_raw(engine_ptr as *mut Engine);
    }
}

/// Evaluates mq code with the given input.
/// The caller is responsible for freeing the result using `mq_free_result`.
#[no_mangle]
pub extern "C" fn mq_eval(
    engine_ptr: *mut MQEngine,
    code_c: *const c_char,
    input_c: *const c_char,
    input_format_c: *const c_char, // "markdown" or "text"
) -> MQResult {
    if engine_ptr.is_null() {
        return MQResult {
            values: ptr::null_mut(),
            values_len: 0,
            error_msg: to_c_string("Engine pointer is null".to_string()),
        };
    }
    let engine = unsafe { &mut *(engine_ptr as *mut Engine) };

    let code = match unsafe { c_str_to_rust_str_slice(code_c) } {
        Ok(s) => s,
        Err(_) => {
            return MQResult {
                values: ptr::null_mut(),
                values_len: 0,
                error_msg: to_c_string("Invalid UTF-8 sequence in code".to_string()),
            };
        }
    };

    let input_str = match unsafe { c_str_to_rust_str_slice(input_c) } {
        Ok(s) => s,
        Err(_) => {
            return MQResult {
                values: ptr::null_mut(),
                values_len: 0,
                error_msg: to_c_string("Invalid UTF-8 sequence in input".to_string()),
            };
        }
    };

    let input_format_str = match unsafe { c_str_to_rust_str_slice(input_format_c) } {
        Ok(s) => s.to_lowercase(),
        Err(_) => {
             return MQResult {
                values: ptr::null_mut(),
                values_len: 0,
                error_msg: to_c_string("Invalid UTF-8 sequence in input_format".to_string()),
            };
        }
    };

    let mq_input_values: Vec<Value> = match input_format_str.as_str() {
        "text" => input_str.lines().map(Value::from).collect::<Vec<_>>(),
        "markdown" => {
            match mq_markdown::Markdown::from_str(input_str) {
                Ok(md) => md.nodes.into_iter().map(Value::from).collect::<Vec<_>>(),
                Err(e) => {
                    return MQResult {
                        values: ptr::null_mut(),
                        values_len: 0,
                        error_msg: to_c_string(format!("Markdown parsing error: {}", e)),
                    };
                }
            }
        }
        _ => {
            return MQResult {
                values: ptr::null_mut(),
                values_len: 0,
                error_msg: to_c_string(format!("Unsupported input format: {}", input_format_str)),
            };
        }
    };

    match engine.eval(code, mq_input_values.into_iter()) {
        Ok(result_values) => {
            let mut c_values: Vec<*mut c_char> = Vec::new();
            for value in result_values {
                // For now, convert all values to their string representation.
                // More sophisticated type handling could be added later.
                c_values.push(to_c_string(value.to_string()));
            }
            let len = c_values.len();
            let ptr = c_values.as_mut_ptr();
            std::mem::forget(c_values); // Prevent Rust from freeing the Vec's memory

            MQResult {
                values: ptr,
                values_len: len,
                error_msg: ptr::null_mut(),
            }
        }
        Err(e) => MQResult {
            values: ptr::null_mut(),
            values_len: 0,
            error_msg: to_c_string(format!("Error evaluating query: {}", e)),
        },
    }
}

/// Frees a C string allocated by Rust.
#[no_mangle]
pub extern "C" fn mq_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(s);
    }
}

/// Frees the MQResult structure including its contents.
#[no_mangle]
pub extern "C" fn mq_free_result(result: MQResult) {
    if !result.error_msg.is_null() {
        mq_free_string(result.error_msg);
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
