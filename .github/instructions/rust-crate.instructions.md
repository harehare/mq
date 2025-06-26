---
applyTo: "crates/**/*.rs"
---

# Rust Crate Coding Rules for mq

- Each crate must have a clear purpose and be documented in its `README.md` (if present).
- Organize code into logical modules; avoid large, monolithic files.
- Use `pub(crate)` or tighter visibility unless wider exposure is necessary.
- Prefer explicit error types using `miette` for user-facing errors.
- Write comprehensive unit and integration tests in each crate.
- Document all public APIs with Rust doc comments.
- Avoid unsafe code unless absolutely necessary; document all unsafe blocks.
- Use feature flags for optional functionality.
- Keep dependencies minimal and up-to-date.
- Each crate should have its own `CHANGELOG.md` if it is published independently.

