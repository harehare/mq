---
description: "Analyzes and fixes content or code provided as input."
---

# Fix Input (for Copilot)

Analyzes and fixes content or code provided as input, such as code snippets, configuration files, or documentation.

## Usage

- If arguments contain code snippets, analyze and fix errors
- If arguments contain file content, resolve issues
- If arguments contain configuration, validate and correct settings
- If arguments contain documentation, improve clarity and accuracy
- Supports multiple inputs or descriptions

## What it does

- Analyzes provided input content
- Identifies syntax errors, logic issues, or improvements
- Fixes content following mq conventions and best practices
- Ensures Rust code uses miette for error handling
- Validates configuration against project standards
- Improves documentation clarity and accuracy

## Example

/fix-input "fn parse_markdown(input: &str) -> Result<Document> { panic!(\"not implemented\") }"
/fix-input "Invalid TOML configuration in Cargo.toml"
/fix-input "README.md section needs better examples"
