---
paths: crates/mq-markdown/**
---

# mq-markdown Rules

## Purpose

Markdown parser and manipulation utilities.

## Coding Rules

- All Markdown parsing and manipulation logic must reside in this crate
- Write tests for all parsing and transformation functions
- Ensure robust handling of edge cases in Markdown syntax
- Document all public APIs and provide usage examples in doc comments
- Avoid panics on malformed input; return descriptive errors using `miette`
- Keep the API surface minimal and focused on Markdown processing
- Support CommonMark specification and document any extensions
- Handle various Markdown flavors correctly (GFM, etc.)
- Provide efficient parsing and transformation operations
- Write comprehensive tests for various Markdown constructs
- Test with edge cases: nested structures, malformed input, edge syntax
- Ensure the parser is resilient to unexpected input
- Provide clear error messages with context
- Keep performance in mind; optimize hot paths
- Document any limitations or unsupported Markdown features
