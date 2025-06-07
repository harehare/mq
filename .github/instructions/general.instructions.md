---
applyTo: "**"
---

# General Coding Conventions for mq

- Follow Rust best practices and idioms for all Rust code.
- Use meaningful variable, function, and type names.
- Write concise, clear, and specific comments and documentation.
- Avoid magic numbers; use named constants.
- Prioritize code readability and maintainability.
- All code must be formatted with `cargo fmt` and pass `cargo clippy`.
- Use the `miette` crate for error handling and provide user-friendly error messages.
- Avoid panics; return appropriate `Result` types instead.
- All public items (functions, structs, traits, enums, etc.) must have documentation comments.
- Update or add tests for all new or changed functionality.
- All changes must be reflected in `CHANGELOG.md`.
- Ensure all contributions are compatible with the MIT License.

## Directory Structure

The mq project follows this main directory structure:

- `/src` - Main CLI tool source code
- `/crates` - Contains multiple Rust crates
  - `mq-lang` - Implementation of the mq language
  - `mq-hir` - High-level Internal Representation (HIR)
  - `mq-formatter` - Code formatter
  - `mq-lsp` - Language Server Protocol implementation
  - `mq-markdown` - Markdown parser and manipulation utilities
  - `mq-wasm` - WebAssembly (Wasm) implementation for running mq in browsers and other WASM environments
  - `mq-mcp` - MCP implementation for mq
  - `mq-python` - Python bindings for integrating mq functionality into Python applications
- `/docs` - Documentation and user guides
- `/editors` - Editor integrations and plugins for popular code editors
- `/assets` - Static assets such as images, icons, and other resources
- `/examples` - Usage examples
- `/tests` - Integration tests

## Feature Requests

When proposing feature additions to `mq`, please include the following information:

1. A description of the use case
2. Examples of the proposed syntax and behavior
3. Relationship to existing features

## Bug Reports

When reporting bugs, provide the following information:

1. A detailed description of the issue
2. Steps to reproduce
3. Expected behavior vs. actual behavior
4. If possible, Markdown and `mq` query examples that reproduce the issue

## License

This project is provided under the MIT License. All contributions must be compatible with this license. Please include the license header in new files as appropriate.
