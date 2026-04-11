<h1 align="center">mq-test</h1>

Standalone test runner for mq — auto-discovers and executes test functions in `.mq` files.

## Overview

`mq-test` discovers test functions in `.mq` files and runs them using the mq engine. A function is treated as a test if:

- Its name starts with `test_`, **OR**
- It is immediately preceded by a `# @test` annotation comment.

Test discovery uses the CST so both conventions are resolved accurately without any line-scanning heuristics.

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
```

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

Use the `# @test` comment immediately before a function to mark it as a test regardless of its name:

```mq
include "test"
|

# @test
def check_string_len():
  assert_eq(len("hello"), 5)
end
```

### Test Helpers

Tests use the built-in `assert_eq` and related helpers from the `test` module:

| Function                   | Description                             |
| -------------------------- | --------------------------------------- |
| `assert_eq(actual, expect)` | Fails if `actual != expect`            |
| `assert(cond)`              | Fails if `cond` is not `true`          |
| `test_case(name, fn)`       | Registers a named test case            |
| `run_tests(cases)`          | Runs all registered test cases         |

The runner automatically generates a `run_tests([...])` call from all discovered test functions — test files do not need to maintain a manual list.

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
```

```bash
$ mq-test example.mq
✓ add
✓ string_upcase
✓ verify_array_length

3 passed, 0 failed
```

## Development

### Running Tests

```bash
just test
```

### Building

```bash
cargo build -p mq-test
```

## License

MIT

