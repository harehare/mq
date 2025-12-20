---
paths: crates/mq-ffi/**
---

# mq-ffi Rules

## Purpose

Foreign Function Interface for integrating mq with other programming languages.

## Coding Rules

- Provide safe and idiomatic bindings for target languages
- Use proper memory management and document ownership semantics
- Export only necessary symbols; keep internal functions private
- Provide comprehensive documentation for all public APIs
- Include safety checks for invalid parameters and edge cases
- Use `miette` for error handling on the Rust side
- Convert errors to language-appropriate error types
- Write tests in both Rust and target languages where applicable
- Document memory allocation/deallocation responsibilities clearly
- Ensure thread safety or document thread safety requirements
- Provide example usage in documentation
- Keep the API surface minimal and focused
- Consider ABI stability when making changes
- Use appropriate FFI patterns for each target language
- Handle null pointers and invalid data gracefully
