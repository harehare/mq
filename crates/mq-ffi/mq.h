#ifndef MQ_H
#define MQ_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Timeout value for `mq_set_timeout_ms` that disables the timeout (the default).
 */
#define MQ_NO_TIMEOUT 0

/**
 * Outcome of an `_ex` call. `MQ_OK` is zero; every other value is an error.
 */
typedef enum mq_status_t {
  /**
   * Success.
   */
  MQ_OK = 0,
  /**
   * A null pointer, invalid UTF-8 or unsupported input format.
   */
  MQ_ERR_INVALID_ARGUMENT,
  /**
   * The input document could not be parsed.
   */
  MQ_ERR_INPUT_PARSE,
  /**
   * The query has a syntax error.
   */
  MQ_ERR_SYNTAX,
  /**
   * The query failed while running.
   */
  MQ_ERR_RUNTIME,
  /**
   * A module could not be resolved or loaded.
   */
  MQ_ERR_MODULE,
  /**
   * The timeout set by `mq_set_timeout_ms` was exceeded.
   */
  MQ_ERR_TIMEOUT,
  /**
   * The query called `halt`; see `mq_error_exit_code`.
   */
  MQ_ERR_HALT,
  /**
   * The library was built without the requested feature.
   */
  MQ_ERR_UNSUPPORTED,
  /**
   * A panic was caught at the API boundary.
   */
  MQ_ERR_INTERNAL,
} mq_status_t;

/**
 * Type of a result value. A JSON object can be a dict or a markdown node, and a JSON
 * string can be a string, a symbol or bytes, so use this to tell them apart.
 */
typedef enum mq_type_t {
  /**
   * No such value: a null handle or an index out of range.
   */
  MQ_TYPE_INVALID = 0,
  /**
   * The mq `None` value.
   */
  MQ_TYPE_NONE,
  /**
   * `true` or `false`.
   */
  MQ_TYPE_BOOL,
  /**
   * A number; see `mq_value_number`.
   */
  MQ_TYPE_NUMBER,
  /**
   * A string.
   */
  MQ_TYPE_STRING,
  /**
   * A symbol.
   */
  MQ_TYPE_SYMBOL,
  /**
   * An array. Use `mq_value_to_json` to read its elements.
   */
  MQ_TYPE_ARRAY,
  /**
   * A dict. Use `mq_value_to_json` to read its entries.
   */
  MQ_TYPE_DICT,
  /**
   * A markdown node; see `mq_value_node_kind` and `mq_value_position`.
   */
  MQ_TYPE_MARKDOWN,
  /**
   * Raw bytes.
   */
  MQ_TYPE_BYTES,
  /**
   * A function, coroutine or other value that has no data form.
   */
  MQ_TYPE_OTHER,
} mq_type_t;

/**
 * Opaque handle to an mq engine.
 */
typedef struct mq_context_t mq_context_t;

/**
 * Opaque error details, freed with `mq_error_free`.
 */
typedef struct mq_error_t mq_error_t;

/**
 * Opaque HTML to Markdown conversion options, freed with `mq_html_options_free`.
 */
typedef struct mq_html_options_t mq_html_options_t;

/**
 * Opaque list of query results, freed with `mq_values_free`.
 */
typedef struct mq_values_t mq_values_t;

/**
 * Result of a query: `values` on success, `error_msg` on failure.
 */
typedef struct mq_result_t {
  /**
   * Array of `values_len` result strings.
   */
  char **values;
  /**
   * Number of entries in `values`.
   */
  uintptr_t values_len;
  /**
   * Error message, or null on success.
   */
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
  /**
   * Base URL for resolving relative `href`/`src` values; null falls back to `<base href>`.
   */
  const char *base_url;
} MqConversionOptions;

/**
 * Position of an error in its source.
 */
typedef struct mq_location_t {
  /**
   * Line, starting at 1.
   */
  uint32_t line;
  /**
   * Column in characters, starting at 1.
   */
  uint32_t column;
  /**
   * Byte offset from the start of the source.
   */
  uintptr_t offset;
  /**
   * Length of the span in bytes.
   */
  uintptr_t length;
} mq_location_t;

