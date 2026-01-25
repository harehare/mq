# Contributing to mq

Thank you for your interest in contributing to mq! This document provides guidelines for contributing to the project.

## Getting Started

Before contributing, please:

1. Read the [README.md](README.md) to understand the project
2. Check the [documentation](https://mqlang.org/book/) for detailed usage information
3. Browse existing [issues](https://github.com/harehare/mq/issues) to see what's already being worked on
4. Look at recent [pull requests](https://github.com/harehare/mq/pulls) to understand the contribution process

## Development Setup

### Prerequisites

- Rust 1.93 or later
- Git
- [just](https://github.com/casey/just) command runner

### Setting up the Development Environment

1. Fork the repository on GitHub
2. Clone your fork locally:

   ```bash
   git clone https://github.com/YOUR_USERNAME/mq.git
   cd mq
   ```

3. Install dependencies:

   ```bash
   cargo build
   ```

4. Run tests to ensure everything is working:

   ```bash
   just test
   ```

## Code Style and Standards

### Rust Conventions

- **Formatting**: All code must be formatted with `cargo fmt`
- **Linting**: Code must pass `cargo clippy` without warnings
- **Documentation**: Add appropriate doc comments to all public functions, structs, traits, and enums
- **Error Handling**: Use the `miette` crate for error handling and provide user-friendly error messages
- **No Panics**: Avoid panics whenever possible; return appropriate `Result` types instead
- **Testing**: Write comprehensive tests for new functionality

### Code Organization

- Each crate should have a clear, focused purpose
- Use `pub(crate)` visibility unless wider exposure is necessary
- Organize code into logical modules; avoid large, monolithic files
- Keep dependencies minimal and up-to-date

## Testing

### Running Tests

**Always run the full test suite before submitting changes:**

```bash
just test
```

### Test Guidelines

- Write comprehensive tests for all new features and bug fixes
- Use descriptive names for test functions and modules
- Prefer table-driven tests for similar input/output patterns
- Use `assert_eq!`, `assert!`, and custom error messages for clarity
- Keep tests fast and isolated
- Place integration tests in the `tests/` directory, unit tests alongside implementation

### Test Coverage

Run tests with coverage reporting:

```bash
just test-cov
```

## Submitting Changes

### Before Submitting

1. **Ensure tests pass**: Run `just test` and fix any failures
2. **Update documentation**: Add or update documentation for new features
3. **Follow commit conventions**: Use clear, descriptive commit messages

### Documentation Requirements

- Update `/docs` and crate-level `README.md` files for new features

### Building Documentation

```bash
just docs
```

## Bug Reports

When reporting bugs, please provide:

1. A detailed description of the issue
2. Steps to reproduce the problem
3. Expected behavior vs. actual behavior
4. Markdown and `mq` query examples that reproduce the issue
5. Your environment details (OS, Rust version, mq version)

## Feature Requests

When proposing new features, please include:

1. A clear description of the use case
2. Examples of the proposed syntax and behavior
3. How it relates to existing features
4. Potential implementation approach (if applicable)

## License

This project is licensed under the MIT License. By contributing, you agree that your contributions will be licensed under the same license.

Thank you for contributing to mq! 🚀
