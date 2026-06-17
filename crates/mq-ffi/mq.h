#ifndef MQ_H
#define MQ_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * C-compatible optimization level for AST transformations applied before evaluation.
 */
typedef enum MqOptimizationLevel {
  None = 0,
  Basic = 1,
  Full = 2,
} MqOptimizationLevel;

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

/**
 * Returns the mq-ffi library version as a static, null-terminated string.
 */
const char *mq_version(void);

/**
 * Sets the optimization level for AST transformations applied before evaluation.
 * Has no effect if `engine_ptr` is null.
 */
void mq_set_optimization_level(mq_context_t *engine_ptr, enum MqOptimizationLevel level);

/**
 * Sets the maximum call stack depth for function calls, to guard against
 * runaway recursion in untrusted mq code. Has no effect if `engine_ptr` is null.
 */
void mq_set_max_call_stack_depth(mq_context_t *engine_ptr, uint32_t max_call_stack_depth);

/**
 * Sets the search paths used to resolve modules loaded via `mq_import_module`
 * or `mq_load_module`. Has no effect if `engine_ptr` is null.
 */
void mq_set_search_paths(mq_context_t *engine_ptr, const char *const *paths, uintptr_t paths_len);

/**
 * Defines a string variable that can be referenced from mq code evaluated
 * afterwards by `mq_eval`, allowing values from the host environment to be
 * injected without building query strings by hand.
 * Has no effect if `engine_ptr` is null.
 */
void mq_define_string_value(mq_context_t *engine_ptr, const char *name_c, const char *value_c);

/**
 * Imports an external module by name, searched for in the paths configured via
 * `mq_set_search_paths`, making its exported definitions available to subsequent
 * `mq_eval` calls on the same engine.
 */
char *mq_import_module(mq_context_t *engine_ptr, const char *module_name_c);

/**
 * Loads an external module by name, searched for in the paths configured via
 * `mq_set_search_paths`, making its exported definitions available to subsequent
 * `mq_eval` calls on the same engine.
 */
char *mq_load_module(mq_context_t *engine_ptr, const char *module_name_c);

/**
 * Replaces the HTTP resolver's domain allowlist used when importing modules
 * over HTTP(S) via `mq_import_module` / `mq_load_module`. An empty list restricts
 * access to the built-in default domain only; it does not open up all URLs.
 * Has no effect if `engine_ptr` is null.
 */
void mq_set_http_allowed_domains(mq_context_t *engine_ptr,
                                 const char *const *domains,
                                 uintptr_t domains_len);

/**
 * Clears locally-cached HTTP module files, forcing a re-fetch of all cached
 * modules on the next import. Has no effect if `engine_ptr` is null.
 */
char *mq_clear_http_cache(mq_context_t *engine_ptr);

/**
 * Clears all HTTP module cache including versioned modules and lock files.
 * Has no effect if `engine_ptr` is null.
 */
char *mq_clear_http_cache_all(mq_context_t *engine_ptr);

#endif  /* MQ_H */
