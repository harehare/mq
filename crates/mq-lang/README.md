# mq-lang

Core language implementation for mq query language.

## Overview

`mq-lang` provides a parser and evaluator for the [mq](https://github.com/harehare/mq) query language. It handles parsing, evaluation, and execution of mq queries.

## Examples

### Basic Evaluation

```rust
use mq_lang::{DefaultEngine, Value};
use mq_markdown::Markdown;

let code = "add(\"world!\")";
let input = vec![Value::Markdown(
    "Hello,".parse::<Markdown>().unwrap()
)].into_iter();
let mut engine = DefaultEngine::default();

let result = engine.eval(code, input).unwrap();
// Result: Value::String("Hello,world!".to_string())
```

### Parsing Code

```rust
use mq_lang::parse_recovery;

let code = "1 + 2";
let (cst_nodes, errors) = parse_recovery(code);

assert!(!errors.has_errors());
assert!(!cst_nodes.is_empty());
```

## Features

- `ast-json`: Enables serialization and deserialization of the AST (Abstract Syntax Tree) to/from JSON format
- `cst`: Enables Concrete Syntax Tree support for error recovery parsing
- `debugger`: Enables debugging support (requires `sync` feature)
- `file-io`: Enables file I/O operations
- `sync`: Enables thread-safe operations
- `std`: Enables standard library support (default)

## License

Licensed under the MIT License.
