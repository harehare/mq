---
paths: crates/mq-python/**
---

# mq-python Rules

## Purpose

Python bindings for integrating mq functionality into Python applications.

## Coding Rules

- Provide Pythonic and idiomatic bindings
- Use PyO3 or similar for creating bindings
- Follow Python naming conventions (snake_case)
- Provide type hints for all public APIs
- Document all public APIs using docstrings
- Handle errors gracefully; convert Rust errors to Python exceptions
- Use `miette` for error handling on the Rust side
- Write tests using pytest or similar Python test frameworks
- Provide example usage in documentation and docstrings
- Support both synchronous and asynchronous APIs where appropriate
- Ensure thread safety and GIL handling is correct
- Test with various Python versions (3.8+)
- Provide wheel distributions for common platforms
- Keep the API surface minimal and focused
- Document memory management and ownership semantics
