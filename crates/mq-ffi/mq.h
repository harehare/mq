#ifndef MQ_H
#define MQ_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef void mq_context_t;

typedef struct mq_result_t {
  char **values;
  uintptr_t values_len;
  char *error_msg;
} mq_result_t;

/**
 * C-compatible conversion options for HTML to Markdown conversion.
 */
typedef struct MqConversionOptions {
  /**
   * Extract script tags as code blocks
   */
  bool extract_scripts_as_code_blocks;
  /**
   * Generate front matter from HTML head metadata
   */
  bool generate_front_matter;
  /**
   * Use HTML title tag as H1 heading
   */
  bool use_title_as_h1;
} MqConversionOptions;

/**
 * Creates a new mq_lang engine.
 * The caller is responsible for destroying the engine using `mq_destroy`.
 */
mq_context_t *mq_create(void);

/**
 * Destroys an mq_lang engine.
 */
void mq_destroy(mq_context_t *engine_ptr);

/**
 * Evaluates mq code with the given input.
 * The caller is responsible for freeing the result using `mq_free_result`.
 *
 * # Safety
 *
 * This function is unsafe because it dereferences raw pointers. The caller must ensure:
 * - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`
 * - `code_c` must be a valid pointer to a null-terminated C string
 * - `input_c` must be a valid pointer to a null-terminated C string
 * - `input_format_c` must be a valid pointer to a null-terminated C string
 * - All string pointers must remain valid for the duration of this function call
 * - The returned `MqResult` must be freed using `mq_free_result` to avoid memory leaks
 */
struct mq_result_t mq_eval(mq_context_t *engine_ptr,
                           const char *code_c,
                           const char *input_c,
                           const char *input_format_c);

/**
 * Frees a C string allocated by Rust.
 *
 * # Safety
 *
 * This function is unsafe because it dereferences a raw pointer. The caller must ensure:
 * - `s` must be a valid pointer to a C string previously allocated by Rust using `CString::into_raw()`
 * - `s` must not be used after calling this function (use-after-free protection)
 * - This function must only be called once per pointer (double-free protection)
 * - If `s` is null, the function safely returns without performing any operations
 */
void mq_free_string(char *s);

/**
 * Frees the MqResult structure including its contents.
 */
void mq_free_result(struct mq_result_t result);

/**
 * Converts HTML to Markdown with the given conversion options.
 * Returns a C string containing the markdown output, or NULL on error.
 * The caller is responsible for freeing the result using `mq_free_string`.
 *
 * # Safety
 *
 * This function is unsafe because it dereferences raw pointers. The caller must ensure:
 * - `html_input_c` must be a valid pointer to a null-terminated C string
 * - `error_msg` must be a valid pointer to a location where an error message pointer can be stored
 * - The string pointer must remain valid for the duration of this function call
 * - The returned C string must be freed using `mq_free_string` to avoid memory leaks
 * - If an error occurs, the function returns NULL and sets `*error_msg` to an error message
 *
 * # Example
 *
 * ```c
 * char* error_msg = NULL;
 * MqConversionOptions options = {
 *     .extract_scripts_as_code_blocks = false,
 *     .generate_front_matter = true,
 *     .use_title_as_h1 = true
 * };
 *
 * char* markdown = mq_html_to_markdown(
 *     "<html><head><title>Hello</title></head><body><p>World</p></body></html>",
 *     options,
 *     &error_msg
 * );
 *
 * if (markdown == NULL) {
 *     printf("Error: %s\n", error_msg);
 *     mq_free_string(error_msg);
 * } else {
 *     printf("%s\n", markdown);
 *     mq_free_string(markdown);
 * }
 * ```
 */
char *mq_html_to_markdown(const char *html_input_c,
                          struct MqConversionOptions options,
                          char **error_msg);

#endif  /* MQ_H */
