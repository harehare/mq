# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`mq` is a jq-like command-line tool for Markdown processing written in Rust. It allows users to slice, filter, map, and transform Markdown files using a familiar syntax similar to jq but designed specifically for Markdown documents.

## Development Commands

### Build and Test
```bash
# Build the project in release mode
just build

# Run all tests (includes formatting and linting)
just test

# Run tests with coverage
just test-cov

# Run formatting and linting only
just lint

# Check formatting
cargo fmt --all -- --check

# Run clippy linter
cargo clippy --all-targets --all-features --workspace -- -D clippy::all
```

### Running the CLI
```bash
# Run CLI with arguments
just run [ARGS...]

# Or directly with cargo
cargo run -- [ARGS...]

# Start REPL
cargo run -- repl

# Start TUI
cargo run -- tui
```

### Development Tools
```bash
# Start web playground development server
just playground

# Build WebAssembly package
just build-wasm

# Run benchmarks
just bench

# Check for unused dependencies
just deps

# Update documentation
just docs
```

## Architecture Overview

This is a multi-crate Rust workspace with the following key components:

### Core Language Infrastructure
- **mq-lang**: Core language implementation with parser, evaluator, and engine
- **mq-hir**: High-level Intermediate Representation for code analysis and tooling
- **mq-markdown**: Markdown parsing and manipulation utilities

### User Interfaces
- **mq-cli**: Main command-line interface
- **mq-repl**: Interactive REPL environment  
- **mq-tui**: Terminal User Interface
- **mq-lsp**: Language Server Protocol implementation for editor support

### Language Tooling
- **mq-formatter**: Code formatter for mq files
- **mq-test**: Testing utilities

### Integration APIs
- **mq-c-api**: C API for integration into C applications
- **mq-python**: Python bindings via PyO3/maturin
- **mq-wasm**: WebAssembly bindings for browser usage
- **mq-web-api**: Web API implementation
- **mq-mcp**: Model Context Protocol implementation

### Processing Pipeline
1. **Lexing**: Raw text → tokens (`mq-lang/lexer`)
2. **Parsing**: Tokens → AST/CST (`mq-lang/ast`, `mq-lang/cst`)  
3. **HIR**: AST → High-level IR for analysis (`mq-hir`)
4. **Evaluation**: HIR → runtime values (`mq-lang/eval`)
5. **Output**: Values → formatted output (`mq-formatter`)

The engine (`mq-lang/engine.rs`) orchestrates this pipeline, with the evaluator handling runtime execution and the optimizer improving performance.

## Code Conventions

### Error Handling
- Use `miette` crate for user-friendly error messages
- Avoid panics; return `Result` types
- Provide contextual error information

### Testing
- Write comprehensive tests for new functionality
- Update related tests when modifying existing code
- Use `just test` to run full test suite including lint checks

### Documentation
- Add documentation comments to all public functions, structs, traits, enums
- Update `/docs` directory for user-facing features
- Use `just docs` to regenerate documentation

### Rust Toolchain
- Uses Rust 1.86.0 (specified in `rust-toolchain.toml`)
- Format with `cargo fmt`
- Lint with `cargo clippy`
- Use workspace dependencies defined in root `Cargo.toml`

## Development Workflow

1. Make changes to relevant crates
2. Run `just test` to ensure formatting, linting, and tests pass
3. Build with `just build` to verify release compilation
4. Test CLI functionality with `just run [args]`
5. Use `just test-cov` to check code coverage if needed

The project uses `justfile` for task automation - prefer `just` commands over direct `cargo` commands when available.