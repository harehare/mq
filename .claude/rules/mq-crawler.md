---
paths: crates/mq-crawler/**
---

# mq-crawler Rules

## Purpose

Tool for crawling directories and collecting Markdown files for batch processing.

## Coding Rules

- Implement efficient directory traversal
- Handle filesystem errors gracefully using `miette`
- Support configurable file filtering (extensions, patterns, ignore files)
- Respect `.gitignore` and similar ignore patterns when appropriate
- Avoid following symlinks infinitely; detect and handle cycles
- Provide progress feedback for large directory trees
- Write tests for various directory structures and edge cases
- Document all configuration options and filtering rules
- Handle permissions errors gracefully
- Support concurrent/parallel crawling where appropriate
- Limit resource usage (memory, file handles) appropriately
- Provide clear error messages for inaccessible paths
- Test with edge cases: empty directories, deep nesting, large file counts
