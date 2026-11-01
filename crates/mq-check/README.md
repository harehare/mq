<h1 align="center">mq-check</h1>

> [!IMPORTANT]
> This crate is currently **under active development**. The type inference and checking features are experimental, and the API or behavior may change without notice. Use in production environments is not recommended.

Type inference and checking for the mq language.

## Usage

### As a Library

```rust
use mq_hir::Hir;
use mq_check::TypeChecker;

// Build HIR from mq code
let mut hir = Hir::default();
hir.add_code(None, "def add(x, y): x + y;");

// Run type checker
let mut type_checker = TypeChecker::new();
let errors = type_checker.check(&hir);

if errors.is_empty() {
    // Type checking succeeded
    for (symbol_id, type_scheme) in type_checker.symbol_types() {
        println!("{:?} :: {}", symbol_id, type_scheme);
    }
} else {
    // Type checking found errors (multiple errors possible)
    for err in &errors {
        eprintln!("Type error: {}", err);
    }
}
```

## Type System

### Basic Types

- `number`: Numeric values (int and float)
- `string`: Text strings
- `bool`: Boolean values (true/false)
- `symbol`: Symbol literals
- `none`: Null/none value
- `markdown`: Markdown document nodes

### Composite Types

- `[T]`: Array of elements of type T
- `(T1, T2, ..., Tn)`: Tuple with known per-element types
- `{K: V}`: Dictionary with keys of type K and values of type V
- `(T1, T2, ..., Tn) -> R`: Function taking arguments T1..Tn and returning R

### Union Types

When a control flow construct (such as `if`, `while`, `loop`, `foreach`, `match`, or `try/catch`) may produce values of different types depending on the branch taken, the type checker infers a **union type**, written as `T1 | T2 | ...`.

Union types propagate through variable bindings and pipe chains. Using a union-typed value in a context that requires a single concrete type (e.g., arithmetic) is a type error.

```mq
// if/else with different branch types → number | string
let x = if (cond): 42 else: "hello";;

// foreach body may return different types → [number | string]
let xs = foreach(item, arr): if (cond): item else: "default";;;

// loop body with different branch types → number | string
let y = loop: if (cond): 1 else: "done";;;

// match arms with different types → number | string
let z = match (val): | 0: "zero" | _: 1 end;

// try/catch with different types → number | string
let r = try: 42 catch: "error";;

// Type error: union-typed variable used in arithmetic
x + 1
// Error: no matching overload for +(number | string, number)
```

Records returned by `try` and `catch` with the same keys are merged field by field, and a union of arrays is checked as an array of the union of its elements.

```mq
// try/catch records → {data: number | none, error: none | {message: string}}
let r = try: {"data": 1, "error": None} catch(e): {"data": None, "error": {"message": e["message"]}};

// A parameter that defaults to None accepts other types: (a | none) -> ...
def f(x, index = None): if (is_none(index)): [1] else: index;
```

### Markdown Node Types

A Markdown node has type `markdown`, or a narrower set of node kinds such as `h1 | h2`, `code` or `markdown - code`. `is_*` predicates, `:h1`-style patterns and selector conditions (`if (.h1): ...`) narrow a value to the kinds they test in the then-branch and remove them in the else-branch.

```mq
def f(node):
  if (is_h1(node) || is_h2(node)):
    node        // node: h1 | h2
  elif (is_code(node)):
    node        // node: code
  else:
    node;;
```

Attributes are typed by the kinds a value can have (`.depth` of a heading is `number`, `.lang` of a code block is `string | none`). An attribute that none of the kinds has is an error, and one that only some kinds have adds `none`. A value of unknown kind (`markdown`) gets the plain attribute type.

```mq
.code | .depth   // Error: attribute `depth` does not exist on code
attr(n, "depth") // number for a heading, the same table when the name is a string literal
if (to_md_name(n) == "h1"): n   // n: h1 inside the branch
.code | .lang    // string | none
```

A function of one parameter whose body is a single condition on it, such as `def is_title(n): to_md_name(n) == "h1" || to_md_name(n) == "h2";`, narrows the same way as the built-in predicates when it is used as a condition. With `&&` only the then-branch narrows, and a body with more than one step does not narrow. `!`, `||` and calls of other such functions work as in a plain condition.

### Generators

Reading a record field narrows with the tests around it: in `if (v["a"] != None): ...` (or `!is_none(v["a"])`, or the else-branch of `== None`) the read is not `none`, and `if (contains(keys(v), "a")): ...` selects the members of a union of records that have the key. Without such a test, a record that lacks the key reads as `none`.

