# MQ Agents

This document provides an overview of the different agents (components) that make up the `mq` project. Each component plays a specific role in the overall functionality of `mq`, from parsing and evaluating mq lang, to providing language server features and web capabilities.

## Crates

These components are Rust crates that form the core functionalities and tools of `mq`.

### `mq-c-api`
- **Purpose**: Provides a C API for the `mq` library.
- **Functionality**: Allows integration of `mq`'s Markdown parsing and processing capabilities into C and C++ applications.

### `mq-cli`
- **Purpose**: Command-line interface for `mq`.
- **Functionality**: Enables users to interact with `mq` from the terminal. (Note: README.md was not found for this component during documentation generation).

### `mq-formatter`
- **Purpose**: Code formatter for `mq` language files.
- **Functionality**: Enforces a consistent code style for `mq` scripts.

### `mq-hir`
- **Purpose**: Implements the High-level Intermediate Representation (HIR).
- **Functionality**: Serves as an intermediate step in the compilation and analysis of `mq` code.

### `mq-lang`
- **Purpose**: Parser and evaluator for the `mq` language.
- **Functionality**: Handles the lexical analysis, parsing, and execution of `mq` scripts.

### `mq-lsp`
- **Purpose**: Language Server Protocol (LSP) implementation for `mq`.
- **Functionality**: Provides IDE features like syntax highlighting, code completion, go-to-definition, and diagnostics for `mq` scripts.

### `mq-macros`
- **Purpose**: Provides procedural macros for `mq`.
- **Functionality**: Offers compile-time validation of `mq` queries embedded in Rust code.

### `mq-markdown`
- **Purpose**: Handles Markdown parsing and conversion.
- **Functionality**: Provides utilities for parsing Markdown content and converting it to other formats like HTML.

### `mq-mcp`
- **Purpose**: Implements Markdown Command Protocol (MCP) functionality.
- **Functionality**: Handles command evaluation and execution against Markdown documents, often used for editor integrations.

### `mq-python`
- **Purpose**: Python bindings for `mq`.
- **Functionality**: Allows Python developers to use `mq`'s Markdown processing capabilities from Python. The package is typically installed as `markdown-query`.

### `mq-repl`
- **Purpose**: REPL (Read-Eval-Print Loop) environment for `mq`.
- **Functionality**: Enables interactive execution and experimentation with `mq` scripts.

### `mq-test`
- **Purpose**: Utility crate for testing `mq` components.
- **Functionality**: Provides tools and helpers for writing tests for the `mq` ecosystem. (Note: README.md was not found for this component during documentation generation).

### `mq-tui`
- **Purpose**: Text-based User Interface (TUI) for `mq`.
- **Functionality**: Offers an interactive terminal interface for querying and manipulating Markdown content.

### `mq-wasm`
- **Purpose**: WebAssembly runtime for `mq`.
- **Functionality**: Enables execution of `mq` scripts in WebAssembly environments, typically browsers.

### `mq-web-api`
- **Purpose**: Web API for `mq`.
- **Functionality**: Likely provides server-side API endpoints for `mq` functionalities. (Note: README.md was not found for this component during documentation generation).

## Packages

These components are typically JavaScript/TypeScript packages, often for web-related functionality.

### `mq-web`
- **Purpose**: Enables `mq` functionality in web environments.
- **Functionality**: A TypeScript/JavaScript package that uses WebAssembly to run `mq` scripts, format `mq` code, and provide diagnostics within web applications or browsers.

### `playground`
- **Purpose**: Provides an interactive web-based environment for `mq`.
- **Functionality**: An online playground for users to experiment with `mq` scripts and see results in real-time, built using web technologies and `mq-wasm`/`mq-web`.
