---
paths: crates/mq-formatter/**
---

# mq-formatter Rules

## Purpose

Code formatter for mq query language.

## Coding Rules

- Produce consistent, idiomatic formatting
- Preserve semantic equivalence; never change program behavior
- Handle all valid syntax correctly
- Provide clear error messages for malformed input using `miette`
- Support configurable formatting options where appropriate
- Make formatting decisions deterministic and well-documented
- Write comprehensive tests covering all syntax constructs
- Test with edge cases: deeply nested expressions, long lines, comments
- Ensure formatting is idempotent (formatting twice produces same result)
- Optimize for readability and common conventions
- Document all formatting rules and options
- Provide examples of before/after formatting
- Handle comments and whitespace appropriately
- Keep formatting fast and efficient
