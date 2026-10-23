<div style="display: flex; align-items: center;">
    <img src="../images//logo.svg" style="width: 96px; height: 96px; margin-right: 10px;"/>
    <div><h1>Syntax</h1></div>
</div>

This section outlines the syntax rules in mq, providing a clear reference for writing valid code.

## Statements and Pipes

A program is a sequence of expressions joined by the pipe operator (`|`). The result of each expression is the input of the next one.

A `|` is required between two statements. Two expressions written next to each other are a syntax error, for example `let x = 1 let y = 2`.

```mq
let x = 1
| let y = 2
| x + y
# => 3
```

Array elements and call arguments are separated by commas. A trailing comma is allowed.

### Constructs that need no pipe after them

A construct that ends with its own `;` or `end` can be followed directly by the next statement. This applies to `def`, `fn`, `do ... end`, `foreach`, `while`, `until`, `loop`, `match` and `module`, and to `import` and `include`.

```mq
import "csv"
def double(x): x * 2;
| double(21)
# => 42
```
