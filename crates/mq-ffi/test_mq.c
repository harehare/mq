#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <assert.h>
#include "mq.h"

#define TEST_MODULE_DIR "/tmp"

// Test helper functions
void assert_not_null(void *ptr, const char *msg) {
    if (ptr == NULL) {
        fprintf(stderr, "FAIL: %s\n", msg);
        exit(1);
    }
}

void assert_null(void *ptr, const char *msg) {
    if (ptr != NULL) {
        fprintf(stderr, "FAIL: %s\n", msg);
        exit(1);
    }
}

void assert_equals(size_t a, size_t b, const char *msg) {
    if (a != b) {
        fprintf(stderr, "FAIL: %s (expected %zu, got %zu)\n", msg, b, a);
        exit(1);
    }
}

void assert_str_equals(const char *a, const char *b, const char *msg) {
    if (strcmp(a, b) != 0) {
        fprintf(stderr, "FAIL: %s\n  Expected: %s\n  Got: %s\n", msg, b, a);
        exit(1);
    }
}

void test_create_destroy() {
    printf("Test 1: Create and destroy engine... ");

    mq_context_t *engine = mq_create();
    assert_not_null(engine, "Engine should not be null");

    mq_destroy(engine);

    // Test destroying null engine (should not crash)
    mq_destroy(NULL);

    printf("PASS\n");
}

void test_eval_text_input() {
    printf("Test 2: Eval with text input... ");

    mq_context_t *engine = mq_create();

    struct mq_result_t result = mq_eval(
        engine,
        "select(contains(\"line\"))",
        "# line1\n## line2\n### line3",
        "text"
    );

    assert_null(result.error_msg, "Should not have error");
    assert_not_null(result.values, "Values should not be null");
    assert_equals(result.values_len, 3, "Should have 3 values");

    // Check values
    assert_str_equals(result.values[0], "# line1", "First value mismatch");
    assert_str_equals(result.values[1], "## line2", "Second value mismatch");
    assert_str_equals(result.values[2], "### line3", "Third value mismatch");

    mq_free_result(result);
    mq_destroy(engine);

    printf("PASS\n");
}

void test_eval_markdown_input() {
    printf("Test 3: Eval with markdown input... ");

    mq_context_t *engine = mq_create();

    struct mq_result_t result = mq_eval(
        engine,
        ".h",
        "# Header\n\nSome text\n\n## Subheader",
        "markdown"
    );

    assert_null(result.error_msg, "Should not have error");
    assert_not_null(result.values, "Values should not be null");

    mq_free_result(result);
    mq_destroy(engine);

    printf("PASS\n");
}

void test_eval_null_engine() {
    printf("Test 4: Eval with null engine... ");

    struct mq_result_t result = mq_eval(
        NULL,
        ".h",
        "test",
        "text"
    );

    assert_null(result.values, "Values should be null");
    assert_equals(result.values_len, 0, "Values length should be 0");
    assert_not_null(result.error_msg, "Should have error message");
    assert_str_equals(result.error_msg, "Engine pointer is null", "Error message mismatch");

    mq_free_result(result);

    printf("PASS\n");
}

void test_eval_invalid_code() {
    printf("Test 5: Eval with invalid code... ");

    mq_context_t *engine = mq_create();

    struct mq_result_t result = mq_eval(
        engine,
        "invalid_function()",
        "test",
        "text"
    );

    assert_null(result.values, "Values should be null");
    assert_equals(result.values_len, 0, "Values length should be 0");
    assert_not_null(result.error_msg, "Should have error message");

    mq_free_result(result);
    mq_destroy(engine);

    printf("PASS\n");
}

void test_eval_unsupported_format() {
    printf("Test 6: Eval with unsupported format... ");

    mq_context_t *engine = mq_create();

    struct mq_result_t result = mq_eval(
        engine,
        ".h",
        "test",
        "json"
    );

    assert_null(result.values, "Values should be null");
    assert_not_null(result.error_msg, "Should have error message");

    mq_free_result(result);
    mq_destroy(engine);

    printf("PASS\n");
}

void test_format_case_insensitive() {
    printf("Test 7: Case-insensitive format... ");

    mq_context_t *engine = mq_create();

    // Test uppercase format
    struct mq_result_t result1 = mq_eval(engine, ".h", "test", "TEXT");
    assert_null(result1.error_msg, "Should not have error with uppercase TEXT");
    mq_free_result(result1);

    // Test mixed case format
    struct mq_result_t result2 = mq_eval(engine, ".h", "# Test", "MarkDown");
    assert_null(result2.error_msg, "Should not have error with mixed case MarkDown");
    mq_free_result(result2);

    mq_destroy(engine);

    printf("PASS\n");
}

