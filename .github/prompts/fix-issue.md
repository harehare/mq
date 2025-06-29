---
description: "Analyzes and fixes issues in the mq codebase based on bug reports or error descriptions."
---

# Fix Issue (for Copilot)

Analyzes and fixes issues in the mq codebase based on bug reports, error descriptions, or failing tests.

## Usage

- If arguments contain error messages, analyze and fix the specific error
- If arguments contain file paths, focus on those files
- If arguments contain issue descriptions, resolve the described problem
- Supports multiple descriptions or file paths

## What it does

- Investigates issues and identifies root causes
- Fixes bugs following mq conventions and best practices
- Ensures error handling uses miette
- Runs tests to verify fixes
- Formats and validates code with cargo fmt and clippy
- Updates documentation if public APIs are affected

## Example

/fix-issue "The CLI tool panics when processing malformed Markdown files"
/fix-issue "Error: thread 'main' panicked at 'index out of bounds' in mq-markdown/src/parser.rs:45"
