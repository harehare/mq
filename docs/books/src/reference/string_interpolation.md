# String Interpolation

String Interpolation allow embedding expressions directly inside string literals. In mq, an interpolated string is prefixed with `s"` and variables can be embedded using `${}` syntax.

## Syntax

```
s"text ${ident} more text"
```

## Escaping

You can escape the `$` character in a string interpolation by using `$$`.
This allows you to include literal `$` symbols in your interpolated strings.

```python
let price = 25
| s"The price is $$${price}"
# => Output: "The price is $25"
```

## Examples

```python
let name = "Alice"
| let age = 30
| s"Hello, my name is ${name} and I am ${age} years old."
# => Output: "Hello, my name is Alice and I am 30 years old."
```
