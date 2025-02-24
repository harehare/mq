# mq-repl

This module contains the main library code for the `mq-repl` crate.

It includes the following submodules:

- `command_context`: Handles the context for commands executed within the REPL.
- `repl`: Contains the implementation of the Read-Eval-Print Loop (REPL).

## Example

```rust
use mq_repl::Repl;

let repl = mq_repl::Repl::new(vec![mq_lang::Value::String("".to_string())]);
repl.run().unwrap();
```
