# mq-wasm

WebAssembly bindings for mq Markdown processing.

## Overview

`mq-wasm` provides WebAssembly bindings for the [mq](https://github.com/harehare/mq) Markdown query and transformation tool, enabling mq to run in web browsers and other WebAssembly environments.

## Features

- **Browser Support**: Run mq queries directly in web browsers
- **Full mq Functionality**: Access to the complete mq query language
- **OPFS Integration**: File system access via Origin Private File System
- **Async Operations**: Support for asynchronous operations in WASM

## Usage

This crate is primarily used through the npm package `@mq-lang/mq-web`. For JavaScript/TypeScript usage, see the [mq-web package](../../packages/mq-web).

### Online Playground

Try mq in your browser at the [Online Playground](https://mqlang.org/playground), which is powered by this WebAssembly implementation.

## Building

```bash
wasm-pack build --target web
```

## License

Licensed under the MIT License.
