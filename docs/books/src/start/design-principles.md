# Design Principles

`mq` is a small language for querying and transforming Markdown. It is not a general-purpose programming language, and it keeps its feature set deliberately narrow.

## What is in the language

- Values: Markdown nodes, strings, numbers, booleans, `None`, arrays, and dicts
- Control flow: `if`, `elif`, `else`, `foreach`, `while`, `until`, `match`, `try`/`catch`
- Abstraction: functions, `let`/`var` bindings, and [modules](modules.md)
- Composition: the `|` pipe

## What is left out

`mq` does not have classes, user-defined types, or inheritance. Data is always one of the built-in values above, and reuse comes from functions and modules.

## Why

- **A query stays readable.** A reader needs to know only the built-in values, not types defined elsewhere.
- **Behavior is predictable.** Every value has a fixed shape, so a query does the same thing on any input.
- **Static analysis stays simple.** The type checker, linter, and language server work on a closed set of types.

Requests that need a user-defined type are better solved by a function that builds and returns a dict.
