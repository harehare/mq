<div style="display: flex; align-items: center;">
    <img src="../images//logo.svg" style="width: 96px; height: 96px; margin-right: 10px;"/>
    <div><h1>Syntax</h1></div>
</div>

This section outlines the syntax rules in mq, providing a clear reference for writing valid code.


## Statements and Pipes

A program is a sequence of expressions joined by the pipe operator (`|`). The result of each expression is the input of the next one.

A `|` is required between two statements. Two expressions written next to each other are a syntax error:

```text
let x = 1 let y = 2   # error: needs `|` between the bindings
[1] 7                 # error
x[0] 7                # error
```

```mq
let x = 1
| let y = 2
| x + y
# => 3
```

### Constructs that need no pipe after them

A construct that ends with its own `;` or `end` can be followed directly by the next statement. This applies to `def`, `fn`, `do ... end`, `foreach`, `while`, `until`, `loop`, `match` and `module`, and to `import` and `include`.

```mq
import "csv"
def double(x): x * 2;
| double(21)
# => 42
```

### Lists and entries

Array elements and call arguments are separated by commas, and a comma needs an element before it. A trailing comma is allowed.

```text
[1 2]        # error: missing comma
[1,, 2]      # error: doubled comma
[, 1]        # error: leading comma
```

### Match arms

Each `match` arm is written `| pattern: body`. The colon is required, also when the body is a `do` block.

```mq
match (x):
  | 1: "one"
  | _: do "other" end
end
```

### Module bodies

A `module` body holds `let`, `def`, `module` and `import` only.

### Selector chains

Selectors written one after another form a descendant chain (`.blockquote .code`). A trailing attribute such as `.lang` ends the chain, so nothing selector-like may follow it without a `|`. See [Selectors](selectors.md#descendant-selectors).
