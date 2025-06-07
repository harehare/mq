# GitHub Copilot Instructions for mq

This file contains instructions for GitHub Copilot for the mq.

## Project Overview

`mq` is a jq-like command-line tool for Markdown processing. Written in Rust, it allows you to easily slice, filter, map, and transform Markdown files.

## Coding Conventions

### Rust Code Conventions

- Always format and validate code using `cargo fmt` and `cargo clippy`
- Add appropriate documentation comments to all public functions, structs, traits, enums, etc.
- Use the `miette` crate for error handling and provide user-friendly error messages
- Avoid panics whenever possible and return appropriate `Result` types
- Write comprehensive tests and update related tests when adding or changing functionality

### Directory Structure

The mq project follows this main directory structure:

- `/crates` - Contains multiple Rust crates
  - `mq-c-api` - C API for integrating mq functionality into C applications
  - `mq-cli` - Implementation of the mq command-line interface
  - `mq-formatter` - Code formatter
  - `mq-hir` - High-level Internal Representation (HIR)
  - `mq-lang` - Implementation of the mq
  - `mq-lsp` - Language Server Protocol implementation
  - `mq-markdown` - Markdown parser and manipulation utilities
  - `mq-mcp` - MCP implementation for mq
  - `mq-python` - Python bindings for integrating mq functionality into Python applications
  - `mq-tui` - Terminal User Interface (TUI) for interacting with mq
  - `mq-wasm` - WebAssembly (Wasm) implementation for running mq in browsers and other WASM environments
- `/docs` - Documentation and user guides
- `/editors` - Editor integrations and plugins for popular code editors
- `/assets` - Static assets such as images, icons, and other resources
- `/examples` - Usage examples
- `/tests` - Integration tests
- `/scripts` - Scripts for automation tasks
- `/packages` - Contains various packages for different functionalities
  - `mq-web` - npm package for using mq in web applications and JavaScript environments
  - `playground` - A playground for developing and testing for mq

## Pull Request Review Criteria

When creating a pull request, please ensure:

1. All tests pass
2. Code coverage is maintained at existing levels (check with Codecov)
3. Code is formatted and passes lint checks
4. Appropriate documentation is added/updated
5. Changes are recorded in `CHANGELOG.md`

## Commit Message Conventions

Use the following format for commit messages:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types include:

- ✨ feat: New feature
- 🐛 fix: Bug fix
- 📝 docs: Documentation changes
- 💄 style: Code style changes that don't affect behavior
- ♻️ refactor: Refactoring
- ⚡ perf: Performance improvements
- ✅ test: Adding or modifying tests
- 📦 build: Changes to build system or external dependencies
- 👷 ci: Changes to CI configuration files and scripts

## Documentation

When adding new features, update the following documentation:

1. Documentation comments in the relevant source files
2. Related documentation in the `/docs` directory
3. `README.md` as needed

## Feature Requests

When proposing feature additions to `mq`, include:

1. A description of the use case
2. Examples of the proposed syntax and behavior
3. Relationship to existing features

## Bug Reports

When reporting bugs, provide:

1. A detailed description of the issue
2. Steps to reproduce
3. Expected behavior vs. actual behavior
4. If possible, Markdown and `mq` query examples that reproduce the issue

## License

This project is provided under the MIT License. Please ensure all contributions are compatible with this license.
