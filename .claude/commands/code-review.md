---
allowed-tools: Read(*.rs,*.md,*.ts,*.tsx,*.js,*.toml,*.json)
description: "Performs code review for the specified files or directories."
---

# Code Review

Performs comprehensive code review for the mq project focused on quality and performance improvements.

## What it does

- Analyzes Rust code for performance bottlenecks and optimization opportunities
- Reviews code for adherence to mq project conventions and best practices
- Checks error handling patterns using miette crate
- Validates documentation coverage for public APIs
- Identifies potential security issues and anti-patterns
- Suggests improvements for code maintainability and readability

## How to use

Run the code review command to analyze the entire codebase or specific files:

```
/code-review
```

Or for specific files/directories:

```
/code-review crates/mq-lang/src/
```

## Review Areas

The code review will focus on:

1. **Performance Analysis**
   - Memory allocation patterns
   - Algorithmic complexity
   - Unnecessary clones and allocations
   - Inefficient data structures usage

2. **Code Quality**
   - Adherence to Rust idioms and best practices
   - Error handling with miette crate
   - Documentation coverage
   - Test coverage and quality

3. **Security & Safety**
   - Unsafe code usage
   - Input validation
   - Resource management
   - Potential panic conditions

4. **Project Standards**
   - Following mq coding conventions from CLAUDE.md
   - Proper use of crate organization
   - Consistent API design patterns

5. **Maintainability**
   - Code complexity and readability
   - Modular design principles
   - Dependency management

The review will provide specific, actionable recommendations with code examples where applicable.
