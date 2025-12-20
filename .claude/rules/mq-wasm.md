---
paths: crates/mq-wasm/**
---

# mq-wasm Rules

## Purpose

WebAssembly (Wasm) implementation for running mq in browsers and other WASM environments.

## Coding Rules

- Use `wasm-bindgen` for JavaScript interop
- Provide JavaScript-friendly APIs with appropriate type conversions
- Keep wasm bundle size minimal; use wasm-opt for optimization
- Document all exported functions and types
- Handle errors gracefully; convert to JavaScript errors
- Write tests for both Rust and JavaScript sides
- Provide clear examples of usage in web contexts
- Support both browser and Node.js environments where appropriate
- Avoid panics; use proper error handling
- Test with various browsers and JavaScript runtimes
- Document memory management and cleanup
- Provide TypeScript type definitions
- Keep the API surface minimal and focused
- Optimize for bundle size and performance
- Document any limitations compared to native implementation
