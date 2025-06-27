---
applyTo: "tests/**/*.rs,crates/**/tests/**/*.rs"
---

# Testing Conventions

- Write comprehensive tests for all new features and bug fixes.
- Use descriptive names for test functions and modules.
- Prefer table-driven tests for similar input/output patterns.
- Use `assert_eq!`, `assert!`, and custom error messages for clarity.
- Avoid flaky or timing-dependent tests.
- Place integration tests in the `tests/` directory and unit tests alongside implementation.
- Mock external dependencies where possible.
- Keep tests fast and isolated.
- Update or add tests when changing existing code.

Use `just test` to run tests instead of `cargo test`.

