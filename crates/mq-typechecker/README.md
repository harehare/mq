<h1 align="center">mq-typechecker</h1>

> [!IMPORTANT]
> This crate is currently **under active development**. The type inference and checking features are experimental, and the API or behavior may change without notice. Use in production environments is not recommended.

Type inference and checking for the mq language.


## Usage

### As a Library

```rust
use mq_hir::Hir;
use mq_typechecker::TypeChecker;

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

## Development

### Running Tests

```bash
just test
```

### Building

```bash
cargo build -p mq-typechecker
```

## License

MIT
