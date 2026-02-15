<h1 align="center">mq-typechecker</h1>

Type inference and checking for the mq.

## Features

- **Automatic Type Inference**: No type annotations required - types are inferred automatically
- **Hindley-Milner Algorithm**: Robust and proven type inference algorithm with constraint-based unification
- **HIR Integration**: Works seamlessly with mq's High-level Internal Representation
- **Error Collection**: Reports multiple type errors in a single pass
- **Detailed Error Messages**: Clear and actionable type error messages with source locations (line, column, span)
- **Readable Type Names**: Type errors display resolved type names (e.g., "number", "string") instead of raw type variables
- **Builtin Type Signatures**: Comprehensive type signatures for 100+ builtin functions and operators
- **User-Defined Function Type Checking**: Detects argument type mismatches, arity errors, and return type propagation errors

## Usage

### As a Library

```rust
use mq_hir::Hir;
use mq_typechecker::TypeChecker;

// Build HIR from mq code
let mut hir = Hir::default();
hir.add_code(None, "def add(x, y): x + y;");
hir.resolve();

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

### Type Inference

The type checker uses Hindley-Milner type inference, which means:

1. **No annotations required**: Types are inferred from usage
2. **Principal types**: Every expression has a most general type
3. **Parametric polymorphism**: Functions can work with multiple types

Example:

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
