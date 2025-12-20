---
paths: crates/mq-dap/**
---

# mq-dap Rules

## Purpose

Debug Adapter Protocol implementation for debugging mq queries and scripts.

## Coding Rules

- Follow the Debug Adapter Protocol specification strictly
- Clearly separate protocol, transport, and debugging logic
- Document all public types and functions exposed to DAP clients
- Write integration tests for DAP features and message handling
- Use `miette` for error reporting where possible
- Avoid blocking operations in async handlers
- Ensure robust handling of invalid or unexpected DAP messages
- Support standard DAP features: breakpoints, stepping, variable inspection
- Provide clear error messages for debugging failures
- Document any deviations from or extensions to the DAP spec
- Test with popular DAP clients (VS Code, etc.)
- Handle concurrent debugging sessions if applicable
- Keep debugging state consistent and well-documented
