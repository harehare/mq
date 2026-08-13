<h1 align="center">mq-test</h1>

Standalone test runner for mq — auto-discovers and executes test functions in `.mq` files.

## Overview

`mq-test` discovers test functions in `.mq` files and runs them using the mq engine. A function is treated as a test if:

- Its name starts with `test_`, **OR**
- It is immediately preceded by a `# @test` or `# [test]` annotation comment, **OR**
- It is immediately preceded by a `# @parametrize(...)` annotation comment.

Test discovery uses the CST so all conventions are resolved accurately without any line-scanning heuristics.

## Installation

```bash
cargo install mq-test
```

## Usage

```bash
# Run all *.mq files in the current directory (recursive)
mq-test

# Run a specific test file
mq-test tests.mq

# Run multiple test files
mq-test tests.mq other_tests.mq

# Run with a line-coverage report
mq-test --coverage

# Write an lcov tracefile for CI (e.g. codecov, genhtml)
mq-test --coverage --coverage-format lcov --coverage-output lcov.info

# Write a self-contained HTML report with per-line source highlighting
mq-test --coverage --coverage-format html --coverage-output coverage.html

# Write an HTML report and open it in the browser
mq-test --coverage --coverage-format html --coverage-output coverage.html --open

# Write a Markdown report (e.g. to paste into a PR description)
mq-test --coverage --coverage-format markdown --coverage-output coverage.md

# Write a JSON report
mq-test --coverage --coverage-format json --coverage-output coverage.json

# Write a Cobertura XML report (e.g. Jenkins, GitLab CI)
mq-test --coverage --coverage-format cobertura --coverage-output cobertura.xml

# Only run tests whose name contains "parse" (case-insensitive)
mq-test --filter parse

# Only run tests tagged "smoke" (see `# @tags(...)` below)
mq-test --tag smoke

# Run test files in parallel once more than 4 files are discovered
mq-test --parallel-threshold 4

# Accept the current output of every assert_snapshot(...) call as the new golden snapshot
mq-test --update-snapshots
```

## Coverage

Pass `--coverage` to report how much of the code your tests `include`/`import`
was actually exercised while the tests ran — i.e. is the library code under
test covered, not just the test file itself:

```
Coverage report:
  lib/string_utils.mq                                   66.7% (2/3)
      uncovered lines: 9

  Total: 66.7% (2/3)
