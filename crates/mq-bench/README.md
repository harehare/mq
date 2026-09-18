<h1 align="center">mq-bench</h1>

Standalone benchmark runner for mq — times `bench_` functions written in `.mq` files.

## Overview

`mq-bench` discovers bench functions in `.mq` files and times them with the mq engine. A function is treated as a bench if:

- Its name starts with `bench_`, **OR**
- It is immediately preceded by a `# @bench` or `# [bench]` annotation comment.

Bench discovery uses the CST, same as [`mq-test`](../mq-test). Each bench takes no
parameters, is compiled once, warmed up, then run `--iterations` times sequentially with
`std::time::Instant` — never in parallel, since that would add scheduling noise to the
timings.

This complements low-level, single-instruction Tarn VM benches (written in Rust with
[`divan`](https://docs.rs/divan)) with whole-pipeline timings of realistic `.mq` queries:
parsing, compiling, and running the way a user actually would.

## Installation

```bash
cargo install mq-bench
```

## Usage

```bash
# Run all *.mq files in the current directory (recursive)
mq-bench

# Run a specific bench file
mq-bench benches.mq

# Run more iterations with more warmup
mq-bench benches.mq --iterations 1000 --warmup 10

# Only run benches whose name contains "parse" (case-insensitive)
mq-bench --filter parse

# Write machine-readable results for CI
mq-bench --format json --output results.json

# Paste results into a PR description
mq-bench --format markdown

# Compare the current run against a previous --format json run
mq-bench --baseline results.json
```

## Writing benches

```
def bench_markdown_table_render():
  let header = ["id", "name"]
  | let rows = map(range(0, 79), fn(i): [to_string(i), "item-" + to_string(i)];)
  | to_string(md::doc(md::table(header, rows)))
end
```

Bench functions must take no parameters — the runner calls each one as `name()`.
