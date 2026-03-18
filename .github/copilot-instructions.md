# mq Development Guide

## Project Overview

`mq` is a jq-like command-line tool for Markdown processing. Written in Rust, it allows you to easily slice, filter, map, and transform Markdown files.

## Coding Conventions

### Rust Code Conventions

- Always format and validate code using `cargo fmt` and `cargo clippy`
- Add appropriate documentation comments to all public functions, structs, traits, enums, etc.
- Use the `miette` crate for error handling and provide user-friendly error messages
- Avoid panics whenever possible and return appropriate `Result` types
- Write comprehensive tests and update related tests when adding or changing functionality

## Commit Message Conventions

Use the following format for commit messages:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

- Types include:
  - ✨ feat: New feature
  - 🐛 fix: Bug fix
  - 📝 docs: Documentation changes
  - 💄 style: Code style changes that don't affect behavior
  - ♻️ refactor: Refactoring
  - ⚡ perf: Performance improvements
  - ✅ test: Adding or modifying tests
  - 📦 build: Changes to build system or external dependencies
  - 👷 ci: Changes to CI configuration files and scripts
- Write clear, concise, and descriptive commit messages.
- Reference related issues or pull requests when relevant.

## Documentation Guidelines

When adding new features, update the documentation.

- Keep documentation up-to-date with code changes.
- Use clear, concise language and provide usage examples.
- Document all public APIs, commands, and features.
- Update `/docs` and crate-level `README.md` files for new features or changes.
- Add changelog entries for all user-facing changes.
- Ensure documentation is consistent across all files and crates.
- Use Markdown best practices for formatting and structure.

## Testing Conventions

- Write comprehensive tests for all new features and bug fixes.
- Use descriptive names for test functions and modules.
- Prefer table-driven tests for similar input/output patterns.
- Use `assert_eq!`, `assert!`, and custom error messages for clarity.
- Avoid flaky or timing-dependent tests.
- Place integration tests in the `tests/` directory and unit tests alongside implementation.
- Mock external dependencies where possible.
- Keep tests fast and isolated.
- Update or add tests when changing existing code.

Use `just test` to run tests instead of `cargo test`.

## Markdown Parser/Utility Coding Rules

- All Markdown parsing and manipulation logic must reside in `mq-markdown`.
- Write tests for all parsing and transformation functions.
- Ensure robust handling of edge cases in Markdown syntax.
- Document all public APIs and provide usage examples in doc comments.
- Avoid panics on malformed input; return descriptive errors using `miette`.
- Keep the API surface minimal and focused on Markdown processing.

## Rust Crate Coding Rules for mq

- Each crate must have a clear purpose and be documented in its `README.md` (if present).
- Organize code into logical modules; avoid large, monolithic files.
- Use `pub(crate)` or tighter visibility unless wider exposure is necessary.
- Prefer explicit error types using `miette` for user-facing errors.
- Write comprehensive unit and integration tests in each crate.
- Document all public APIs with Rust doc comments.
- Avoid unsafe code unless absolutely necessary; document all unsafe blocks.
- Use feature flags for optional functionality.
- Keep dependencies minimal and up-to-date.

## CLI Tool Coding Rules

- All command-line interface logic must reside in `mq-run`.
- Use `clap` or similar crate for argument parsing.
- Provide clear, user-friendly error messages using `miette`.
- Document all commands, flags, and options in code and in the CLI help output.
- Write integration tests for CLI behavior and edge cases.
- Ensure the CLI is robust against malformed input and unexpected usage.
- Output should be clear and suitable for piping/automation.

## License

This project is provided under the MIT License. Please ensure all contributions are compatible with this license.
