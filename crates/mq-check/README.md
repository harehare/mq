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
- `(T1, T2, ..., Tn)`: Tuple with known per-element types (enabled with `--tuple`)
- `{K: V}`: Dictionary with keys of type K and values of type V
- `(T1, T2, ..., Tn) -> R`: Function taking arguments T1..Tn and returning R

### Union Types

When a control flow construct (such as `if`, `while`, `loop`, `foreach`, `match`, or `try/catch`) may produce values of different types depending on the branch taken, the type checker infers a **union type** — written as `T1 | T2 | ...`.

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

### Example:

```mq
def identity(x): x;
// Inferred type: forall 'a. ('a) -> 'a

def map_array(f, arr): arr | map(f);
// Inferred type: forall 'a 'b. (('a) -> 'b, ['a]) -> ['b]
```

### Error Messages

Type errors display clear, readable type names:

```mq
[1, 2, "string"]
// Error: Type mismatch: expected number, found string
//   at line 1, column 7
```

Type variables are displayed with readable names (e.g., `'1v0`, `'2v1`) when unresolved, and as concrete types (e.g., `number`, `string`) when resolved.

## CLI Options

When used as a command-line tool (`mq-typecheck`), the following options are available:

| Option           | Description                                                        |
| ---------------- | ------------------------------------------------------------------ |
| `--show-types`   | Display inferred types for all user-defined symbols                |
| `--no-builtins`  | Disable automatic builtin preloading                               |
| `--strict-array` | Reject heterogeneous arrays (e.g., `[1, "hello"]` is a type error) |
| `--tuple`        | Type heterogeneous array literals as tuples with per-element types |

### `--strict-array`

Enforces that all elements in an array literal share the same type.

```bash
echo '[1, "hello"]' | mq-check --strict-array
# Error: heterogeneous array: [number, string]
```

### `--tuple`

Types heterogeneous array literals as **tuple types** instead of generic arrays.
This enables precise per-element type tracking: `v[0]` returns the type of the
first element, `v[1]` returns the type of the second, and so on.

```bash
echo 'let v = [1, "hello"] | v[0] + 1' | mq-check --tuple
# ✓ No type errors found.  (v[0] is number)

echo 'let v = [1, "hello"] | v[1] - 1' | mq-check --tuple
# Error: type mismatch: expected number, found string  (v[1] is string)
```

When the index is a variable (dynamic), the element type becomes the union of all
element types:

```mq
let v = [1, "hello"]
// v[i]  →  number | string
```

Homogeneous arrays are unaffected — `[1, 2, 3]` remains `[number]` even in tuple mode.

## Development

### Running Tests

```bash
just test
```

### Building

```bash
cargo build -p mq-check
```

## License

MIT
