---
paths: crates/mq-repl/**
---

# mq-repl Rules

## Purpose

REPL (Read-Eval-Print Loop) for interactive mq query development.

## Coding Rules

- Provide an intuitive and user-friendly interactive experience
- Support command history and editing (arrow keys, etc.)
- Implement auto-completion for language constructs
- Provide helpful error messages using `miette`
- Support multi-line input where appropriate
- Display results in a clear and readable format
- Provide REPL-specific commands (help, reset, quit, etc.)
- Handle Ctrl-C and other interrupts gracefully
- Support loading and executing files within the REPL
- Write tests for REPL commands and interactions
- Document all REPL commands and features
- Provide syntax highlighting if possible
- Show helpful hints and tips for new users
- Keep REPL state manageable and allow resetting
- Test on various terminal emulators and platforms
