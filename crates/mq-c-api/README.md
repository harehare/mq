# mq-c-api

A C API for the mq Markdown processing library, providing a C-compatible interface for integrating mq functionality into C and C++ applications.

## Overview

`mq-c-api` exposes the core functionality of the mq Markdown processing tool through a C API, allowing developers to use mq's powerful Markdown manipulation capabilities in C/C++ projects.

## Features

- C-compatible API for mq's Markdown parsing and processing
- Memory-safe interface with proper resource management
- Support for mq query language execution
- Cross-platform compatibility

## Usage

Include the header file and link against the library:

```c
#include "mq.h"

int main() {
    // Initialize mq context
    MqContext* ctx = mq_create();

    // Process Markdown with mq query
    const char* markdown = "# Hello World\n\nThis is a test.";
    const char* query = ".";

    MqResult* result = mq_exec(ctx, markdown, query);
    if (result) {
        printf("%s\n", result);
        mq_free_string(result);
    }

    // Clean up
    mq_context_free(ctx);
    return 0;
}
```

## Building

The C API is built as part of the main mq project:

```bash
cargo build --release
```

This generates both static and dynamic libraries that can be linked with C/C++ projects.

## Memory Management

The API follows standard C conventions for memory management:

- Functions that return strings allocate memory that must be freed with `mq_free_string()`
- Context objects must be freed with `mq_context_free()`
- Input parameters are not modified and do not need to be freed by the library

## Error Handling

Functions return `NULL` or error codes to indicate failures. Check return values and handle errors appropriately in your application.

## License

Licensed under the MIT License. See the main project LICENSE file for details.
