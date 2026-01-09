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

## Default Parameters

Anonymous functions also support default parameter values, just like named functions defined with `def`.

### Syntax

```python
fn(param1, param2=default_value): program;
```

### Examples

```python
# Anonymous function with default parameter
let multiply = fn(x, factor=2): x * factor;

# Using default value
multiply(10)
# Multiplies each value by 2 (default factor)

# Overriding default value
multiply(10, 3)
# Multiplies each value by 10

# Using in callbacks
[1, 2] | map(fn(x, prefix="Item: "): prefix + to_text(x);)
```

### Rules

- Parameters with default values must come after parameters without default values
- Default values are evaluated when the function is called
- Default values can be any valid expression
