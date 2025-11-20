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

````c
#include <stdio.h>
#include <string.h>
#include "mq.h"

int main()
{
    mq_context_t *ctx = mq_create();
    if (ctx == NULL)
    {
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
        "- Cross-platform\n\n"
        "## Installation\n\n"
        "Run the following command:\n\n"
        "```bash\n"
        "cargo install mq\n"
        "```\n\n"
        "That's it!\n";

    mq_result_t result = mq_eval(ctx, ".h | to_text()", markdown_content, "markdown");

    if (result.error_msg != NULL)
    {
        printf("Error: %s\n", result.error_msg);
    }
    else
    {
        for (size_t i = 0; i < result.values_len; i++)
        {
            if (strlen(result.values[i]) > 0)
            {
                printf("  - %s\n", result.values[i]);
            }
        }
    }
    printf("\n");

    // Clean up
    mq_free_result(result);
    mq_destroy(ctx);

    return 0;
}
````

## Memory Management

The API follows standard C conventions for memory management:

- Functions that return strings allocate memory that must be freed with `mq_free_string()`
- Context objects must be freed with `mq_context_free()`
- Input parameters are not modified and do not need to be freed by the library

## Error Handling

Functions return `NULL` or error codes to indicate failures. Check return values and handle errors appropriately in your application.

## License

Licensed under the MIT License.
