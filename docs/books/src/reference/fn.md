# Fn Expression

Anonymous functions (lambda expressions) allow you to define functions inline without naming them.
These functions can be passed as arguments to other functions, assigned to variables, or used directly in expressions.

## Syntax

```
fn(parameters): program;
```

## Examples

```python
# Basic Anonymous Function
nodes | map(fn(x): add(x, "1");)

# Using Anonymous Functions as Callbacks
nodes | .[] | sort_by(fn(x): to_text(x);)
```