`match` is checked for exhaustiveness against the type of the value. `:number`-style type patterns, `||` alternatives, `[]` with `[a, ..rest]`, `:array` and `:dict` cover what they match, and a match that cannot be decided (an unknown type, nested refutable patterns) is not reported.

A function that contains `yield` returns a coroutine, typed `generator<T>` where `T` is the type of the yielded values. `first`, `map`, `filter`, `flat_map`, `fold`, `next`, `send`, `status` and `close` understand it (`next` gives `{value: T | none, done: bool}`), and `is_coroutine(x)` narrows to it.

```mq
def numbers(): yield: 1 | yield: 2;
// Inferred type: () -> generator<number>

first(numbers())            // number | none
map(numbers(), fn(x): to_string(x);)   // generator<string>
len(numbers())              // Error: a generator is not an array or a string
```

### Example:

```mq
def identity(x): x;
// Inferred type: forall 'a. ('a) -> 'a

def map_array(f, arr): arr | map(f);
// Inferred type: forall 'a 'b. (('a) -> 'b, ['a]) -> ['b]
```

### Error Messages

Type errors name the types involved and point at the code. Type variables that are not resolved get short names (`'a`, `'b`), shared across one message, so two different variables never share a name.

```mq
let s = "a" | abs(s)
// Error: Type mismatch: expected number, found string
//   help: in argument 1 to 'abs'
//   note: `s` is `string` (line 1)
```

A call that no overload takes reads `` `abs` cannot be applied to (string, number) ``, with the signatures to try as help. When a variable or parameter with a known type is passed to the failing call, the CLI shows where it is defined as a note, and the language server sends it as related information.

### Error codes

Every diagnostic has a stable code, available as `TypeError::error_code`, and a fix hint, `TypeError::hint`, when there is one.

| Code | Meaning | Example |
| --- | --- | --- |
| `typechecker::type_mismatch` | A value has a different type than expected. | `let s = "a" \| abs(s)` |
| `typechecker::unification_error` | Two types cannot be made equal, or no overload of a builtin takes the arguments. | `abs("a", 1)` |
| `typechecker::wrong_arity` | A call passes the wrong number of arguments. | `def f(a, b): a + b; \| f(1, 2, 3)` |
| `typechecker::undefined_field` | A record has no such key. | `let r = {"a": 1} \| r["b"] + 1` |
| `typechecker::undefined_attribute` | A node kind has no such attribute. | `to_code("a", "rust") \| .depth` |
| `typechecker::heterogeneous_array` | Array elements have mixed types. Only with `--strict-array`. | `[1, "a"]` |
| `typechecker::nullable_propagation` | An operation gets a value that may be `none` and would silently give `none`. Always a warning. | `def f(x): x \| upcase(); \| f(None)` |
| `typechecker::unreachable_code` | A branch can never run. Always a warning. | `let n = 1 \| if (is_string(n)): 1 else: 2` |
| `typechecker::non_exhaustive_patterns` | A `match` does not cover every value of its type. Always a warning. | `match (1): \| 0: "a" end` |

## CLI Options

When used as a command-line tool (`mq-typecheck`), the following options are available:

| Option           | Description                                                        |
| ---------------- | ------------------------------------------------------------------ |
| `--show-types`   | Display inferred types for all user-defined symbols                |
| `--strict-array` | Reject heterogeneous arrays (e.g., `[1, "hello"]` is a type error) |
| `--show-uncertain` | Show errors the checker is not sure about (see below) as warnings |
| `--strict`       | Report errors the checker is not sure about as errors |
| `--input-type`   | Type of the input document `.`, e.g. `h1 \| h2`, `code`, `markdown - code` (default: unknown) |
| `--format`       | Diagnostic output format: `text` (default), `json`, `markdown`, or `sarif` |

### `--strict-array`

Enforces that all elements in an array literal share the same type.

```bash
echo '[1, "hello"]' | mq-check --strict-array
# Error: heterogeneous array: [number, string]
```

### Definite and uncertain errors

A type error is *definite* only when every operand it is about is *exact*: its type is known for certain. Definite errors are errors. Every other error is *uncertain*: it may be wrong for a program that runs fine, so it is left out. `--show-uncertain` shows the uncertain errors as warnings, and `--strict` reports them as errors. Advice about the code (`nullable_propagation`, `unreachable_code`, `non_exhaustive_patterns`, `heterogeneous_array`) is always a warning.

An expression is exact when it is

