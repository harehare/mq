# Profiler

The profiler reports which mq functions and source lines take the most time. It is a separate binary, `mq-profile`, that behaves like `mq` and adds the profiling flags. It is built only with the `profile` feature, so `mq` itself is unaffected and installing one does not replace the other.

## Installation

Build `mq-profile` from source with the feature enabled:

```bash
# Per-function timing
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-profile --features profile

# Per-function timing and per-line timing
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-profile --features profile-lines
```

`profile-lines` includes `profile`.

## Overhead

`mq` contains none of the profiler code. `mq-profile` run without a profile flag costs up to about 8% on call-heavy queries.

With a profile flag, `mq-profile` runs slower because it takes timestamps and updates counters while it runs. Release build, median of 10 runs, relative to `mq`:

| Query | `--profile` | `--profile-lines` |
| ----- | ----------- | ----------------- |
| Recursive `fib(31)` (about 4.4 million calls) | 2.4x | 3.3x |
| `map` over 3 million elements | 2.6x | 4.3x |

### Measurement cost

Each profiled call costs tens of nanoseconds for the timestamps and bookkeeping. When a scope starts and again when it ends, `mq-profile` measures that cost and subtracts it per call from the function times, so a function called millions of times no longer looks slow only because it was timed. The text output ends with the amount, for example `(measurement cost of 41ns per call subtracted)`, and the JSON output has it as `overhead_per_call_ns`.

The correction is approximate:

- On a recursive `fib(31)`, the corrected total is within about 10% of the run time without the profiler.
- Callbacks that a builtin such as `map` calls cost more to track than plain calls, so they are still reported higher than they are. On `map` over 3 million elements the total is about 1.5x the real time.
- Line times (`--profile-lines`) are not corrected.

Compare functions and lines within one profile rather than against a normal run, and treat very small times with caution.

## Per-function timing

`--profile` prints a table to stderr after the query runs. The query result is still written to stdout.

```bash
mq-profile --profile -f query.mq input.md
```

```text
function                              calls         self        total  self%
map                                       1     45.577ms     58.078ms  73.5%
<closure@query:1>                     20002     12.551ms     62.049ms  20.2%
fib                                    1973      3.621ms      3.621ms   5.8%
(measurement cost of 41ns per call subtracted)
```

| Column | Meaning |
| ------ | ------- |
| `calls` | Number of calls. |
| `self` | Time in the function itself, excluding the functions it calls. |
| `total` | Time including callees. A recursive function is counted once, not once per level. |
| `self%` | Share of the summed self time. |

Builtins and the functions defined in `builtin.mq` are listed alongside your own `def`s. Anonymous functions are labelled by where their code starts, such as `<closure@query:3>` or `<closure@helper:7>`, and the top-level program is `<main>`.

The profile is printed even when the query fails, so you can see where the time went before the error.

### Output formats

`--profile` takes an optional format: `--profile=text` (default), `--profile=folded` or `--profile=json`.

`folded` prints one stack per line with its self time in microseconds, in the format used by flame graph tools. Pass it to [inferno](https://github.com/jonhoo/inferno) or load it into [speedscope](https://www.speedscope.app/):

```bash
mq-profile --profile=folded -f query.mq input.md 2> profile.folded
inferno-flamegraph < profile.folded > flame.svg
```

`json` prints the per-function table with times in nanoseconds, for comparing runs in CI.

## Per-line timing

`--profile-lines` prints the 20 hottest source lines. It needs the `profile-lines` feature and can be combined with `--profile`.

```bash
mq-profile --profile-lines -f query.mq input.md
```

```text
line                               hits         time      %  source
builtin:457                        5003     10.036ms  67.7%
helper:1                           5004      4.756ms  32.1%
query:1                               2     11.584µs   0.1%  import "helper" | helper::heavy(5000)
```

- Lines are shown as `module:line`. The top-level query is `query`, the bundled functions are `builtin`, and imported files use the module name.
- The time of a function a line calls, including native builtins, is charged to the line that made the call.
- `hits` is the number of times execution moved onto the line.

### Heatmap

`--profile-lines-html FILE` writes the line profile as a self-contained HTML page. Each module is a card with its source, and a line's background gets darker the more of the hottest line's time it took. Lines far from any measured line are collapsed.

```bash
mq-profile --profile-lines-html profile.html -f query.mq input.md
```

## Using the profiler from Rust

With the `profile` feature on `mq-lang`, wrap the evaluation in a `ProfileScope`:

```rust
let scope = mq_lang::profile::ProfileScope::start();
// ... evaluate a query ...
let profile = scope.finish();
eprint!("{profile}");
```

`ProfileScope::with_lines()` also records lines when `mq-lang` is built with `profile-lines`. `Profile::functions()` and `Profile::lines()` return the raw numbers, and `to_folded()` and `to_json()` render the other formats.
