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
