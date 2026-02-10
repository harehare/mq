<h1 align="center">mq-typechecker</h1>

Type inference and checking for the mq language using Hindley-Milner type inference.

## Features

- **Automatic Type Inference**: No type annotations required - types are inferred automatically
- **Hindley-Milner Algorithm**: Robust and proven type inference algorithm
- **HIR Integration**: Works seamlessly with mq's High-level Internal Representation
- **CLI Support**: Type check mq files from the command line
- **Detailed Error Messages**: Clear and actionable type error messages with source locations
- **Readable Type Names**: Type errors display resolved type names (e.g., "number", "string") instead of raw type variables

## Architecture

The type checker consists of several key components:

### Type Representation (`types.rs`)

- `Type`: Represents types in the mq type system
  - Basic types: `Int`, `Float`, `Number`, `String`, `Bool`, `Symbol`, `None`, `Markdown`
  - Composite types: `Array<T>`, `Dict<K, V>`, `Function(Args) -> Ret`
  - Type variables: `Var(TypeVarId)` for inference
- `TypeScheme`: Polymorphic type schemes with quantified type variables
- `TypeVarContext`: Manages fresh type variable generation
- `Substitution`: Type variable substitutions for unification

### Constraint Generation (`constraint.rs`)

Generates type constraints from HIR symbols:
- Assigns concrete types to literals
- Creates fresh type variables for unknowns
- Generates equality constraints for references
- Handles function calls, arrays, dictionaries, control flow, etc.

### Unification (`unify.rs`)

Implements the unification algorithm:
- Unifies types to find a consistent assignment
- Performs occurs checks to prevent infinite types
- Applies substitutions to resolve type variables
- Handles complex types (arrays, dicts, functions)

### Inference Engine (`infer.rs`)

Coordinates the type inference process:
- Maintains type variable context
- Stores symbol-to-type mappings
- Collects and solves constraints
- Finalizes inferred types into type schemes

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
match type_checker.check(&hir) {
    Ok(()) => {
        // Type checking succeeded
        for (symbol_id, type_scheme) in type_checker.symbol_types() {
            println!("{:?} :: {}", symbol_id, type_scheme);
        }
    }
    Err(err) => {
        // Type checking failed
        eprintln!("Type error: {}", err);
    }
}
```

### From the CLI

Type check mq files:

```bash
# Basic type checking
mq check file.mq

# With verbose output showing inferred types
mq check --verbose file.mq

# Multiple files
mq check file1.mq file2.mq file3.mq
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

## Limitations & Future Work

### Current Limitations

1. **Builtin function signatures**: Builtin function types are not yet fully specified
2. **Polymorphic generalization**: Currently creates monomorphic type schemes
3. **Pattern matching**: Limited support for complex pattern types

### Planned Enhancements

- [ ] Complete builtin function type signatures
- [ ] Implement full polymorphic type generalization
- [ ] Add support for union types (e.g., `string | number`)
- [ ] Add support for structural typing for dictionaries
- [ ] Add support for type aliases
- [ ] Incremental type checking for LSP
- [ ] Improve source span accuracy for better error reporting

## Development

### Running Tests

```bash
cargo test -p mq-typechecker
```

### Building

```bash
cargo build -p mq-typechecker
```

## References

- **Hindley-Milner Type System**: The foundational type inference algorithm
- **Algorithm W**: The type inference algorithm implementation
- **mq-hir**: HIR provides symbol and scope information

## License

See the main mq project license (MIT).
