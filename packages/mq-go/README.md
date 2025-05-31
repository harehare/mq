# mq-go: Go Bindings for mq-lang

This package provides Go bindings for the `mq-lang` library through a C API.

## Prerequisites

- Go compiler (with CGo enabled, which is default)
- Rust compiler and Cargo (to build the `mq-c-api` library)

## Building

1.  **Build the Rust C-API library:**

    Navigate to the root of the `mq` monorepo. Then run:
    ```bash
    cargo build --package mq-c-api
    ```
    For a release build (recommended for performance):
    ```bash
    cargo build --package mq-c-api --release
    ```
    This will produce a dynamic library (e.g., `libmq_c_api.so` on Linux, `libmq_c_api.dylib` on macOS) and/or a static library (e.g., `libmq_c_api.a`) in the `target/debug` or `target/release` directory at the workspace root.

2.  **Adjust CGo flags in `mq.go` (if necessary):**

    The `mq.go` file contains CGo directives:
    ```go
    /*
    #cgo CFLAGS: -I../../crates/mq-c-api/include -I../../target/debug -I../../target/release
    // For dynamic linking (adjust path and library name as needed):
    // #cgo LDFLAGS: -L../../target/debug -lmq_c_api
    // For static linking (adjust path and library name as needed):
    // #cgo LDFLAGS: ../../target/debug/libmq_c_api.a
    */
    ```
    The `CFLAGS` are set to help find headers or libraries. The `LDFLAGS` tell CGo where to find the compiled `mq-c-api` library and how to link it.
    - The path `-L../../target/debug` (or `../../target/release`) assumes `mq.go` is in `packages/mq-go/` and the `target` directory is at the workspace root.
    - `-lmq_c_api` links against `libmq_c_api.so` or `libmq_c_api.dylib`.
    - `../../target/debug/libmq_c_api.a` links against the static library.

    You might need to uncomment and adjust one of the `LDFLAGS` lines depending on your operating system and whether you built in debug or release mode, and whether you prefer static or dynamic linking.

    Alternatively, instead of modifying the file, you can provide these flags via environment variables when building/running your Go code.

3.  **Run the example Go code:**

    The `mq.go` file contains a `main` function for demonstration. To run it:

    *   **Using Dynamic Linking (Linux Example):**
        Make sure `libmq_c_api.so` is in `../../target/debug/`.
        ```bash
        # From the packages/mq-go directory
        CGO_LDFLAGS="-L../../target/debug -lmq_c_api" go run mq.go
        ```
        On macOS, it would be `-lmq_c_api` for `libmq_c_api.dylib`. You might also need to set `LD_LIBRARY_PATH` (Linux) or `DYLD_LIBRARY_PATH` (macOS) if the library is not in a standard location and you are not using an rpath, or if you are running a compiled binary later:
        ```bash
        export LD_LIBRARY_PATH=../../target/debug:$LD_LIBRARY_PATH # For Linux
        export DYLD_LIBRARY_PATH=../../target/debug:$DYLD_LIBRARY_PATH # For macOS
        # Then run:
        go run mq.go
        # (Or build then run: go build -o myapp && ./myapp)
        ```

    *   **Using Static Linking (Example):**
        Make sure `libmq_c_api.a` is in `../../target/debug/`.
        ```bash
        # From the packages/mq-go directory
        CGO_LDFLAGS_ALLOW_ALL=1 CGO_LDFLAGS="../../target/debug/libmq_c_api.a" go run mq.go
        ```
        Note: Static linking might require other system libraries to be linked as well, depending on the Rust code's dependencies. The exact command might vary. `CGO_LDFLAGS_ALLOW_ALL=1` might be needed for more complex linker flags.

    For a release build, replace `debug` with `release` in the paths.

## Usage in your Go project

1.  Ensure the `mq-c-api` library (`.so`, `.dylib`, or `.a`) is built and accessible.
2.  Import the `mqgo` package.
3.  Set up CGo linking flags either in your Go files or via environment variables.

```go
import "path/to/your/mq/packages/mq-go" // Adjust import path

func main() {
    engine, err := mqgo.NewEngine()
    if err != nil {
        // handle error
        return
    }
    defer engine.Close()

    results, err := engine.Eval("your_mq_code", "your_input_data", "text") // or "markdown"
    if err != nil {
        // handle error
        return
    }
    // use results
}

```

## Notes
*   The `input_format` in `Eval` currently supports `"text"` and `"markdown"`.
*   Memory management: `NewEngine` allocates an engine that must be freed with `engine.Close()`. The `Eval` function handles freeing memory for results and errors from the C API.
