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
```

## Coverage

Pass `--coverage` to report which lines of each executed test file were run
by the evaluator:

```
Coverage report:
  tests.mq                                              66.7% (2/3)
      uncovered lines: 9

  Total: 66.7% (2/3)
```

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
  any expression on it. `def`/`include`/`import` declaration lines themselves
  aren't counted (only their bodies are), and coverage of `include`d/imported
  modules is not tracked — only the file passed to `mq-test` is measured.
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

### Test Helpers

Tests use the built-in `assert_eq` and related helpers from the `test` module:

| Function                    | Description                    |
| --------------------------- | ------------------------------ |
| `assert_eq(actual, expect)` | Fails if `actual != expect`    |
| `assert(cond)`              | Fails if `cond` is not `true`  |
| `test_case(name, fn)`       | Registers a named test case    |
| `run_tests(cases)`          | Runs all registered test cases |

The runner automatically generates a `run_tests(flatten([...]))` call from all
discovered test functions — test files do not need to maintain a manual list.

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