- a literal, or an array or dict literal of exact elements
- an operator on exact operands
- a selector on the input document, when its type was declared with `--input-type`
- a call of an audited builtin (see below) with exact arguments, fed by an exact pipe stage
- an `if` whose branches are exact
- a reference to a `let` or `var` that is never assigned again, whose value is exact, and that no condition narrows around the reference
- an operation that gives the same type whatever its operands are, such as `to_string(x)` (a string) or `x == 1` (a bool)

A call of a function defined in the program is exact when it is given every parameter (or all but the piped one), the arguments are exact and the body is exact when its parameters are (a recursive function is not). Parameters outside such a call, loops, `match` and anything else are not exact, so an error on them is uncertain. An error inside a `try:` body is uncertain as well. A case the rules do not cover can only make an error uncertain, never definite.

Library users get the same split from `TypeChecker::check_classified`, which returns each error with its `Certainty`.

### Stricter than the runtime

The checker is stricter than the runtime on purpose. It accepts a program only when the types it infers fit the signature of every builtin and operator, so `len(1)` and `1 < "a"` are rejected even though they evaluate without an error. Calling a value that is no function (`let x = 1 | x(2)`) is rejected too.

### User-defined functions

A function is checked in two ways, so that a mistake is found whether or not the function is called.

- **In the body.** An operation fails whatever its operands of unknown type (a parameter, a loop variable) are when no overload takes any type for them, and that error is definite. `def f(x): x - [1];` and `def f(x): abs([x, 1]);` are errors before `f` is called. An array or dict literal counts as an array or dict even when its elements are unknown, and an operation that gives the same type whatever its operands are (`to_string(x)`, `x == 1`) has that type. A body that no type of its parameters lets run through (`def f(x): abs(x) | upcase(x);`) is an error too, for up to two parameters.
- **At a call.** The stages of the body that always run (not those inside `if`, `match`, loops, `try:` or a lambda) are run on the types of the arguments. An operation that nothing takes is reported at the call, so `def f(x): abs(x); | f("a")` is an error, also through another function (`def g(y): f(y); | g("a")`) for a piped call (`"a" | f()`) for a lambda that a builtin applies to the elements of an argument (`def f(xs): map(xs, fn(s): abs(s);); | f(["a"])`) and for a lambda that the function applies (`def apply(f, v): f(v); | apply(fn(x): abs(x);, "a")`). The error is definite when the arguments are exact, even if the rest of the body is not.

A default is used when the call leaves it out. A function with a variadic parameter, and a parameter that is assigned, is not run this way.

### Test files

`mq-check` treats `current_test_file` and `assert_snapshot`, which `mq-test` defines at runtime, as defined names. Library users can declare them with `Hir::declare_global` (see `mq_check::TEST_RUNNER_GLOBALS`).

### `--input-type`

Declares the type of the input document, so that selectors, predicates and attributes are checked against it. The type is a union (`|`) of node kinds (`h`, `h1`..`h6`, `code`, `list`, ...), optionally with kinds removed (`markdown - code`); `number`, `string`, `bool`, `none`, `symbol`, `bytes`, `dynamic` and arrays (`[h1]`) are also accepted.

```bash
echo '.depth' | mq-check --input-type 'h1 | h2'   # ok
echo '.depth' | mq-check --input-type code        # Error: undefined attribute `depth` on code
```

### CI Integration

`--format` controls how diagnostics are rendered, independent of `--show-types` and the other checks above:

- `text` (default): colored, human-readable report.
- `json`: a single JSON array of diagnostics (syntax errors/warnings and type errors) across every
  checked file, each with `file`, `severity` (`error`/`warning`), `code`, `message`, and an optional
  `range`.
- `markdown`: a Markdown table of diagnostics, suitable for pasting into a PR description or comment.
- `sarif`: a single SARIF 2.1.0 log covering every checked file, suitable for
  [`github/codeql-action/upload-sarif`](https://github.com/github/codeql-action/tree/main/upload-sarif) or any
  other SARIF-consuming tool.

```bash
mq-check --format json script.mq
mq-check --format markdown script.mq
mq-check --format sarif $(git ls-files '*.mq') > mq-check.sarif
```

```yaml
# .github/workflows/check.yml
- run: mq-check --format sarif $(git ls-files '*.mq') > mq-check.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: mq-check.sarif
```

Exits with a non-zero status if any error-severity diagnostic was found, regardless of `--format`.

## Development

### Running Tests

```bash
just test-all
```

### Building

```bash
cargo build -p mq-check
```

## License

MIT
