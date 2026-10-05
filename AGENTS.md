# mq Development Guide

`mq` is a jq-like command-line tool for Markdown processing, written in Rust.

## Where things live

- `mq-markdown`: all Markdown parsing and manipulation
- `mq-run`: all CLI logic
- `mq-hir`: HIR shared by the LSP, linter and type checker
- `mq-check`: type checker, holds builtin type signatures
- `mq-macros`: `#[mq_macros::mq_fn]` native builtin registration
- `mq-test` / `mq-bench`: runners for `.mq` tests and `bench_` functions

## Commands

Use `just`, not bare `cargo test`.

- `just test-all`: fmt check, clippy, mq tests, doctests, all-features and workspace tests
- `just test-mq`: `.mq` tests only. Sufficient when the change is confined to `crates/mq-lang`
- `just lint`: clippy with `-D clippy::all`
- `just dump-bytecode '<query>'` / `just vm-profile '<query>'`: inspect VM output

## Conventions

- Use `miette` for user-facing errors. Avoid panics.
- Public items get doc comments. Keep comments and clap help text terse, only on non-obvious parts.
- Do not write migration or "what changed from the old parser/markdown-rs" notes in docs or comments. Put that in the commit message.
- Delete code that a change makes unused or always-false, even if the diff grows.
- Avoid em dashes in comments and docs.
- Update `docs/` and the crate `README.md` for user-visible changes.

## Generated docs

Do not hand-edit these, and do not run `just docs` during normal development. It is run only at release time. It regenerates them from the released `mq` binary, and CI checks they match it, so a new flag or builtin will not appear until after release.

- `README.md` Options block
- `docs/books/src/reference/cli.md`
- `docs/books/src/builtins.html`

`mq docs` shells out to the installed `mq-docs`, not to the local build.

## Builtins

- Do not add a builtin that duplicates or thinly wraps an existing one or an operator (e.g. a `substr` when `slice` exists).
- A new builtin needs its type signature registered in `crates/mq-check/src/builtin.rs`.
- Pure-mq functions in `builtin.mq`: tests go only in `crates/mq-lang/builtin_tests.mq`.
- Native Rust builtins: test only the logic you added, not the behavior of the underlying crate (e.g. `regex`).
- In `builtin.mq`, a multi-statement `let`/`var` pipe chain used as an `if`/`else` branch value must be wrapped in `do ... end`. Single-expression branches need no `end`.

## Commits

Conventional commits with a leading emoji, e.g. `🐛 fix(lang): ...`, `⚡ perf(lang): ...`.
