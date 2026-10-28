use super::*;
use crate::error::*;
use crate::html::*;
use crate::values::*;

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
    let format = make_c_string("docx");

    let result = unsafe { mq_eval(engine, code, input, format) };

    assert!(result.values.is_null());
    assert_eq!(result.values_len, 0);
    assert!(!result.error_msg.is_null());

    let error_msg = unsafe { c_string_to_rust_string(result.error_msg) };
    assert!(error_msg.contains("Unsupported input format: docx"));

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

    // Interior NUL bytes are replaced rather than yielding a null pointer
    let c_ptr = to_c_string("test\0with\0nulls".to_string());
    assert!(!c_ptr.is_null());
    let result = unsafe { CStr::from_ptr(c_ptr).to_str().unwrap() };
    assert_eq!(result, "test\u{FFFD}with\u{FFFD}nulls");
    unsafe {
        mq_free_string(c_ptr);
    }
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
        base_url: ptr::null(),
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
        base_url: ptr::null(),
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

    // The recursive call must not be in tail position: tail calls deliberately reuse a
    // frame, while this test verifies enforcement of the configured stack-depth limit.
    let code = make_c_string("def rec(): 1 + rec(); | rec()");
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
fn test_set_timeout_ms_stops_infinite_loop() {
    let engine = mq_create();
    mq_set_timeout_ms(engine, 100);

    // Should not crash when the engine pointer is null.
    mq_set_timeout_ms(ptr::null_mut(), 100);

    let code = make_c_string("while(true): 1 end");
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
fn test_set_timeout_ms_no_timeout_disables() {
    let engine = mq_create();
    mq_set_timeout_ms(engine, 1);
    mq_set_timeout_ms(engine, MQ_NO_TIMEOUT);

    let code = make_c_string("1");
    let input = make_c_string("test");
    let format = make_c_string("text");
    let result = unsafe { mq_eval(engine, code, input, format) };

    assert!(result.error_msg.is_null());

    mq_free_result(result);
    mq_destroy(engine);
    unsafe {
        mq_free_string(code as *mut c_char);
        mq_free_string(input as *mut c_char);
        mq_free_string(format as *mut c_char);
    }
}

#[test]
fn test_eval_result_with_nul_byte_has_no_null_value() {
    let engine = mq_create();
    let code = make_c_string(r#""a\u0000b""#);
    let input = make_c_string("test");
    let format = make_c_string("text");
    let result = unsafe { mq_eval(engine, code, input, format) };

    assert!(result.error_msg.is_null());
    assert_eq!(result.values_len, 1);
    assert!(!result.values.is_null());
    let values = unsafe { std::slice::from_raw_parts(result.values, result.values_len) };
    assert!(!values[0].is_null());

    mq_free_result(result);
    mq_destroy(engine);
    unsafe {
        mq_free_string(code as *mut c_char);
        mq_free_string(input as *mut c_char);
        mq_free_string(format as *mut c_char);
    }
}

#[test]
fn test_eval_many_values_roundtrip_through_free() {
    // 5 values: a Vec would have grown to capacity 8, so len != capacity.
    let engine = mq_create();
    let code = make_c_string("select(true)");
    let input = make_c_string("a\nb\nc\nd\ne");
    let format = make_c_string("text");
    let result = unsafe { mq_eval(engine, code, input, format) };

    assert!(result.error_msg.is_null());
    assert_eq!(result.values_len, 5);

    mq_free_result(result);
    mq_destroy(engine);
    unsafe {
        mq_free_string(code as *mut c_char);
        mq_free_string(input as *mut c_char);
        mq_free_string(format as *mut c_char);
    }
}

#[test]
fn test_is_panic_safe_matches_build_config() {
    assert_eq!(mq_is_panic_safe(), cfg!(panic = "unwind"));
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

#[test]
fn test_panic_message_str_payload() {
    let payload = std::panic::catch_unwind(|| panic!("boom")).unwrap_err();
    assert_eq!(panic_message(&*payload), "boom");
}

#[test]
fn test_panic_message_string_payload() {
    let payload = std::panic::catch_unwind(|| panic!("boom {}", 42)).unwrap_err();
    assert_eq!(panic_message(&*payload), "boom 42");
}

#[test]
fn test_panic_message_unknown_payload() {
    let payload: Box<dyn std::any::Any + Send> = Box::new(123u32);
    assert_eq!(panic_message(&*payload), "unknown panic");
}

#[test]
fn test_panic_to_c_string() {
    let payload = std::panic::catch_unwind(|| panic!("boom")).unwrap_err();
    let c_str = panic_to_c_string(payload);
    assert!(!c_str.is_null());
    let msg = unsafe { c_string_to_rust_string(c_str) };
    assert_eq!(msg, "Internal error: boom");
    unsafe { mq_free_string(c_str) };
}

// `mq_eval_ex` helpers: owned results so tests do not manage frees by hand.
struct ExOutcome {
    status: MqStatus,
    values: Vec<String>,
    error: Option<ExError>,
}

struct ExError {
    message: String,
    rendered: String,
    help: Option<String>,
    source_name: String,
    location: Option<MqLocation>,
    exit_code: Option<i32>,
}

unsafe fn owned_str(p: *const c_char) -> String {
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

fn eval_ex_with(engine: *mut MqContext, code: &str, input: &str, format: &str) -> ExOutcome {
    let (code, input, format) = (
        CString::new(code).unwrap(),
        CString::new(input).unwrap(),
        CString::new(format).unwrap(),
    );
    let mut values: *mut MqValues = ptr::null_mut();
    let mut err: *mut MqError = ptr::null_mut();
    let status = unsafe {
        mq_eval_ex(
            engine,
            code.as_ptr(),
            input.as_ptr(),
            format.as_ptr(),
            &mut values,
            &mut err,
        )
    };

    let values_out = (0..unsafe { mq_values_len(values) })
        .map(|i| unsafe { owned_str(mq_value_string(values, i)) })
        .collect();
    let error = unsafe {
        err.as_ref().map(|_| {
            let mut location = MqLocation {
                line: 0,
                column: 0,
                offset: 0,
                length: 0,
            };
            let mut exit_code = 0;
            let help = mq_error_help(err);
            ExError {
                message: owned_str(mq_error_message(err)),
                rendered: owned_str(mq_error_rendered(err)),
                help: (!help.is_null()).then(|| owned_str(help)),
                source_name: owned_str(mq_error_source_name(err)),
                location: mq_error_location(err, &mut location).then_some(location),
                exit_code: mq_error_exit_code(err, &mut exit_code).then_some(exit_code),
            }
        })
    };
    if let Some(error) = &error {
        assert_eq!(unsafe { mq_error_status(err) }, status, "{}", error.message);
    }
    assert_eq!(values.is_null(), status != MqStatus::MQ_OK);

    unsafe {
        mq_values_free(values);
        mq_error_free(err);
    }
    ExOutcome {
        status,
        values: values_out,
        error,
    }
}

fn eval_ex(code: &str) -> ExOutcome {
    let engine = mq_create();
    let outcome = eval_ex_with(engine, code, "hello", "text");
    mq_destroy(engine);
    outcome
}

#[test]
fn test_eval_ex_success() {
    let outcome = eval_ex(r#"add(" world")"#);
    assert_eq!(outcome.status, MqStatus::MQ_OK);
    assert_eq!(outcome.values, vec!["hello world"]);
    assert!(outcome.error.is_none());
}

#[test]
fn test_eval_ex_syntax_error_has_location_and_source_excerpt() {
    let outcome = eval_ex("let a = 1\n| |");
    let error = outcome.error.unwrap();

    assert_eq!(outcome.status, MqStatus::MQ_ERR_SYNTAX);
    assert!(!error.message.starts_with("Error evaluating query"));
    assert!(error.rendered.contains(&error.message));
    assert!(error.rendered.len() > error.message.len());
    assert!(error.help.is_some());
    assert_eq!(error.source_name, "");
    assert_eq!(error.exit_code, None);
    let location = error.location.unwrap();
    assert_eq!((location.line, location.column), (2, 3));
    assert_eq!(location.offset, "let a = 1\n| ".len());
}

#[test]
fn test_eval_ex_location_column_counts_characters() {
    let outcome = eval_ex("\"あい\" | |");
    let location = outcome.error.unwrap().location.unwrap();

    assert_eq!(outcome.status, MqStatus::MQ_ERR_SYNTAX);
    assert_eq!(location.column, 8);
    assert_eq!(location.offset, "\"あい\" | ".len());
}

#[test]
fn test_eval_ex_runtime_error() {
    let outcome = eval_ex("1 / 0");
    let error = outcome.error.unwrap();

    assert_eq!(outcome.status, MqStatus::MQ_ERR_RUNTIME);
    assert_eq!(error.message, "Division by zero");
    assert_eq!(error.location.map(|l| l.line), Some(1));
}

#[test]
fn test_eval_ex_timeout() {
    let engine = mq_create();
    mq_set_timeout_ms(engine, 100);
    let outcome = eval_ex_with(engine, "while(true): 1 end", "hello", "text");
    mq_destroy(engine);

    assert_eq!(outcome.status, MqStatus::MQ_ERR_TIMEOUT);
}

#[test]
fn test_eval_with_module_input_formats() {
    let engine = mq_create();
    for (format, input, code, expected) in [
        ("json", r#"{"a": [1, 2]}"#, r#"get("a") | get(1)"#, "2"),
        ("JSONL", "{\"a\": 1}\n\n{\"a\": 2}\n", r#"get("a")"#, "2"),
        ("yaml", "a: x", r#"get("a")"#, "x"),
        ("csv", "n,m\n1,2\n", r#"first() | get("m")"#, "2"),
    ] {
        let outcome = eval_ex_with(engine, code, input, format);
        assert_eq!(outcome.status, MqStatus::MQ_OK, "{format}");
        assert_eq!(outcome.values.last().map(String::as_str), Some(expected), "{format}");
    }
    mq_destroy(engine);
}

#[test]
fn test_eval_with_binary_input_format_is_rejected() {
    let engine = mq_create();
    for format in ["bytes", "cbor"] {
        let outcome = eval_ex_with(engine, ".", "x", format);
        assert_eq!(outcome.status, MqStatus::MQ_ERR_INVALID_ARGUMENT, "{format}");
    }
    mq_destroy(engine);
}

#[test]
fn test_eval_ex_halt_reports_exit_code() {
    let outcome = eval_ex("halt(3)");

    assert_eq!(outcome.status, MqStatus::MQ_ERR_HALT);
    assert_eq!(outcome.error.unwrap().exit_code, Some(3));
}

#[test]
fn test_eval_ex_invalid_arguments() {
    let engine = mq_create();
    for (outcome, message) in [
        (
            eval_ex_with(ptr::null_mut(), ".h", "x", "text"),
            "Engine pointer is null",
        ),
        (
            eval_ex_with(engine, ".h", "x", "docx"),
            "Unsupported input format: docx",
        ),
    ] {
        assert_eq!(outcome.status, MqStatus::MQ_ERR_INVALID_ARGUMENT);
        let error = outcome.error.unwrap();
        assert_eq!(error.message, message);
        assert_eq!(error.rendered, message);
        assert_eq!(error.location, None);
    }
    mq_destroy(engine);
}

#[test]
fn test_eval_ex_null_out_and_err_pointers() {
    let engine = mq_create();
    let (code, input, format) = (c".h", c"x", c"text");

    // A null `out` is rejected, and a null `err` just drops the details.
    let status = unsafe {
        mq_eval_ex(
            engine,
            code.as_ptr(),
            input.as_ptr(),
            format.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!(status, MqStatus::MQ_ERR_INVALID_ARGUMENT);

    // Reading through a null handle is safe.
    unsafe {
        assert_eq!(mq_error_status(ptr::null()), MqStatus::MQ_ERR_INVALID_ARGUMENT);
        assert!(mq_error_message(ptr::null()).is_null());
        assert_eq!(mq_values_len(ptr::null()), 0);
        assert!(mq_value_string(ptr::null(), 0).is_null());
        mq_error_free(ptr::null_mut());
        mq_values_free(ptr::null_mut());
    }
    mq_destroy(engine);
}

#[test]
fn test_eval_ex_value_string_out_of_range_is_null() {
    let engine = mq_create();
    let (code, input, format) = (c"1", c"x", c"text");
    let mut values: *mut MqValues = ptr::null_mut();
    let status = unsafe {
        mq_eval_ex(
            engine,
            code.as_ptr(),
            input.as_ptr(),
            format.as_ptr(),
            &mut values,
            ptr::null_mut(),
        )
    };

    assert_eq!(status, MqStatus::MQ_OK);
    unsafe {
        assert_eq!(mq_values_len(values), 1);
        assert!(mq_value_string(values, 1).is_null());
        mq_values_free(values);
    }
    mq_destroy(engine);
}

#[test]
fn test_legacy_eval_error_message_is_unchanged() {
    let engine = mq_create();
    let (code, input, format) = (c"|", c"x", c"text");
    let result = unsafe { mq_eval(engine, code.as_ptr(), input.as_ptr(), format.as_ptr()) };
    let message = unsafe { c_string_to_rust_string(result.error_msg) };

    assert!(message.starts_with("Error evaluating query: "), "{message}");
    mq_free_result(result);
    mq_destroy(engine);
}

#[test]
fn test_eval_ex_missing_module_is_module_error() {
    let outcome = eval_ex(r#"import "no_such_module_xyz" | 1"#);

    assert_eq!(
        outcome.status,
        MqStatus::MQ_ERR_MODULE,
        "{:?}",
        outcome.error.map(|e| e.message)
    );
}

// Runs `code` and hands the results to `f`.
fn with_values(code: &str, input: &str, format: &str, f: impl FnOnce(*const MqValues)) {
    let engine = mq_create();
    let (code, input, format) = (
        CString::new(code).unwrap(),
        CString::new(input).unwrap(),
        CString::new(format).unwrap(),
    );
    let mut values: *mut MqValues = ptr::null_mut();
    let status = unsafe {
        mq_eval_ex(
            engine,
            code.as_ptr(),
            input.as_ptr(),
            format.as_ptr(),
            &mut values,
            ptr::null_mut(),
        )
    };
    assert_eq!(status, MqStatus::MQ_OK);
    f(values);
    unsafe { mq_values_free(values) };
    mq_destroy(engine);
}

fn type_of(code: &str) -> MqType {
    let mut ty = MqType::MQ_TYPE_INVALID;
    with_values(code, "x", "text", |values| ty = unsafe { mq_value_type(values, 0) });
    ty
}

#[test]
fn test_value_type_distinguishes_values_that_share_a_json_form() {
    assert_eq!(type_of("None"), MqType::MQ_TYPE_NONE);
    assert_eq!(type_of("true"), MqType::MQ_TYPE_BOOL);
    assert_eq!(type_of("1.5"), MqType::MQ_TYPE_NUMBER);
    assert_eq!(type_of(r#""s""#), MqType::MQ_TYPE_STRING);
    assert_eq!(type_of(":s"), MqType::MQ_TYPE_SYMBOL);
    assert_eq!(type_of("[1, 2]"), MqType::MQ_TYPE_ARRAY);
    assert_eq!(type_of(r#"{"a": 1}"#), MqType::MQ_TYPE_DICT);
    assert_eq!(type_of("fn(): 1;"), MqType::MQ_TYPE_OTHER);
}

#[test]
fn test_value_type_out_of_range_is_invalid() {
    with_values("1", "x", "text", |values| unsafe {
        assert_eq!(mq_value_type(values, 1), MqType::MQ_TYPE_INVALID);
        assert_eq!(mq_value_type(ptr::null(), 0), MqType::MQ_TYPE_INVALID);
    });
}

#[test]
fn test_value_number_and_bool_only_match_their_type() {
    with_values("1.5", "x", "text", |values| unsafe {
        let (mut n, mut b) = (0.0, false);
        assert!(mq_value_number(values, 0, &mut n));
        assert_eq!(n, 1.5);
        assert!(!mq_value_bool(values, 0, &mut b));
        assert!(!mq_value_number(values, 0, ptr::null_mut()));
    });
    with_values("true", "x", "text", |values| unsafe {
        let (mut n, mut b) = (0.0, false);
        assert!(mq_value_bool(values, 0, &mut b));
        assert!(b);
        assert!(!mq_value_number(values, 0, &mut n));
    });
}

#[test]
fn test_markdown_value_exposes_kind_position_and_json() {
    with_values(".h", "# Title\n\nbody", "markdown", |values| unsafe {
        assert_eq!(mq_value_type(values, 0), MqType::MQ_TYPE_MARKDOWN);
        assert_eq!(owned_str(mq_value_node_kind(values, 0)), "h1");

        let mut position = MqPosition {
            start_line: 0,
            start_column: 0,
            end_line: 0,
            end_column: 0,
        };
        assert!(mq_value_position(values, 0, &mut position));
        assert_eq!((position.start_line, position.start_column), (1, 1));
        assert_eq!((position.end_line, position.end_column), (1, 8));

        let json = mq_value_to_json(values, 0);
        let parsed: serde_json::Value = serde_json::from_str(&owned_str(json)).unwrap();
        mq_free_string(json);
        assert!(parsed.is_object());
    });
}

#[test]
fn test_non_markdown_value_has_no_kind_or_position() {
    with_values(r#""s""#, "x", "text", |values| unsafe {
        let mut position = MqPosition {
            start_line: 0,
            start_column: 0,
            end_line: 0,
            end_column: 0,
        };
        assert!(mq_value_node_kind(values, 0).is_null());
        assert!(!mq_value_position(values, 0, &mut position));
    });
}

#[test]
fn test_values_to_json_is_an_array_of_all_results() {
    with_values(r#"[1, "a"]"#, "x\ny", "text", |values| unsafe {
        let all = mq_values_to_json(values);
        assert_eq!(owned_str(all), r#"[[1,"a"],[1,"a"]]"#);
        mq_free_string(all);

        assert!(mq_value_to_json(values, 2).is_null());
        assert!(mq_values_to_json(ptr::null()).is_null());
    });
    with_values(r#"{"k": None}"#, "x", "text", |values| unsafe {
        let one = mq_value_to_json(values, 0);
        assert_eq!(owned_str(one), r#"{"k":null}"#);
        mq_free_string(one);
    });
}

#[test]
fn test_engine_new_creates_a_usable_engine() {
    let mut engine: *mut MqContext = ptr::null_mut();
    let mut err: *mut MqError = ptr::null_mut();
    let status = unsafe { mq_engine_new(&mut engine, &mut err) };

    assert_eq!(status, MqStatus::MQ_OK);
    assert!(!engine.is_null());
    assert!(err.is_null());
    assert_eq!(eval_ex_with(engine, "1", "x", "text").values, vec!["1"]);
    mq_destroy(engine);

    let status = unsafe { mq_engine_new(ptr::null_mut(), &mut err) };
    assert_eq!(status, MqStatus::MQ_ERR_INVALID_ARGUMENT);
    unsafe { mq_error_free(err) };
}

#[test]
fn test_module_ex_reports_status() {
    let engine = mq_create();
    let missing = c"no_such_module_xyz";
    let mut err: *mut MqError = ptr::null_mut();

    for status in unsafe {
        [
            mq_import_module_ex(engine, missing.as_ptr(), &mut err),
            mq_load_module_ex(engine, missing.as_ptr(), ptr::null_mut()),
        ]
    } {
        assert_ne!(status, MqStatus::MQ_OK);
        assert_ne!(status, MqStatus::MQ_ERR_INVALID_ARGUMENT);
    }
    assert!(!err.is_null());
    unsafe { mq_error_free(err) };

    let status = unsafe { mq_import_module_ex(ptr::null_mut(), missing.as_ptr(), ptr::null_mut()) };
    assert_eq!(status, MqStatus::MQ_ERR_INVALID_ARGUMENT);
    mq_destroy(engine);
}

#[test]
fn test_clear_http_cache_ex_without_engine_or_feature() {
    let engine = mq_create();
    let mut err: *mut MqError = ptr::null_mut();

    #[cfg(feature = "http-import")]
    {
        let status = unsafe { mq_clear_http_cache_ex(ptr::null_mut(), &mut err) };
        assert_eq!(status, MqStatus::MQ_ERR_INVALID_ARGUMENT);
        let status = unsafe { mq_clear_http_cache_all_ex(engine, &mut err) };
        assert_eq!(status, MqStatus::MQ_OK);
    }
    #[cfg(not(feature = "http-import"))]
    for status in unsafe {
        [
            mq_clear_http_cache_ex(engine, &mut err),
            mq_clear_http_cache_all_ex(engine, ptr::null_mut()),
        ]
    } {
        assert_eq!(status, MqStatus::MQ_ERR_UNSUPPORTED);
    }

    unsafe { mq_error_free(err) };
    mq_destroy(engine);
}

const HTML: &CStr = c"<html><head><title>Hello</title></head><body><p>World</p><a href=\"/x\">link</a></body></html>";

unsafe fn html_to_markdown_ex(options: *const MqHtmlOptions) -> String {
    let mut out: *mut c_char = ptr::null_mut();
    let status = unsafe { mq_html_to_markdown_ex(HTML.as_ptr(), options, &mut out, ptr::null_mut()) };
    assert_eq!(status, MqStatus::MQ_OK);
    let markdown = unsafe { owned_str(out) };
    unsafe { mq_free_string(out) };
    markdown
}

#[test]
fn test_html_options_setters_change_the_conversion() {
    unsafe {
        let defaults = html_to_markdown_ex(ptr::null());
        assert!(!defaults.contains("# Hello"));
        assert!(defaults.contains("](/x)"));

        let options = mq_html_options_new();
        assert_eq!(html_to_markdown_ex(options), defaults);

        mq_html_options_set_use_title_as_h1(options, true);
        mq_html_options_set_base_url(options, c"https://example.com".as_ptr());
        let configured = html_to_markdown_ex(options);
        assert!(configured.contains("# Hello"));
        assert!(configured.contains("](https://example.com/x)"));

        // A null base URL clears it again.
        mq_html_options_set_base_url(options, ptr::null());
        assert!(html_to_markdown_ex(options).contains("](/x)"));
        mq_html_options_free(options);
    }
}

#[test]
fn test_html_to_markdown_ex_invalid_arguments() {
    let mut out: *mut c_char = ptr::null_mut();
    let mut err: *mut MqError = ptr::null_mut();
    unsafe {
        let status = mq_html_to_markdown_ex(ptr::null(), ptr::null(), &mut out, &mut err);
        assert_eq!(status, MqStatus::MQ_ERR_INVALID_ARGUMENT);
        assert!(out.is_null());
        assert_eq!(owned_str(mq_error_message(err)), "HTML input pointer is null");
        mq_error_free(err);

        let status = mq_html_to_markdown_ex(HTML.as_ptr(), ptr::null(), ptr::null_mut(), ptr::null_mut());
        assert_eq!(status, MqStatus::MQ_ERR_INVALID_ARGUMENT);

        // Setters and free accept null.
        mq_html_options_set_use_title_as_h1(ptr::null_mut(), true);
        mq_html_options_free(ptr::null_mut());
    }
}
