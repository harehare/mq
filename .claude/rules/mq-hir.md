---
paths: crates/mq-hir/**
---

# mq-hir Rules

## Purpose

High-level Internal Representation (HIR) for mq.

## Coding Rules

- Design HIR to be easy to analyze and transform
- Clearly separate HIR from AST and lower-level representations
- Document all HIR node types and their semantics
- Ensure HIR is well-typed and type-safe
- Provide conversion functions to/from other representations
- Write tests for HIR construction and transformations
- Keep HIR nodes immutable where possible
- Use appropriate data structures for efficient traversal
- Document the HIR design and structure thoroughly
- Ensure HIR is suitable for optimization and analysis passes
- Handle all language constructs correctly
- Provide clear error messages using `miette` for invalid HIR
- Test with complex and nested language constructs
- Keep HIR representation stable across versions where possible
