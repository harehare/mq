---
paths: crates/mq-lang/**
---

# mq-lang Rules

## Purpose

Core implementation of the mq query.

## Coding Rules

- Implement language semantics correctly and completely
- Provide clear, user-friendly error messages using `miette`
- Write comprehensive tests for all language features
- Document all language constructs and their behavior
- Keep the parser, type checker, and evaluator well-separated
- Ensure type safety and catch type errors early
- Handle edge cases and invalid input gracefully
- Optimize for common use cases while maintaining correctness
- Write integration tests for complex queries
- Document the language grammar and semantics
- Ensure backward compatibility or provide migration paths
- Test with various Markdown inputs and edge cases
- Provide helpful error messages with context and suggestions
- Keep the language simple and composable
- Follow functional programming principles where appropriate
