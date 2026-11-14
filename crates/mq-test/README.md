<h1 align="center">mq-test</h1>

Standalone test runner for mq: discovers and runs test functions in `.mq` files.

## Installation

```bash
cargo install mq-test
```

## Usage

```bash
mq-test                      # all *.mq files under the current directory
mq-test tests.mq other.mq    # specific files
mq-test -L vendor/mqmq t.mq  # extra module directory (repeatable)
mq-test -k parse             # tests whose name contains "parse" (case-insensitive)
mq-test --tag smoke          # tests tagged "smoke" (repeatable)
mq-test -P 4                 # run files in parallel once more than 4 are found
mq-test --update-snapshots   # accept current assert_snapshot output as golden
mq-test --list               # list tests without running (honors -k/--tag)
mq-test --format json        # JSON for --list and runs, for editors and CI
mq-test --doc docs/guide.md lib.mq  # run `# =>` examples in ```mq blocks and `.mq` comments
mq-test --coverage           # line coverage of included/imported modules
```

A failing test, or a file that fails to run, does not stop other files. `mq-test` exits non-zero if any failed. An error inside a test body is recorded as that test's failure.

## Writing Tests

A function is a test if its name starts with `test_`, or the comment directly above it is one of
`# @test` / `#[test]`, `@parametrize`, `@property` or `@small_scope`.

```mq
include "test"
|

def test_is_array():
  assert_eq(is_array([1, 2, 3]), true)
end

# @test
def check_len():
  assert_eq(len("hello"), 5)
end
```

| Function                       | Description                                     |
| ------------------------------ | ----------------------------------------------- |
| `assert_eq(actual, expect)`    | Fails if `actual != expect`                     |
| `assert(cond)`                 | Fails if `cond` is not `true`                   |
| `assert_snapshot(name, actual)`| Fails if `actual` differs from the golden file  |
| `mock_fetch(url, body)`        | Seeds the response of a later `http()` call     |

`run_tests(...)` is generated from the discovered tests, so files keep no manual list. A failed
assertion is a returned value, so assertions inside `foreach`, `map` and `while` are detected too.

### Parameterized tests

`# @parametrize(expr)` runs the function once per element; each element is spread as arguments.
Cases are named `name[0]`, `name[1]`, and the `(...)` may span several comment lines.

```mq
# @parametrize([["hello", 5], ["world", 5], ["", 0]])
def test_len(input, expected):
  assert_eq(len(input), expected)
end
```

### Property-based tests

`# @property(count, generators)` runs against `count` (default 100) generated cases, one `gen`
generator per parameter (`import "gen"`: `gen::int`, `gen::string`, `gen::array`, `gen::one_of`,
`gen::markdown(min_blocks, max_blocks, max_depth)`, ...).

```mq
include "test"
| import "gen"
|

# @property(100, [gen::int(-1000, 1000), gen::int(-1000, 1000)])
def test_addition_is_commutative(a, b):
  assert_eq(a + b, b + a)
end
```

Case `name[42]` is seeded by its index: `gen::tuple(generators)(42)` reproduces it. A failure is
shrunk with `gen::shrink` first; the report shows the shrunk and original arguments.

### Small-scope tests

`# @small_scope(max_size, enumerators, options)` runs the function on **every** input of size
`<= max_size`, smallest first, so the first failure is a smallest counterexample. A clean run means
"no counterexample within this scope", not a proof. The report shows a per-size table of
inputs checked.

```mq
# @small_scope(3, [enumerate::markdown({"max_heading_depth": 3})])
def test_heading_lines_parse_as_headings(src):
  let doc = to_markdown(src)
  | assert_eq(len(filter(doc, is_h)), len(filter(split(src, "\n"), fn(l): starts_with(l, "#");)))
end
```

- `max_size` defaults to 3. `options`: `max_cases` (inputs checked, default 100000). Exceeding
  the limit fails the test.
- The bundled `enumerate` module is imported automatically. An enumerator is a dict whose `tier(k)`
  returns every value of size exactly `k`. Combinators: `element`, `one_of`, `tuple`, `array`,
  `array_of`, `transform`, `node`, `described`. Values: `int`, `string`, `bool`, `dict_of`,
  `json`, `markdown`.
- `enumerate::markdown(options)` yields Markdown **source text** (parse with `to_markdown(src)`).
  Options: `kinds` (heading, paragraph, bullet_list, ordered_list, code, blockquote, link,
  emphasis, table), `texts`, `max_heading_depth`, `max_nesting`, `min_blocks`, `max_blocks`,
  `max_list_items`, `max_quote_blocks`, `min_table_cols`, `max_table_cols`, `min_table_rows`,
  `max_table_rows`.
- Every enumerated input is checked. Build inputs that satisfy a test's precondition with the
  enumerator options, or return early from the test for inputs it does not cover.

## Tags

`# @tags(a, b)` (or `# @tag(a)`) on a test, combined with `--tag <TAG>`:

```mq
# @tags(smoke, fast)
def test_add():
  assert_eq(1 + 1, 2)
end
```

## Snapshot Testing

`assert_snapshot(name, actual)` compares against `__snapshots__/<test file stem>/<name>.snap` next
to the test file. A missing snapshot fails the test; create or overwrite with `--update-snapshots`.
Snapshots use real disk I/O, the one exception to the hermetic `Io` below.

## Mocking File and Network I/O

Each test file runs against a fresh in-memory `Io`, so `read_file`, `write_file` and `http` always
work regardless of `--allow-*` flags, and state never leaks between files.

```mq
def test_reads_a_mocked_api_response():
  mock_fetch("https://api.example.com/data", "{\"ok\": true}")
  | assert_eq(http("get", "https://api.example.com/data"), "{\"ok\": true}")
end
```

## Coverage

`--coverage` reports line coverage of the code your tests `include`/`import`. The test file's own
lines are not counted, and a module shared by several test files is reported once, combined.

```
  lib/string_utils.mq                                   66.7% (2/3)
      uncovered lines: 9
```

- `--coverage-format <text|lcov|html|markdown|json|cobertura>` (default `text`).
- `--coverage-output <path>` writes to a file; `--open` then opens it (needs `--coverage-output`).
- Line-based: a line is covered if any expression on it ran. Tracking is off without `--coverage`.

## JSON Output

`--format json` prints one JSON document for `--list` and for runs.

```json
{ "files": [ { "file": "tests.mq", "tests": [
  { "name": "add", "kind": "simple", "tags": [] } ] } ] }
```

`kind` is `simple`, `parametrized`, `property` or `small_scope`; generated cases (`name[0]`, ...)
only appear in run results, which carry `status`, `duration` and `error` per test. A file that
fails to read or evaluate is reported as `{"file": "...", "error": "..."}`.

## Development

```bash
just test-all
cargo build -p mq-test
```

## License

MIT

## Doc examples

`--doc` runs the examples in the given files like a doc test. In Markdown it reads ` ```mq ` blocks,
in `.mq` files it reads fenced blocks inside `#` comments. A query is the code since the last blank
line or expectation, followed by its `# =>` line(s):

````markdown
```mq
[1, 2, 3] | map(fn(x): x * 2;)
# => [2, 4, 6]
```
````

An expectation passes when it equals the printed result or evaluates to the same value. A `def` or
`module` segment without `# =>` stays in scope for the later examples of its block. Put
`<!-- doc-test: skip -->` on the line before a fence (inside the comment for `.mq`) to exclude a
block that needs external input.
