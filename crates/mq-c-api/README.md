# mq-c-api

C-compatible API for the [mq](https://mqlang.org/) Markdown processing library, enabling seamless integration of mq's querying capabilities into C and C++ applications.

## Installation

### Building from Source

```bash
git clone https://github.com/harehare/mq
cd mq/crates/mq-c-api
cargo build --release
```

The compiled library will be available at:
- **Static library**: `target/release/libmq_c_api.a`
- **Dynamic library**: `target/release/libmq_c_api.so` (Linux) or `.dylib` (macOS) or `.dll` (Windows)

## Usage

### Basic Example

```c
#include <stdio.h>
#include <string.h>
#include "mq.h"

int main() {
    // Create mq context
    mq_context_t *ctx = mq_create();
    if (ctx == NULL) {
        fprintf(stderr, "Failed to create mq context\n");
        return 1;
    }

    // Sample markdown content
    const char *markdown_content =
        "# My Document\n\n"
        "This is an introduction paragraph.\n\n"
        "## Features\n\n"
        "- Easy to use\n"
        "- Fast processing\n"
        "- Cross-platform\n\n";

    // Execute query
    mq_result_t result = mq_eval(ctx, ".h | to_text()", markdown_content, "markdown");

    // Check for errors
    if (result.error_msg != NULL) {
        printf("Error: %s\n", result.error_msg);
    } else {
        // Print results
        for (size_t i = 0; i < result.values_len; i++) {
            if (strlen(result.values[i]) > 0) {
                printf("  - %s\n", result.values[i]);
            }
        }
    }

    // Clean up
    mq_free_result(result);
    mq_destroy(ctx);

    return 0;
}
```

### Extracting Code Blocks

```c
#include "mq.h"

void extract_code_blocks(const char *markdown) {
    mq_context_t *ctx = mq_create();
    if (!ctx) return;

    mq_result_t result = mq_eval(ctx, ".code | to_text()", markdown, "markdown");

    if (result.error_msg == NULL) {
        for (size_t i = 0; i < result.values_len; i++) {
            printf("Code block %zu:\n%s\n\n", i + 1, result.values[i]);
        }
    }

    mq_free_result(result);
    mq_destroy(ctx);
}
```

### Processing HTML

```c
#include "mq.h"

void extract_headings_from_html(const char *html) {
    mq_context_t *ctx = mq_create();
    if (!ctx) return;

    // Note: input format is case-insensitive
    mq_result_t result = mq_eval(ctx, ".h", html, "HTML");

    if (result.error_msg == NULL) {
        for (size_t i = 0; i < result.values_len; i++) {
            printf("%s\n", result.values[i]);
        }
    }

    mq_free_result(result);
    mq_destroy(ctx);
}
```

### Filtering List Items

```c
#include "mq.h"

void filter_list_items(const char *markdown, const char *keyword) {
    mq_context_t *ctx = mq_create();
    if (!ctx) return;

    char query[256];
    snprintf(query, sizeof(query), ".[] | select(contains(\"%s\")) | to_text()", keyword);

    mq_result_t result = mq_eval(ctx, query, markdown, "markdown");

    if (result.error_msg == NULL) {
        printf("List items containing '%s':\n", keyword);
        for (size_t i = 0; i < result.values_len; i++) {
            printf("  - %s\n", result.values[i]);
        }
    }

    mq_free_result(result);
    mq_destroy(ctx);
}
```

## API Reference

### Context Management

```c
// Create a new mq context
mq_context_t* mq_create(void);

// Destroy an mq context and free resources
void mq_destroy(mq_context_t* ctx);
```

### Query Execution

```c
// Evaluate an mq query against markdown content
// Parameters:
//   ctx: The mq context
//   query: The mq query string
//   input: The input content
//   input_format: Input format ("markdown", "mdx", "html", "text" - case-insensitive)
// Returns: Result containing values or error message
mq_result_t mq_eval(
    mq_context_t* ctx,
    const char* query,
    const char* input,
    const char* input_format
);
```

### Result Handling

```c
// Result structure
typedef struct {
    char** values;        // Array of result strings
    size_t values_len;    // Number of results
    char* error_msg;      // Error message (NULL if no error)
} mq_result_t;

// Free result memory
void mq_free_result(mq_result_t result);

// Free a string returned by mq
void mq_free_string(char* str);
```

## Error Handling

Functions return `NULL` or set error messages to indicate failures:

```c
mq_context_t *ctx = mq_create();
if (ctx == NULL) {
    // Handle context creation failure
}

mq_result_t result = mq_eval(ctx, query, input, format);
if (result.error_msg != NULL) {
    // Handle query execution error
    fprintf(stderr, "Query error: %s\n", result.error_msg);
}

// Always clean up, even after errors
mq_free_result(result);
mq_destroy(ctx);
```

## Supported Input Formats

| Format       | Description          | Example                          |
| ------------ | -------------------- | -------------------------------- |
| `"markdown"` | Standard Markdown    | CommonMark, GFM                  |
| `"mdx"`      | MDX (Markdown + JSX) | React components in Markdown     |
| `"html"`     | HTML documents       | Converted to Markdown internally |
| `"text"`     | Plain text           | Treated as single paragraph      |

**Note**: Format strings are case-insensitive (`"markdown"`, `"MARKDOWN"`, and `"Markdown"` are equivalent).

### Direct Compilation

```bash
# Linux
gcc -o example main.c -L./target/release -lmq_c_api -lpthread -ldl -lm

# macOS
clang -o example main.c -L./target/release -lmq_c_api

# Windows (MSVC)
cl.exe main.c /link mq_c_api.lib
```

## Thread Safety

The C API is **not thread-safe**. Each thread should have its own `mq_context_t`:

```c
// ❌ BAD: Sharing context between threads
mq_context_t *shared_ctx = mq_create();
// ... use from multiple threads ...

// ✅ GOOD: Each thread has its own context
void* thread_func(void* arg) {
    mq_context_t *ctx = mq_create();
    // ... use ctx ...
    mq_destroy(ctx);
    return NULL;
}
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