```

- Only `include`d/imported modules are measured. The test file's own lines
  (test bodies, assertions, `include`/`import` statements) are not counted —
  they're the thing doing the exercising, not the thing being measured. A
  test file with no `include`/`import` produces no coverage output.
- A module is reported once with combined coverage across every test file
  that imports it, even if several test files share it.
- `--coverage-format <text|lcov|html|markdown|json|cobertura>` selects the report format (default: `text`).
  - `lcov` produces an [lcov tracefile](https://ltp.sourceforge.net/coverage/lcov/geninfo.1.php)
    suitable for `genhtml` or CI coverage integrations.
  - `html` produces a self-contained HTML report: a summary table plus a
    collapsible, line-by-line source view per file, with covered lines
    highlighted green and uncovered lines red. Follows the viewer's
    light/dark theme.
  - `markdown` produces the same summary table plus a per-file source listing
    in a ` ```diff ` block, so GitHub (and other diff-aware Markdown
    renderers) colors covered/uncovered lines green/red — handy for pasting
    into a PR description or CI job summary.
  - `json` produces a machine-readable report with per-file and total stats,
    plus a `lines` array per file giving each line's content and
    `covered`/`uncovered`/`plain` status.
  - `cobertura` produces a [Cobertura](https://cobertura.github.io/cobertura/) XML report
    suitable for Jenkins/GitLab CI coverage integrations.
- `--coverage-output <path>` writes the report to a file instead of stdout.
- `--open` opens the written report in the OS default application (`open` on
  macOS, `xdg-open` on Linux, `start` on Windows). Requires `--coverage-output`.
- Coverage is line-based: a line counts as covered if the evaluator executed
  any expression on it while running the tests. `def`/`include`/`import`
  declaration lines themselves aren't counted (only their bodies are).
- Coverage tracking is only active when `--coverage` is passed, so normal
  `mq-test` runs have no added overhead.

## Writing Tests

### Naming Convention

Any function whose name begins with `test_` is automatically treated as a test:

```mq
include "test"
|

def test_is_array():
  assert_eq(is_array([1, 2, 3]), true)
end
```

### Annotation Convention

Use the `# @test` or `#[test]` comment immediately before a function to mark it as a test regardless of its name:

```mq
include "test"
|

# @test
def check_string_len():
  assert_eq(len("hello"), 5)
end

#[test]
def check_string_upcase():
  assert_eq(upcase("hello"), "HELLO")
end
```

### Parameterized Tests

Use `# @parametrize(expr)` to run a function once per element in an array.
Each element is spread as positional arguments to the function.
Generated test case names use the pattern `name[0]`, `name[1]`, etc.

```mq
include "test"
|

# @parametrize([["hello", 5], ["world", 5], ["", 0]])
def test_len(input, expected):
  assert_eq(len(input), expected)
end
```

This produces three test cases — `len[0]`, `len[1]`, `len[2]` — each called
with the corresponding `[input, expected]` pair.

### Tags

Use `# @tags(a, b)` (or the singular `# @tag(a)`) immediately before a test
function to tag it. Combine with `--tag <TAG>` to only run tests carrying at
least one of the given tags:

```mq
include "test"
|

# @tags(smoke, fast)
def test_add():
  assert_eq(1 + 1, 2)
end

# @tags(slow)
def test_large_input():
  assert_eq(len(range(0, 100000)), 100000)
end
```

```bash
# Only runs test_add
mq-test --tag smoke
```

Tags can be combined with `# @test`/`# @parametrize(...)` on the same
function via a separate comment line placed right above it.

### Filtering and Parallel Execution

- `--filter <SUBSTRING>` / `-k <SUBSTRING>` only runs tests whose display
  name (the test name with any `test_` prefix stripped) contains
  `<SUBSTRING>`, case-insensitively.
- `--tag <TAG>` only runs tests carrying `<TAG>` (see above). Repeat the flag
  to match any of several tags.
- `--parallel-threshold <N>` / `-P <N>` runs test files in parallel — each
  file still gets its own hermetic engine and `Io` — once more than `N`
  files are discovered. Defaults to never parallelizing. Coverage data is
  merged safely across parallel files, and each file's report prints
  atomically so concurrent files' output never interleaves.

A failing test in one file no longer stops other files from running: every
discovered file always runs to completion, and `mq-test` exits non-zero if
any test in any file failed.

### Test Helpers

Tests use the built-in `assert_eq` and related helpers from the `test` module:

| Function                        | Description                    |
| -------------------------------- | ------------------------------ |
| `assert_eq(actual, expect)`      | Fails if `actual != expect`    |
| `assert(cond)`                   | Fails if `cond` is not `true`  |
| `assert_snapshot(name, actual)`  | Fails if `actual` doesn't match the golden snapshot `name` |
| `test_case(name, fn)`            | Registers a named test case    |
| `run_tests(cases)`               | Runs all registered test cases |

The runner automatically generates a `run_tests(flatten([...]))` call from all
discovered test functions — test files do not need to maintain a manual list.

### Snapshot Testing

`assert_snapshot(name, actual)` compares `actual` against a golden file, for outputs
too large to usefully inline in an `assert_eq` diff (e.g. a rendered document):

```mq
include "test"
|

def test_renders_the_full_report():
  assert_snapshot("report", render_report(data))
end
```

The layout follows the `suite`/`ref`/`store` split from
[typst's test runner](https://github.com/typst/typst/tree/main/tests): the test file
itself plus whatever it feeds into `assert_snapshot` is the "suite" (input), and:

- **ref** — `__snapshots__/<test file stem>/<name>.snap`, next to the test file. This is
  the golden, checked-in expected value.
- **store** — `.mq-test-store/<test file stem>/<name>.diff.html` (and `.actual.snap`),
  written on a mismatch. A self-contained HTML diff report plus the raw actual output,
  for reviewing a large mismatch without scrolling a terminal. Not checked in — add
  `.mq-test-store/` to `.gitignore`.

A snapshot that doesn't exist yet fails the test (it does not get created implicitly) —
this is the one exception in `mq-test` where "no such thing" is a real failure, not an
error to silently paper over. Run with `--update-snapshots` to create or overwrite golden
snapshots from the current output:

```bash
mq-test --update-snapshots
```

`assert_snapshot` is real, unmocked disk I/O — the one deliberate exception to the
hermetic `Io` described below, since golden files must survive across runs and be
checked into version control.

### Mocking File and Network I/O

Each test file runs against an in-memory, hermetic `Io` — no real disk or network access —
so `read_file`/`write_file`/`http` are always allowed, regardless of the `--allow-read` /
`--allow-write` / `--allow-net` flags the CLI itself requires.

Files can be seeded from within a test simply by writing them first:

```mq
include "test"
|

def test_reads_a_file_it_wrote():
  write_file("/config.json", "{}")
  | assert_eq(read_file("/config.json"), "{}")
end
```

`mock_fetch(url, body)` seeds the response body a subsequent `http()` call for `url`
returns, so a test can exercise code that calls `http()` without making a real request:

```mq
include "test"
|

def test_reads_a_mocked_api_response():
  mock_fetch("https://api.example.com/data", "{\"ok\": true}")
  | assert_eq(http("get", "https://api.example.com/data"), "{\"ok\": true}")
end
```

State (files written, mocked responses) does not leak between test files — each gets a
fresh in-memory `Io`.

## Example

```mq
include "test"
|

def test_add():
  assert_eq(1 + 1, 2)
end

def test_string_upcase():
  assert_eq(upcase("hello"), "HELLO")
end

# @test
def verify_array_length():
  assert_eq(length([1, 2, 3]), 3)
end

#[test]
def verify_string_empty():
  assert_eq(length(""), 0)
end
```

## Development

### Running Tests

```bash
just test-all
```

### Building

```bash
cargo build -p mq-test
```

## License

MIT
