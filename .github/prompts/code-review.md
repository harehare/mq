---
description: "Performs code review for the specified files or directories."
---

# Code Review (for Copilot)

Performs a comprehensive code review for the mq project, focusing on quality, performance, and adherence to project conventions.

## Usage

- If arguments are provided, review only the specified files or directories
- If no arguments, review the entire codebase
- Supports glob patterns and multiple paths

## What it does

- Analyzes Rust code for performance and optimization
- Reviews code for mq conventions and best practices
- Checks error handling with miette
- Validates documentation for public APIs
- Identifies security issues and anti-patterns
- Suggests improvements for maintainability and readability

## Example

/code-review crates/mq-lang/src/
/code-review src/lexer.rs src/parser.rs
/code-review "crates/*/src/*.rs"
