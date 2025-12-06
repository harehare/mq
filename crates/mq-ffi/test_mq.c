#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <assert.h>
#include "mq.h"

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

    printf("\nAll tests passed!\n");
    return 0;
}
