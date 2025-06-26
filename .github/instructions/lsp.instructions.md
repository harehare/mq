---
applyTo: "crates/mq-lsp/**/*.rs"
---

# Language Server Protocol (LSP) Coding Rules

- Follow LSP specification and conventions for all protocol handling.
- Clearly separate protocol, transport, and business logic.
- Document all public types and functions, especially those exposed to LSP clients.
- Write integration tests for LSP features and message handling.
- Use `miette` for error reporting to the user where possible.
- Avoid blocking operations in async handlers.
- Ensure robust handling of invalid or unexpected LSP messages.

