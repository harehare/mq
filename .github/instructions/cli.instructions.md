---
applyTo: "crates/mq-cli/**/*.rs"
---

# CLI Tool Coding Rules

- All command-line interface logic must reside in `mq-cli`.
- Use `clap` or similar crate for argument parsing.
- Provide clear, user-friendly error messages using `miette`.
- Document all commands, flags, and options in code and in the CLI help output.
- Write integration tests for CLI behavior and edge cases.
- Ensure the CLI is robust against malformed input and unexpected usage.
- Output should be clear and suitable for piping/automation.

