# mq-syntax

Lexer, AST parser and CST parser for the [mq](https://github.com/harehare/mq) query language.

`mq-syntax` has no runtime or Markdown dependencies. Use it to analyze or format mq source
without the evaluator. [`mq-lang`](../mq-lang) builds on it and re-exports its types.

```rust
use mq_syntax::{Arena, Lexer, LexerOptions, TOP_LEVEL_MODULE_ID, ast::parser::Parser};

let tokens = Lexer::new(LexerOptions::default())
    .tokenize("1 + 2", TOP_LEVEL_MODULE_ID)
    .unwrap();
let mut arena = Arena::new(16);
let program = Parser::new(tokens.iter(), &mut arena, TOP_LEVEL_MODULE_ID)
    .parse()
    .unwrap();
assert_eq!(program.len(), 1);
```

## Features

- `cst`: the error-recovering CST parser (`parse_recovery`).
- `ast-json`: JSON (de)serialization of the AST and tokens.
- `sync`: `Arc`/`RwLock` instead of `Rc`/`RefCell` for `Shared`/`SharedCell`.
- `expr-display`: `Display` for AST expressions.
