# Profiler

The profiler reports which mq functions and source lines take the most time. It is built only when the `profile` feature is enabled, so normal `mq` builds are unaffected.

## Installation

Build `mq` from source with the feature enabled:

```bash
# Per-function timing
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq --features profile

# Per-function timing and per-line timing
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq --features profile-lines
```

`profile-lines` includes `profile`. It adds work to every VM instruction, so a build with it runs slower than one with only `profile`.

## Per-function timing

`--profile` prints a table to stderr after the query runs. The query result is still written to stdout.

```bash
mq --profile -f query.mq input.md
```

```text
function                              calls         self        total  self%
map                                       1     45.577ms     58.078ms  73.5%
<closure>                             20002     12.551ms     62.049ms  20.2%
fib                                    1973      3.621ms      3.621ms   5.8%
```

| Column | Meaning |
| ------ | ------- |
| `calls` | Number of calls. |
| `self` | Time in the function itself, excluding the functions it calls. |
| `total` | Time including callees. A recursive function is counted once, not once per level. |
| `self%` | Share of the summed self time. |

Builtins and the functions defined in `builtin.mq` are listed alongside your own `def`s. Anonymous functions and the top-level program appear as `<closure>`.

### Output formats

`--profile` takes an optional format: `--profile=text` (default), `--profile=folded` or `--profile=json`.

`folded` prints one stack per line with its self time in microseconds, in the format used by flame graph tools. Pass it to [inferno](https://github.com/jonhoo/inferno) or load it into [speedscope](https://www.speedscope.app/):

```bash
mq --profile=folded -f query.mq input.md 2> profile.folded
inferno-flamegraph < profile.folded > flame.svg
```

`json` prints the per-function table with times in nanoseconds, for comparing runs in CI.

## Per-line timing

`--profile-lines` prints the 20 hottest source lines. It needs the `profile-lines` feature and can be combined with `--profile`.

```bash
mq --profile-lines -f query.mq input.md
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
mq --profile-lines-html profile.html -f query.mq input.md
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
