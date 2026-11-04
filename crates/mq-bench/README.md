<h1 align="center">mq-bench</h1>

Standalone benchmark runner for mq: times `bench_` functions written in `.mq` files.

## Overview

`mq-bench` discovers bench functions in `.mq` files and times them with the mq engine. A function is treated as a bench if:

- Its name starts with `bench_`, **OR**
- It is immediately preceded by a `# @bench` or `# [bench]` annotation comment.

Bench discovery uses the CST, same as [`mq-test`](../mq-test). Each bench takes no
parameters, is compiled once, warmed up, then timed in `--iterations` samples sequentially with
`std::time::Instant`: never in parallel, since that would add scheduling noise to the
timings. A fast bench runs several evaluations per sample (see
[Measurement](#measurement)), so its time is not lost in the clock's resolution.

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

# Fail (exit 1) when a bench is significantly more than 5% slower than the baseline
mq-bench --baseline results.json --fail-on-regression 5

# Time the parse and bytecode compile instead of the warm evaluation
mq-bench --phase cold
```

## Measurement

Each sample times a batch of evaluations and records the time per evaluation. The batch size is
chosen from one calibration run so a sample lasts at least `--min-sample-time` (default 1 ms);
`--min-sample-time 0` times every evaluation on its own. The `Iterations` column shows
`samples×batch` when batching is in effect.

| Column | Meaning |
| --- | --- |
| Mean, Median, Min, p95, Max | Per-evaluation time over the samples |
| CV | Standard deviation relative to the mean. Above 10% a warning is printed |
| Cold run | Median of a fresh parse, bytecode compile and first evaluation (`--phase all`, the default) |
| Δ vs baseline | Change of the median against `--baseline` |

`--phase` chooses what is measured: `all` (warm evaluation plus the cold run column), `eval`
(warm evaluation only) or `cold` (the cold run is the measurement). A cold run includes one
evaluation, so it only shows compile cost for benches whose evaluation is short.

## Memory

Alongside the timings, `mq-bench` counts the heap use of one evaluation: `Allocs` (allocation
count), `Alloc bytes` (bytes requested in total) and `Peak` (most bytes live at once). The
counts come from a wrapper around the global allocator, so they do not depend on the OS and
do not vary between runs: equal code gives equal numbers, which makes them a precise way to see
a change in allocations that timing noise would hide. With `--baseline`, `Δ alloc bytes` shows
any change of `Alloc bytes`.

The evaluation is counted in its own run, apart from the timed samples, so counting does not
affect the timings. Only the calling thread is counted, and memory that bypasses the allocator
is not seen. With `--phase cold` the counted run is a fresh parse, compile and first evaluation.
The columns are absent when `BenchRunner` is used from a binary that does not install
`mq_bench::CountingAlloc` as its global allocator.

## Comparing against a baseline

A `--format json` result keeps every sample. With such a baseline, a change is reported as faster
(▼) or slower (▲) only when all of these hold; otherwise it is shown as noise (≈):

- a two-sided Mann-Whitney U test gives p < 0.05,
- the median moved by at least `--noise-threshold` percent (default 5),
- the move is larger than the spread (CV) of either run.

A baseline written by an older `mq-bench` has no samples, so only the threshold applies.
`--fail-on-regression PERCENT` exits with a failure when a bench is reported slower by at least
that much.

The test sees the samples of two runs, not the machine state between them. Clock speed,
temperature and background load shift between invocations, and for benches of a few tens of
microseconds that alone can move the median by 10% or more. Take the baseline right before the
run you compare it with, avoid running other work meanwhile, and repeat before trusting a
small difference.

## Progress

When stderr is a terminal, a progress bar shows the overall bench count and the warmup/timed
iterations of the running bench. It is drawn on stderr and hidden when redirected, so
`--format json` and `--output` are unaffected.

## Writing benches

```
def bench_markdown_table_render():
  let header = ["id", "name"]
  | let rows = map(range(0, 79), fn(i): [to_string(i), "item-" + to_string(i)];)
  | to_string(md::doc(md::table(header, rows)))
end
```

Bench functions must take no parameters: the runner calls each one as `name()`.
