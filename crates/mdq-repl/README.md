# mdq-repl

This module contains the main library code for the `mdq-repl` crate.

It includes the following submodules:

- `command_context`: Handles the context for commands executed within the REPL.
- `repl`: Contains the implementation of the Read-Eval-Print Loop (REPL).

## Example

```rust
use mdq_repl::Repl;

let repl = mdq_repl::Repl::new(vec![mdq_lang::Value::String("".to_string())]);
repl.run().unwrap();
```
