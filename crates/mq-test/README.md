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
mq-test --smt f.mq | z3 -in  # SMT-LIB for @requires/@ensures contracts
mq-test --verify f.mq        # run a solver (z3) on the contracts, ✅/❌ per @ensures
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
inputs, valid and skipped counts, and flags a high skip rate with ⚠️.

```mq
# @small_scope(3, [enumerate::markdown({"max_heading_depth": 3})])
def test_heading_lines_parse_as_headings(src):
  let doc = to_markdown(src)
  | assert_eq(len(filter(doc, is_h)), len(filter(split(src, "\n"), fn(l): starts_with(l, "#");)))
end
```

- `max_size` defaults to 3. `options`: `max_cases` (valid checks, default 100000), `max_total`
  (valid plus skipped, default `10 * max_cases`), `skip_warn` (default `0.5`). Exceeding a limit
  fails the test.
- The bundled `enumerate` module is imported automatically. An enumerator is a dict whose `tier(k)`
  returns every value of size exactly `k`. Combinators: `element`, `one_of`, `tuple`, `array`,
  `array_of`, `transform`, `node`, `described`. Values: `int`, `string`, `bool`, `dict_of`,
  `json`, `markdown`.
- `enumerate::markdown(options)` yields Markdown **source text** (parse with `to_markdown(src)`).
  Options: `kinds` (heading, paragraph, bullet_list, ordered_list, code, blockquote, link,
  emphasis, table), `texts`, `max_heading_depth`, `max_nesting`, `min_blocks`, `max_blocks`,
  `max_list_items`, `max_quote_blocks`, `min_table_cols`, `max_table_cols`, `min_table_rows`,
  `max_table_rows`.
- A `@requires` violation in a function the test calls directly is a **skip**. `@ensures`
  violations, other errors, and a `@requires` violation raised from one contracted function calling
  another (the caller's bug) are failures. The report lists valid, skipped and total counts, warns
  above `skip_warn`, and fails a run with no valid input. Prefer enumerator options over skips.
- Nested calls are told apart by the contract wrapper. A violation reached through an
  *uncontracted* helper still looks direct and is skipped; the skip count makes that visible.

## Function Contracts

`# @requires(expr)` and `# @ensures(expr)` on a top-level `def` are checked on every call while
tests run (plain `mq` ignores them). The function need not be a test.

```mq
# @requires(n >= 0)
# @ensures(result >= n)
def fact(n):
  if (n <= 1): 1 else: n * fact(n - 1)
end

# @property(50, [gen::int(0, 10)])
def test_fact_keeps_its_contract(n):
  fact(n)
end
```

- `@requires` sees the arguments; a violation is the caller's fault. `@ensures` also sees `result`;
  a violation is the function's fault (so a parameter cannot be named `result`).
- Several of each may be given, and all must hold. Reports name the function, expression, line,
  arguments and result. Recursive calls are checked too.
- In `@property` tests an `@ensures` violation is shrunk; shrink candidates violating `@requires`
  are skipped, so the counterexample stays in the valid domain.
- Use `all`, `any` and other higher-order builtins in expressions. There is no `old(x)` or `forall`.
- Only top-level `def`s (not inside a `module`) without variadic parameters. Default values are kept. Expressions are
  not type-checked or checked for purity.

After the run, a `## Contracts` table lists each contract with how often it was evaluated and how
often it did not hold (`❌` a violated `@ensures`, `↷` an unmet `@requires`, i.e. an input outside the
function's domain). A contract shown as `⚪` (0 evaluations) was never exercised by the tests. `--format json`
carries the same data as a `contracts` array per file.

The `def` is renamed to `__orig_<name>` and a checking wrapper is appended, so line numbers stay put.

Contracts written in a module you `include`/`import` are checked the same way, so tests that call a
library function also verify the contracts on it (except the modules bundled with `mq-test`):

```mq
# lib/rows.mq
# @ensures(len(result) == len(xs) + 1)
def push(xs, x): xs + [x];
```

### SMT-LIB

`mq-test --smt file.mq` prints an SMT-LIB script instead of running tests, so a solver can check
contracts for **all** inputs rather than sampled ones. `mq-test --verify file.mq` runs a solver on
it and prints a verdict per `@ensures` (`just smt` checks the bundled `smt/examples.mq`):

```bash
mq-test --smt clamp.mq | z3 -in   # raw script
mq-test --verify clamp.mq         # ✅ holds / ❌ counterexample with model / ⚠️ unknown
mq-test --verify --solver "cvc5 --lang smt2 --tlimit=5000" clamp.mq
```

`--verify` runs the solver (default `z3 -in -t:10000`) as an external command, so `mq-test` itself
has no solver dependency. It exits non-zero on a counterexample or a solver error.

A counterexample is shown as mq values (`name = "" gives result = "hello, "`) and replayed in the real
interpreter. `(reproduced in mq)` means mq raises the same `@ensures` violation with the same result;
`not reproduced` means the SMT translation differs from mq's semantics for that input (for example
`xs[5]` on a short array is unconstrained in SMT-LIB but `None` in mq).

```mq
# @requires(lo <= hi)
# @ensures(result >= lo && result <= hi)
def clamp(x, lo, hi): min(max(x, lo), hi);
```

For each function with `@ensures`, the script defines the function, assumes every `@requires` and
asserts the negation of one `@ensures`: `unsat` means it always holds, `sat` is a counterexample.

- Sorts come from a `# @type(...)` comment on the `def`, written like `mq-check` types: `number`,
  `bool`, `string`, `[T]` and `(A, B) -> R`. A function without one takes and returns numbers.
  Nothing is inferred, so the sorts are always the ones you wrote.

  ```mq
  # @type((string, [number]) -> bool)
  # @ensures(result == (len(s) > len(xs)))
  def longer(s, xs): len(s) > len(xs);
  ```

- Supported expressions: integer, string (printable ASCII) and array literals, `+ - * %` (`+` also
  concatenates strings and arrays), comparisons, `==`/`!=`, `&&`, `||`, `!`, `not`, `if`/`elif`/
  `else` (the `else` is required), leading `let` bindings, `len`, `x[i]`, `starts_with`,
  `ends_with`, `contains`, `abs`/`min`/`max`, and calls to other top-level `def`s in the file.
- Numbers are modeled as mathematical integers. `%` is the truncated remainder; `/` is not
  translated. A zero divisor is unconstrained, whereas mq raises an error.
- Self-recursion becomes `define-fun-rec`; a solver may answer `unknown` or time out because no
  induction is performed. Mutual recursion is not supported.
- A function outside this subset is skipped with a `; skipped <name>: <reason>` comment instead of
  being approximated.

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
