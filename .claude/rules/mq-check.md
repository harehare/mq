---
paths: crates/mq-check/**
---

# mq-check Rules

## Purpose

Static type checker and type inference engine for mq programs, implemented using Hindley-Milner style inference.

## Architecture

- `lib.rs` — Public API: `TypeChecker`, `TypeEnv`, `TypeError`, `TypeCheckerOptions`
- `infer.rs` — `InferenceContext` and constraint collection passes
- `constraint.rs` / `constraint/` — Constraint generation from HIR
- `unify.rs` — Unification and constraint solving
- `types.rs` — Type representation: `Type`, `TypeScheme`
- `narrowing.rs` — Type narrowing for conditional branches (e.g., `is_string(x)`)
- `deferred.rs` — Deferred resolution: tuple/record field access, overloads, user-call returns
- `builtin.rs` — Built-in function type registrations
- `main.rs` — CLI binary (`mq-check`)

## Features and Running

- Build/run: `cargo run -p mq-check --features cli -- <file.mq>` (omit file to read from stdin)
- CLI flags: `--show-types` (print inferred types), `--strict-array` (reject mixed-type arrays), `--no-builtins` (skip builtin preloading, use when checking `builtin.mq` itself)
- Debug: `DUMP_HIR=1 cargo run -p mq-check --features cli -- <file.mq>` dumps HIR to stderr
- Tests: `just test` or `cargo test -p mq-check`

## Coding Rules

- `TypeChecker::check()` returns `Vec<TypeError>` — collect all errors rather than stopping at the first
- Constraint generation, unification, and deferred resolution are separate passes; maintain this separation
- All `TypeError` variants must include an `Option<mq_lang::Range>` for source location reporting
- Use `miette` and `thiserror` for error types; provide helpful `context` strings where applicable
- Avoid panics on malformed or unexpected HIR; emit an `Internal` error instead
- The `TypeCheckerOptions` struct controls optional strictness modes (e.g., `strict_array`); add new modes there rather than as global state
- Deferred resolution passes must be ordered correctly — see `check()` in `lib.rs` for the authoritative pass order
- `TypeScheme::generalize()` must be applied to `Function` types during `finalize()` to support polymorphism
- Write table-driven tests using `rstest` for type inference and error cases
- Update `builtin.rs` whenever a new built-in function is added to `mq-lang`
- The CLI binary reads from stdin when no files are given; keep this behavior intact
- `--show-types` output and error formatting are user-facing; keep them clear and consistently styled
- `DUMP_HIR` env var is a debug aid; do not remove it
