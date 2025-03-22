# String Interpolation

String Interpolation allow embedding expressions directly inside string literals. In mq, an interpolated string is prefixed with `s"` and variables can be embedded using `${}` syntax.

## Syntax

```
s"text ${expression} more text"
```

## Examples

```python
let name = "Alice"
| let age = 30
| s"Hello, my name is ${name} and I am ${age} years old."
# => Output: "Hello, my name is Alice and I am 30 years old."
```