/**
 * Position of a markdown node in the input. Lines and columns start at 1 and columns
 * count UTF-16 code units.
 */
typedef struct mq_position_t {
  /**
   * Line where the node starts.
   */
  uint32_t start_line;
  /**
   * Column where the node starts.
   */
  uint32_t start_column;
  /**
   * Line where the node ends.
   */
  uint32_t end_line;
  /**
   * Column where the node ends.
   */
  uint32_t end_column;
} mq_position_t;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Creates a new mq_lang engine.
 * The caller is responsible for destroying the engine using `mq_destroy`.
 */
struct mq_context_t *mq_create(void);

/**
 * Creates a new mq_lang engine like `mq_create`, reporting why it failed.
 *
 * On success returns `MQ_OK` and stores the engine in `*out`, to be freed with
 * `mq_destroy`. On failure stores null in `*out` and, if `err` is non-null, an error in
 * `*err`, to be freed with `mq_error_free`.
 *
 * # Safety
 *
 * `out` must be valid for writes and `err` must be null or valid for writes.
 */
enum mq_status_t mq_engine_new(struct mq_context_t **out, struct mq_error_t **err);

/**
 * Destroys an mq_lang engine.
 */
void mq_destroy(struct mq_context_t *engine_ptr);

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
struct mq_result_t mq_eval(struct mq_context_t *engine_ptr,
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
 * Sets the maximum call stack depth for function calls, to guard against
 * runaway recursion in untrusted mq code. Has no effect if `engine_ptr` is null.
 */
void mq_set_max_call_stack_depth(struct mq_context_t *engine_ptr, uint32_t max_call_stack_depth);

/**
 * Sets the maximum wall-clock time, in milliseconds, a single `mq_eval` call may run,
 * to guard against non-terminating untrusted mq code. When exceeded, `mq_eval` returns
 * an error. The deadline is checked periodically, so it may be overshot slightly.
 * Pass `MQ_NO_TIMEOUT` to disable it. Has no effect if `engine_ptr` is null.
 */
void mq_set_timeout_ms(struct mq_context_t *engine_ptr, uint64_t timeout_ms);

/**
 * Returns true if panics are caught at the API boundary, false if the library was
 * built with `panic = "abort"` (e.g. `--release`), where any panic aborts the host process.
 */
bool mq_is_panic_safe(void);

/**
 * Sets the search paths used to resolve modules loaded via `mq_import_module`
 * or `mq_load_module`. Has no effect if `engine_ptr` is null.
 *
 * # Safety
 *
 * This function is unsafe because it dereferences raw pointers. The caller must ensure:
 * - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
 * - `paths` must be a valid pointer to an array of `paths_len` null-terminated C strings, or null if `paths_len` is 0
 */
void mq_set_search_paths(struct mq_context_t *engine_ptr,
                         const char *const *paths,
                         uintptr_t paths_len);

/**
 * Defines a string variable that can be referenced from mq code evaluated
 * afterwards by `mq_eval`, allowing values from the host environment to be
 * injected without building query strings by hand.
 * Has no effect if `engine_ptr` is null.
 *
 * # Safety
 *
 * This function is unsafe because it dereferences raw pointers. The caller must ensure:
 * - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
 * - `name_c` must be a valid pointer to a null-terminated C string
 * - `value_c` must be a valid pointer to a null-terminated C string
 */
void mq_define_string_value(struct mq_context_t *engine_ptr,
                            const char *name_c,
                            const char *value_c);

/**
 * Imports an external module by name, searched for in the paths configured via
 * `mq_set_search_paths`, making its exported definitions available to subsequent
 * `mq_eval` calls on the same engine.
 *
 * # Safety
 *
 * This function is unsafe because it dereferences raw pointers. The caller must ensure:
 * - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
 * - `module_name_c` must be a valid pointer to a null-terminated C string
 * - The returned pointer, if non-null, must be freed with `mq_free_string`
 */
char *mq_import_module(struct mq_context_t *engine_ptr, const char *module_name_c);

/**
 * Imports a module like `mq_import_module`, reporting failure as a status plus an
 * `mq_error_t`. On failure, if `err` is non-null, an error is stored in `*err`, to be
 * freed with `mq_error_free`.
 *
 * # Safety
 *
 * `err` must be null or valid for writes. The other arguments are as for
 * `mq_import_module`.
 */
enum mq_status_t mq_import_module_ex(struct mq_context_t *engine_ptr,
                                     const char *module_name_c,
                                     struct mq_error_t **err);

/**
 * Loads an external module by name, searched for in the paths configured via
 * `mq_set_search_paths`, making its exported definitions available to subsequent
 * `mq_eval` calls on the same engine.
 *
 * # Safety
 *
 * This function is unsafe because it dereferences raw pointers. The caller must ensure:
 * - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
 * - `module_name_c` must be a valid pointer to a null-terminated C string
 * - The returned pointer, if non-null, must be freed with `mq_free_string`
 */
char *mq_load_module(struct mq_context_t *engine_ptr, const char *module_name_c);

/**
 * Loads a module like `mq_load_module`, reporting failure as a status plus an
 * `mq_error_t`. On failure, if `err` is non-null, an error is stored in `*err`, to be
 * freed with `mq_error_free`.
 *
 * # Safety
 *
 * `err` must be null or valid for writes. The other arguments are as for
 * `mq_load_module`.
 */
enum mq_status_t mq_load_module_ex(struct mq_context_t *engine_ptr,
                                   const char *module_name_c,
                                   struct mq_error_t **err);

/**
 * Replaces the HTTP resolver's domain allowlist used when importing modules
 * over HTTP(S) via `mq_import_module` / `mq_load_module`. An empty list restricts
 * access to the built-in default domain only; it does not open up all URLs.
 * Has no effect if `engine_ptr` is null.
 *
 * # Safety
 *
 * This function is unsafe because it dereferences raw pointers. The caller must ensure:
 * - `engine_ptr` must be a valid pointer to an `Engine` created by `mq_create`, or null
 * - `domains` must be a valid pointer to an array of `domains_len` null-terminated C strings, or null if `domains_len` is 0
 */
void mq_set_http_allowed_domains(struct mq_context_t *engine_ptr,
                                 const char *const *domains,
                                 uintptr_t domains_len);

/**
 * Clears locally-cached HTTP module files, forcing a re-fetch of all cached
 * modules on the next import. Has no effect if `engine_ptr` is null.
 */
char *mq_clear_http_cache(struct mq_context_t *engine_ptr);

/**
 * Clears all HTTP module cache including versioned modules and lock files.
 * Has no effect if `engine_ptr` is null.
 */
char *mq_clear_http_cache_all(struct mq_context_t *engine_ptr);

/**
 * Clears the HTTP module cache like `mq_clear_http_cache`, reporting failure as a status
 * plus an `mq_error_t`. Unlike the legacy function, a null engine is an error. Returns
 * `MQ_ERR_UNSUPPORTED` if the library was built without the `http-import` feature.
 *
 * # Safety
 *
 * `err` must be null or valid for writes.
 */
enum mq_status_t mq_clear_http_cache_ex(struct mq_context_t *engine_ptr, struct mq_error_t **err);

/**
 * Clears all HTTP module cache like `mq_clear_http_cache_all`, reporting failure as a
 * status plus an `mq_error_t`. See `mq_clear_http_cache_ex`.
 *
 * # Safety
 *
 * `err` must be null or valid for writes.
 */
enum mq_status_t mq_clear_http_cache_all_ex(struct mq_context_t *engine_ptr,
                                            struct mq_error_t **err);

/**
 * Returns the error's status, or `MQ_ERR_INVALID_ARGUMENT` if `error` is null.
 *
 * # Safety
 *
 * `error` must be null or a pointer returned through an `err` out-parameter and not yet freed.
 */
enum mq_status_t mq_error_status(const struct mq_error_t *error);

/**
 * Returns the short error message, e.g. `Unexpected token '|'`. The string is owned by
 * `error` and valid until `mq_error_free`. Returns null if `error` is null.
 *
 * # Safety
 *
 * See `mq_error_status`.
 */
const char *mq_error_message(const struct mq_error_t *error);

/**
 * Returns the message with the offending source excerpt, laid out for a terminal and
 * without colors. Owned by `error`. Equal to the message when there is no source.
 *
 * # Safety
 *
 * See `mq_error_status`.
 */
const char *mq_error_rendered(const struct mq_error_t *error);

/**
 * Returns a hint for fixing the error, or null if there is none. Owned by `error`.
 *
 * # Safety
 *
 * See `mq_error_status`.
 */
const char *mq_error_help(const struct mq_error_t *error);

/**
 * Returns the name of the module the error occurred in, or an empty string for the query
 * itself. Owned by `error`.
 *
 * # Safety
 *
 * See `mq_error_status`.
 */
const char *mq_error_source_name(const struct mq_error_t *error);

/**
 * Stores the error position in `out` and returns true, or returns false if the error has
 * none.
 *
 * # Safety
 *
 * `out` must be null or valid for writes. See also `mq_error_status`.
 */
bool mq_error_location(const struct mq_error_t *error, struct mq_location_t *out);

/**
 * Stores the exit code passed to `halt` in `out` and returns true, or returns false if
 * the query did not halt.
 *
 * # Safety
 *
 * `out` must be null or valid for writes. See also `mq_error_status`.
 */
bool mq_error_exit_code(const struct mq_error_t *error, int32_t *out);

/**
 * Frees an error. Does nothing if `error` is null.
 *
 * # Safety
 *
 * `error` must be null or a pointer returned through an `err` out-parameter, and must not
 * be used afterwards.
 */
void mq_error_free(struct mq_error_t *error);

/**
 * Creates conversion options with every option off and no base URL. Free them with
 * `mq_html_options_free`.
 */
struct mq_html_options_t *mq_html_options_new(void);

/**
 * Frees conversion options. Does nothing if `options` is null.
 *
 * # Safety
 *
 * `options` must be null or a pointer returned by `mq_html_options_new`, and must not be
 * used afterwards.
 */
void mq_html_options_free(struct mq_html_options_t *options);

/**
 * Sets whether `<script>` contents are extracted as code blocks. Does nothing if `options`
 * is null.
 *
 * # Safety
 *
 * `options` must be null or a pointer returned by `mq_html_options_new`.
 */
void mq_html_options_set_extract_scripts_as_code_blocks(struct mq_html_options_t *options,
                                                        bool value);

/**
 * Sets whether front matter is generated from the HTML head metadata. Does nothing if
 * `options` is null.
 *
 * # Safety
 *
 * `options` must be null or a pointer returned by `mq_html_options_new`.
 */
void mq_html_options_set_generate_front_matter(struct mq_html_options_t *options, bool value);

/**
 * Sets whether the HTML title becomes an H1 heading. Does nothing if `options` is null.
 *
 * # Safety
 *
 * `options` must be null or a pointer returned by `mq_html_options_new`.
 */
void mq_html_options_set_use_title_as_h1(struct mq_html_options_t *options, bool value);

/**
 * Sets the base URL for resolving relative `href`/`src` values. The string is copied. A
 * null, empty or non-UTF-8 `base_url` clears it, so `<base href>` is used instead. Does
 * nothing if `options` is null.
 *
 * # Safety
 *
 * `options` must be null or a pointer returned by `mq_html_options_new`, and `base_url`
 * must be null or a null-terminated C string.
 */
void mq_html_options_set_base_url(struct mq_html_options_t *options, const char *base_url);

/**
 * Converts HTML to Markdown like `mq_html_to_markdown`, with options from
 * `mq_html_options_new` (null for the defaults), reporting failure as a status plus an
 * `mq_error_t`.
 *
 * On success returns `MQ_OK` and stores the Markdown in `*out`, to be freed with
 * `mq_free_string`. On failure stores null in `*out` and, if `err` is non-null, an error
 * in `*err`, to be freed with `mq_error_free`.
 *
 * # Safety
 *
 * `html_input_c` must be a null-terminated C string, `options` must be null or a pointer
 * returned by `mq_html_options_new`, `out` must be valid for writes and `err` must be null
 * or valid for writes.
 */
enum mq_status_t mq_html_to_markdown_ex(const char *html_input_c,
                                        const struct mq_html_options_t *options,
                                        char **out,
                                        struct mq_error_t **err);

/**
 * Evaluates mq code like `mq_eval`, reporting failures as a status plus an `mq_error_t`.
 *
 * On success returns `MQ_OK` and stores the results in `*out`, to be freed with
 * `mq_values_free`. On failure stores null in `*out` and, if `err` is non-null, an error
 * in `*err`, to be freed with `mq_error_free`.
 *
 * # Safety
 *
 * `out` must be valid for writes and `err` must be null or valid for writes. The other
 * arguments are as for `mq_eval`.
 */
enum mq_status_t mq_eval_ex(struct mq_context_t *engine_ptr,
                            const char *code_c,
                            const char *input_c,
                            const char *input_format_c,
                            struct mq_values_t **out,
                            struct mq_error_t **err);

/**
 * Returns the number of results, or 0 if `values` is null.
 *
 * # Safety
 *
 * `values` must be null or a pointer returned by `mq_eval_ex` and not yet freed.
 */
uintptr_t mq_values_len(const struct mq_values_t *values);

/**
 * Returns result `index` rendered as text, the same text `mq_eval` returns. The string is
 * owned by `values` and valid until `mq_values_free`. Returns null if `index` is out of
 * range.
 *
 * # Safety
 *
 * See `mq_values_len`.
 */
const char *mq_value_string(const struct mq_values_t *values, uintptr_t index);

/**
 * Returns the type of result `index`, or `MQ_TYPE_INVALID` if there is no such result.
 *
 * # Safety
 *
 * See `mq_values_len`.
 */
enum mq_type_t mq_value_type(const struct mq_values_t *values, uintptr_t index);

/**
 * Stores a number result in `out` and returns true, or returns false if the result is not
 * a number.
 *
 * # Safety
 *
 * `out` must be null or valid for writes. See also `mq_values_len`.
 */
bool mq_value_number(const struct mq_values_t *values, uintptr_t index, double *out);

/**
 * Stores a boolean result in `out` and returns true, or returns false if the result is not
 * a boolean.
 *
 * # Safety
 *
 * `out` must be null or valid for writes. See also `mq_values_len`.
 */
bool mq_value_bool(const struct mq_values_t *values, uintptr_t index, bool *out);

/**
 * Returns the kind of a markdown result, such as `h1`, `code` or `list`, or null if the
 * result is not a markdown node. Owned by `values`.
 *
 * # Safety
 *
 * See `mq_values_len`.
 */
const char *mq_value_node_kind(const struct mq_values_t *values, uintptr_t index);

/**
 * Stores the input position of a markdown result in `out` and returns true, or returns
 * false if the result has none, for example because it is not a markdown node or was
 * built by the query.
 *
 * # Safety
 *
 * `out` must be null or valid for writes. See also `mq_values_len`.
 */
bool mq_value_position(const struct mq_values_t *values,
                       uintptr_t index,
                       struct mq_position_t *out);

/**
 * Returns result `index` as a JSON string, to be freed with `mq_free_string`, or null if
 * there is no such result. Markdown nodes become objects, bytes base64 strings, and
 * functions `null`. Use `mq_value_type` to tell values apart that share a JSON form.
 *
 * # Safety
 *
 * See `mq_values_len`.
 */
char *mq_value_to_json(const struct mq_values_t *values, uintptr_t index);

/**
 * Returns all results as one JSON array string, to be freed with `mq_free_string`. See
 * `mq_value_to_json`. Returns null if `values` is null.
 *
 * # Safety
 *
 * See `mq_values_len`.
 */
char *mq_values_to_json(const struct mq_values_t *values);

/**
 * Frees results. Does nothing if `values` is null.
 *
 * # Safety
 *
 * `values` must be null or a pointer returned by `mq_eval_ex`, and must not be used
 * afterwards.
 */
void mq_values_free(struct mq_values_t *values);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* MQ_H */