void test_html_to_markdown() {
    printf("Test 8: HTML to Markdown conversion... ");

    char *error_msg = NULL;
    MqConversionOptions options = {
        .extract_scripts_as_code_blocks = false,
        .generate_front_matter = false,
        .use_title_as_h1 = false
    };

    char *markdown = mq_html_to_markdown(
        "<p>Hello, World!</p>",
        options,
        &error_msg
    );

    assert_null(error_msg, "Should not have error");
    assert_not_null(markdown, "Markdown should not be null");

    mq_free_string(markdown);

    printf("PASS\n");
}

void test_html_to_markdown_with_options() {
    printf("Test 9: HTML to Markdown with options... ");

    char *error_msg = NULL;
    MqConversionOptions options = {
        .extract_scripts_as_code_blocks = false,
        .generate_front_matter = true,
        .use_title_as_h1 = true
    };

    char *markdown = mq_html_to_markdown(
        "<html><head><title>Test Page</title></head><body><p>Content</p></body></html>",
        options,
        &error_msg
    );

    assert_null(error_msg, "Should not have error");
    assert_not_null(markdown, "Markdown should not be null");

    mq_free_string(markdown);

    printf("PASS\n");
}

void test_html_to_markdown_null_input() {
    printf("Test 10: HTML to Markdown with null input... ");

    char *error_msg = NULL;
    MqConversionOptions options = {
        .extract_scripts_as_code_blocks = false,
        .generate_front_matter = false,
        .use_title_as_h1 = false
    };

    char *markdown = mq_html_to_markdown(NULL, options, &error_msg);

    assert_null(markdown, "Markdown should be null");
    assert_not_null(error_msg, "Should have error message");
    assert_str_equals(error_msg, "HTML input pointer is null", "Error message mismatch");

    mq_free_string(error_msg);

    printf("PASS\n");
}

void test_version() {
    printf("Test 11: mq_version... ");

    const char *version = mq_version();
    assert_not_null((void *)version, "Version should not be null");
    assert(strlen(version) > 0);

    printf("PASS\n");
}

void test_set_optimization_level() {
    printf("Test 12: mq_set_optimization_level... ");

    mq_context_t *engine = mq_create();

    mq_set_optimization_level(engine, None);
    mq_set_optimization_level(engine, Basic);
    mq_set_optimization_level(engine, Full);

    // Evaluation must still succeed after switching optimization levels.
    struct mq_result_t result = mq_eval(engine, "len()", "abc", "text");
    assert_null(result.error_msg, "Should not have error");
    mq_free_result(result);

    // Should not crash with a null engine.
    mq_set_optimization_level(NULL, None);

    mq_destroy(engine);

    printf("PASS\n");
}

void test_set_max_call_stack_depth() {
    printf("Test 13: mq_set_max_call_stack_depth... ");

    mq_context_t *engine = mq_create();
    mq_set_max_call_stack_depth(engine, 2);

    struct mq_result_t result = mq_eval(engine, "def rec(): rec(); rec()", "test", "text");
    assert_not_null(result.error_msg, "Should have error due to stack depth limit");
    mq_free_result(result);

    // Should not crash with a null engine.
    mq_set_max_call_stack_depth(NULL, 2);

    mq_destroy(engine);

    printf("PASS\n");
}

void test_define_string_value() {
    printf("Test 14: mq_define_string_value... ");

    mq_context_t *engine = mq_create();
    mq_define_string_value(engine, "greeting", "hello");

    struct mq_result_t result = mq_eval(engine, "greeting", "test", "text");
    assert_null(result.error_msg, "Should not have error");
    assert_equals(result.values_len, 1, "Should have 1 value");
    assert_str_equals(result.values[0], "hello", "Value mismatch");

    mq_free_result(result);

    // Should not crash with a null engine.
    mq_define_string_value(NULL, "greeting", "hello");

    mq_destroy(engine);

    printf("PASS\n");
}

void test_load_module() {
    printf("Test 15: mq_set_search_paths + mq_load_module... ");

    const char *module_path = TEST_MODULE_DIR "/mq_ffi_c_test_module.mq";
    FILE *f = fopen(module_path, "w");
    assert_not_null(f, "Should be able to create temp module file");
    fputs("def double(x): x * 2;", f);
    fclose(f);

    mq_context_t *engine = mq_create();
    const char *paths[] = {TEST_MODULE_DIR};
    mq_set_search_paths(engine, paths, 1);

    char *error_msg = mq_load_module(engine, "mq_ffi_c_test_module");
    assert_null(error_msg, "Should not have error loading module");

    struct mq_result_t result = mq_eval(engine, "double(21)", "test", "text");
    assert_null(result.error_msg, "Should not have error calling loaded function");
    assert_str_equals(result.values[0], "42", "double(21) should be 42");

    mq_free_result(result);
    mq_destroy(engine);
    remove(module_path);

    printf("PASS\n");
}

void test_load_module_missing() {
    printf("Test 16: mq_load_module with missing module... ");

    mq_context_t *engine = mq_create();
    char *error_msg = mq_load_module(engine, "nonexistent_module_for_c_test");
    assert_not_null(error_msg, "Should have error for missing module");

    mq_free_string(error_msg);
    mq_destroy(engine);

    printf("PASS\n");
}

