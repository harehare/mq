---
paths: crates/mq-run/**
---

# mq-run Rules

## Purpose

Implementation of the mq command-line interface.

## Coding Rules

- All command-line interface logic must reside in this crate
- Use `clap` or similar crate for argument parsing
- Provide clear, user-friendly error messages using `miette`
- Document all commands, flags, and options in code and in the CLI help output
- Write integration tests for CLI behavior and edge cases
- Ensure the CLI is robust against malformed input and unexpected usage
- Output should be clear and suitable for piping/automation
- Support standard Unix conventions (stdin, stdout, stderr, exit codes)
- Handle Ctrl-C and other signals gracefully
- Provide verbose and quiet modes where appropriate
- Support colorized output with options to disable
- Write comprehensive help text and examples
- Test with various input sources (files, stdin, pipes)
- Keep CLI output format stable for scripting compatibility
- Document breaking changes to CLI behavior clearly
