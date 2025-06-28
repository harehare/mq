---
allowed-tools: Read(*.rs,*.md,*.ts,*.tsx,*.js,*.toml,*.json)
description: "Analyzes and fixes issues in the mq codebase based on bug reports or error descriptions."
---

# Fix Issue

Analyzes and fixes issues in the mq codebase based on bug reports, error descriptions, or failing tests.

## Issue Target

$ARGUMENTS will be used to specify the issue to fix:
- If $ARGUMENTS contains error messages, analyze and fix the specific error
- If $ARGUMENTS contains file paths, focus investigation on those files
- If $ARGUMENTS contains issue descriptions, investigate and resolve the described problem
- Supports multiple issue descriptions or file paths separated by spaces

## What it does

- Investigates reported issues and identifies root causes using $ARGUMENTS
- Fixes bugs following mq project conventions and best practices
- Ensures error handling uses miette crate appropriately
- Runs tests to verify fixes work correctly
- Formats and validates code using `cargo fmt` and `cargo clippy`
- Updates documentation if the fix affects public APIs

## How to use

Run the fix-issue command with a description of the issue:

```
/fix-issue "The CLI tool panics when processing malformed Markdown files"
/fix-issue "Error: thread 'main' panicked at 'index out of bounds' in mq-markdown/src/parser.rs:45"
/fix-issue "Tests failing in crates/mq-lang/tests/parser_tests.rs"
/fix-issue "Memory leak in markdown processing loop"
```

The command will automatically:
1. Parse $ARGUMENTS to understand the issue context
2. Use Grep and Glob tools to locate relevant code if file paths are mentioned
3. Read and analyze the affected files using Read tool
4. Implement fixes following project standards
5. Run tests and validation tools to ensure the fix works

## Fix Process

The fix process follows these steps:

1. **Issue Analysis**
   - Parse $ARGUMENTS to understand the problem
   - Reproduces the issue if possible
   - Identifies the root cause in the codebase
   - Determines scope of impact

2. **Solution Implementation**
   - Implements fix following Rust best practices
   - Uses miette for proper error handling
   - Avoids panics and returns appropriate Result types
   - Maintains existing API contracts where possible

3. **Testing & Validation**
   - Runs existing tests to ensure no regressions
   - Adds new tests for the fix if needed
   - Validates with `cargo fmt` and `cargo clippy`
   - Tests edge cases related to the fix

4. **Documentation Updates**
   - Updates doc comments if public APIs change
   - Adds changelog entries for user-facing fixes
   - Updates related documentation files if necessary

## Issue Types Handled

- Runtime panics and crashes
- Logic errors and incorrect behavior
- Performance issues and bottlenecks
- Memory leaks and resource management problems
- Error handling improvements
- API inconsistencies
- Test failures and flaky tests
- Build and compilation issues

The fix will provide a clear explanation of what was changed and why, ensuring the solution is robust and maintainable.