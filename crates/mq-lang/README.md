# mq-lang

`mq-lang` provides a parser and evaluator for a [mq](https://github.com/harehare/mq).

### Examples

```rs
use mq_lang::Engine;

let code = "add(\"world!\")";
let input = vec![mq_lang::Value::Markdown(
  mq_markdown::Markdown::from_str("Hello,").unwrap()
)].into_iter();
let mut engine = mq_lang::Engine::default();

assert!(matches!(engine.eval(&code, input).unwrap(), mq_lang::Value::String("Hello,world!".to_string())));

// Parse code into AST nodes
use mq_lang::{tokenize, LexerOptions, AstParser, Arena};
use std::rc::Shared;
use std::cell::SharedCell;

let code = "1 + 2";
let token_arena = Shared::new(SharedCell::new(Arena::new()));
let ast = mq_lang::parse(code, token_arena).unwrap();

assert_eq!(ast.nodes.len(), 1);

// Parse code into CST nodes
use mq_lang::{tokenize, LexerOptions, CstParser};
use std::sync::Arc;

let code = "1 + 2";
let (cst_nodes, errors) = mq_lang::parse_recovery(code);

assert!(!errors.has_errors());
assert!(!cst_nodes.is_empty());
```

### Features

- `ast-json`: Enables serialization and deserialization of the AST (Abstract Syntax Tree)
  to/from JSON format. This also enables the `Engine::eval_ast` method for direct
  AST execution. When this feature is enabled, `serde` and `serde_json` dependencies
  are included.
