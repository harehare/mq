# mdq-lang

`mdq-lang` is provides a parser and evaluator for a mdq language.

### Examples

```rs
use mdq_lang::Engine;

let code = "add(\"world!\")";
let input = vec![mdq_lang::Value::Markdown(
  mdq_md::Markdown::from_str("Hello,").unwrap()
)].into_iter();
let mut engine = mdq_lang::Engine::default();

assert!(matches!(engine.eval(&code, input).unwrap(), mdq_lang::Value::String("Hello,world!".to_string())));
```

## Modules

- `ast`: Abstract Syntax Tree (AST) structures and parser.
- `cst`: Concrete Syntax Tree (CST) structures and parser.
- `engine`: Execution engine for evaluating mdq code.
- `error`: Error handling utilities.
- `eval`: Evaluation logic and built-in functions.
- `lexer`: Lexical analysis and tokenization.
- `optimizer`: Code optimization utilities.
- `value`: Value types used in the language.
