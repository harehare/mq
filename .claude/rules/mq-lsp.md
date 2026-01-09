---
paths: crates/mq-lsp/**
---

# mq-lsp Rules

## Purpose

Language Server Protocol implementation for mq.

## Coding Rules

- Follow LSP specification and conventions for all protocol handling
- Clearly separate protocol, transport, and business logic
- Document all public types and functions, especially those exposed to LSP clients
- Write integration tests for LSP features and message handling
- Use `miette` for error reporting to the user where possible
- Avoid blocking operations in async handlers
- Ensure robust handling of invalid or unexpected LSP messages
- Support standard LSP features: completion, hover, diagnostics, formatting
- Provide incremental updates for better performance
- Handle concurrent requests appropriately
- Test with popular LSP clients (VS Code, Neovim, etc.)
- Keep LSP server state consistent and well-documented
- Provide clear diagnostics with helpful messages and quick fixes
- Document any deviations from or extensions to the LSP spec
- Optimize for responsiveness and low latency
