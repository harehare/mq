# mq-ffi

C API for the [mq](https://mqlang.org/) Markdown processing library. `mq.h` can be included from both C and C++.

## Installation

```bash
git clone https://github.com/harehare/mq
cd mq/crates/mq-ffi
cargo build --profile release-ffi
```

The library is built at:
- **Static**: `target/release-ffi/libmq_ffi.a`
- **Dynamic**: `target/release-ffi/libmq_ffi.so` (Linux), `.dylib` (macOS) or `.dll` (Windows)

> [!IMPORTANT]
> Build with `--profile release-ffi`, not `--release`. The default `release` profile uses `panic = 'abort'`, so a panic inside mq would abort the host process instead of being reported as an error. `mq_is_panic_safe()` returns `false` for such a build.

## Quick Start

`mq_eval` returns every result as a string.

```c
#include <stdio.h>
#include "mq.h"

int main(void) {
    mq_context_t *ctx = mq_create();
    if (ctx == NULL) return 1;

    mq_result_t result = mq_eval(ctx, ".h | to_text", "# Title\n\n## Section\n", "markdown");

    if (result.error_msg != NULL) {
        fprintf(stderr, "Error: %s\n", result.error_msg);
    } else {
        for (size_t i = 0; i < result.values_len; i++) {
            printf("%s\n", result.values[i]);
        }
    }

    mq_free_result(result);
    mq_destroy(ctx);
    return 0;
}
```

## Choosing between `mq_eval` and `mq_eval_ex`

| | `mq_eval` | `mq_eval_ex` |
| --- | --- | --- |
| Use it to | print or concatenate result text | branch on result types, report error positions, get JSON |
| Results | strings | typed values (`mq_values_t`) |
| Errors | one message string | status code and details (`mq_error_t`) |
| Free with | `mq_free_result` | `mq_values_free` and `mq_error_free` |

`mq_eval` is not deprecated and its behavior does not change. New features are added to `mq_eval_ex` only.

## Using `mq_eval_ex`

```c
mq_values_t *values = NULL;
mq_error_t *err = NULL;

if (mq_eval_ex(ctx, query, input, "markdown", &values, &err) != MQ_OK) {
    fprintf(stderr, "%s\n", mq_error_rendered(err));  // message with the source excerpt

    mq_location_t at;
    if (mq_error_location(err, &at)) {
        fprintf(stderr, "at %u:%u\n", at.line, at.column);
    }
    mq_error_free(err);
    return 1;
}

for (size_t i = 0; i < mq_values_len(values); i++) {
    switch (mq_value_type(values, i)) {
    case MQ_TYPE_NUMBER: {
        double n;
        mq_value_number(values, i, &n);
        break;
    }
    case MQ_TYPE_MARKDOWN: {
        const char *kind = mq_value_node_kind(values, i);  // "h1", "code", ...
        mq_position_t pos;
        if (mq_value_position(values, i, &pos)) { /* ... */ }
        break;
    }
    default:
        printf("%s\n", mq_value_string(values, i));
    }
}

char *json = mq_values_to_json(values);  // all results as a JSON array
mq_free_string(json);
mq_values_free(values);
```

- `err` may be `NULL` when only the status is needed.
- Strings from `mq_error_*`, `mq_value_string` and `mq_value_node_kind` belong to their handle and are valid until it is freed.
- Use `mq_value_type` to tell values apart that share a JSON form: a dict and a markdown node are both objects, and a string and a symbol are both strings.
- `mq_value_to_json` returns one result as JSON. Read arrays and dicts through it.
- Error lines and columns start at 1 and columns count characters. Node positions also start at 1 but count UTF-16 code units.

| Status | Meaning |
| --- | --- |
| `MQ_ERR_INVALID_ARGUMENT` | Null pointer, invalid UTF-8 or unsupported input format |
| `MQ_ERR_INPUT_PARSE` | The input could not be parsed |
| `MQ_ERR_SYNTAX` | Syntax error in the query |
| `MQ_ERR_RUNTIME` | The query failed while running |
| `MQ_ERR_MODULE` | A module could not be resolved or loaded |
| `MQ_ERR_TIMEOUT` | `mq_set_timeout_ms` was exceeded |
| `MQ_ERR_HALT` | The query called `halt`; see `mq_error_exit_code` |
| `MQ_ERR_UNSUPPORTED` | The library was built without a required feature |
| `MQ_ERR_INTERNAL` | A panic was caught at the API boundary |

New statuses and types may be added, so keep a `default` branch.

## Other `_ex` Functions

They report failure the same way as `mq_eval_ex`: a `mq_status_t` return value and an optional `mq_error_t **err`.

```c
mq_status_t mq_engine_new(mq_context_t **out, mq_error_t **err);

mq_status_t mq_import_module_ex(mq_context_t *ctx, const char *name, mq_error_t **err);
mq_status_t mq_load_module_ex(mq_context_t *ctx, const char *name, mq_error_t **err);
mq_status_t mq_clear_http_cache_ex(mq_context_t *ctx, mq_error_t **err);
mq_status_t mq_clear_http_cache_all_ex(mq_context_t *ctx, mq_error_t **err);
```

HTML to Markdown takes its options through an opaque object, so options can be added without breaking the ABI. `options` may be `NULL` for the defaults.

```c
mq_html_options_t *options = mq_html_options_new();
mq_html_options_set_use_title_as_h1(options, true);
mq_html_options_set_base_url(options, "https://example.com");

char *markdown = NULL;
if (mq_html_to_markdown_ex(html, options, &markdown, &err) == MQ_OK) {
    printf("%s\n", markdown);
    mq_free_string(markdown);
}
mq_html_options_free(options);
```

`mq_html_to_markdown` and `MqConversionOptions` keep working but will not gain new options.

## Running Untrusted Queries

```c
mq_set_max_call_stack_depth(ctx, 256);  // bound recursion
mq_set_timeout_ms(ctx, 1000);           // bound wall-clock time per evaluation, MQ_NO_TIMEOUT disables
```

Result strings are NUL-terminated, so a `\0` in a value is replaced with U+FFFD.

## Input Formats

The format name is case-insensitive.

| Format | Input |
| --- | --- |
| `"markdown"` | Markdown (CommonMark, GFM) |
| `"mdx"` | MDX (Markdown with JSX) |
| `"html"` | HTML, converted to Markdown |
| `"text"` | Plain text, one value per line |
| `"raw"` | The whole input as one string |
| `"null"` | No input |
| `"csv"`, `"tsv"`, `"psv"` | Delimited text with a header row |
| `"json"`, `"jsonl"`, `"yaml"`, `"toml"`, `"toon"`, `"xml"`, `"gron"` | Parsed by the matching module |

`"bytes"` and `"cbor"` are CLI only, since the input is a C string.

## Support

- [Report bugs or request features](https://github.com/harehare/mq/issues)
- [Documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