void test_load_module_null_engine() {
    printf("Test 17: mq_load_module with null engine... ");

    char *error_msg = mq_load_module(NULL, "anything");
    assert_not_null(error_msg, "Should have error message");
    assert_str_equals(error_msg, "Engine pointer is null", "Error message mismatch");

    mq_free_string(error_msg);

    printf("PASS\n");
}

void test_import_module() {
    printf("Test 18: mq_set_search_paths + mq_import_module... ");

    const char *module_path = TEST_MODULE_DIR "/mq_ffi_c_test_import_module.mq";
    FILE *f = fopen(module_path, "w");
    assert_not_null(f, "Should be able to create temp module file");
    fputs("def triple(x): x * 3;", f);
    fclose(f);

    mq_context_t *engine = mq_create();
    const char *paths[] = {TEST_MODULE_DIR};
    mq_set_search_paths(engine, paths, 1);

    char *error_msg = mq_import_module(engine, "mq_ffi_c_test_import_module");
    assert_null(error_msg, "Should not have error importing module");

    // Imported modules are namespaced, unlike mq_load_module.
    struct mq_result_t result = mq_eval(engine, "mq_ffi_c_test_import_module::triple(2)", "test", "text");
    assert_null(result.error_msg, "Should not have error calling imported function");
    assert_str_equals(result.values[0], "6", "triple(2) should be 6");

    mq_free_result(result);
    mq_destroy(engine);
    remove(module_path);

    printf("PASS\n");
}

void test_import_module_missing() {
    printf("Test 19: mq_import_module with missing module... ");

    mq_context_t *engine = mq_create();
    char *error_msg = mq_import_module(engine, "nonexistent_module_for_c_test");
    assert_not_null(error_msg, "Should have error for missing module");

    mq_free_string(error_msg);
    mq_destroy(engine);

    printf("PASS\n");
}

void test_import_module_null_engine() {
    printf("Test 20: mq_import_module with null engine... ");

    char *error_msg = mq_import_module(NULL, "anything");
    assert_not_null(error_msg, "Should have error message");
    assert_str_equals(error_msg, "Engine pointer is null", "Error message mismatch");

    mq_free_string(error_msg);

    printf("PASS\n");
}

void test_set_search_paths_edge_cases() {
    printf("Test 21: mq_set_search_paths edge cases... ");

    mq_context_t *engine = mq_create();

    // Should not crash with an empty/null paths array.
    mq_set_search_paths(engine, NULL, 0);

    // Should not crash with a null engine.
    const char *paths[] = {TEST_MODULE_DIR};
    mq_set_search_paths(NULL, paths, 1);

    mq_destroy(engine);

    printf("PASS\n");
}

void test_http_allowed_domains_does_not_crash() {
    printf("Test 22: mq_set_http_allowed_domains... ");

    mq_context_t *engine = mq_create();
    const char *domains[] = {"example.com"};

    // The symbol must always be present regardless of build features; it
    // should never crash, even with a null engine or empty domain list.
    mq_set_http_allowed_domains(engine, domains, 1);
    mq_set_http_allowed_domains(engine, NULL, 0);
    mq_set_http_allowed_domains(NULL, domains, 1);

    mq_destroy(engine);

    printf("PASS\n");
}

void test_clear_http_cache_does_not_crash() {
    printf("Test 23: mq_clear_http_cache / mq_clear_http_cache_all... ");

    mq_context_t *engine = mq_create();

    // These symbols must always be present regardless of build features.
    // This build does not enable `http-import`, so they are expected to
    // report that HTTP module support is unavailable rather than crash.
    char *error_msg = mq_clear_http_cache(engine);
    assert_not_null(error_msg, "Should report http-import is unavailable");
    mq_free_string(error_msg);

    char *error_msg_all = mq_clear_http_cache_all(engine);
    assert_not_null(error_msg_all, "Should report http-import is unavailable");
    mq_free_string(error_msg_all);

    mq_destroy(engine);

    printf("PASS\n");
}

int main() {
    printf("Running mq-ffi C tests...\n\n");

    test_create_destroy();
    test_eval_text_input();
    test_eval_markdown_input();
    test_eval_null_engine();
    test_eval_invalid_code();
    test_eval_unsupported_format();
    test_format_case_insensitive();
    test_html_to_markdown();
    test_html_to_markdown_with_options();
    test_html_to_markdown_null_input();
    test_version();
    test_set_optimization_level();
    test_set_max_call_stack_depth();
    test_define_string_value();
    test_load_module();
    test_load_module_missing();
    test_load_module_null_engine();
    test_import_module();
    test_import_module_missing();
    test_import_module_null_engine();
    test_set_search_paths_edge_cases();
    test_http_allowed_domains_does_not_crash();
    test_clear_http_cache_does_not_crash();

    printf("\nAll tests passed!\n");
    return 0;
}
