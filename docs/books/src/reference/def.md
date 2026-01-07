# Def Expression

The def expression defines reusable functions with parameters:

## Syntax

```
def function_name(parameters):
  program;
```

## Examples

```python
# Function that doubles input
def double(x):
  mul(x, 2);

# Function with conditional logic
def is_positive(x):
  gt(x, 0);

# Composition of functions
def add_then_double(x, y):
  add(x, y) | double(self);
```

## Default Parameters

You can define default values for function parameters. Parameters with default values can be omitted when calling the function.

### Syntax

```python
def function_name(param1, param2=default_value):
  program;
```

### Examples

```python
# Function with default parameter
def greet(name, greeting="Hello"):
  greeting + " " + name;

# Using default value
greet("Alice")
# Output: "Hello Alice"

# Overriding default value
greet("Bob", "Hi")
# Output: "Hi Bob"

# Default value can be an expression
def add_with_offset(x, offset=10 + 5):
  x + offset;

add_with_offset(20)
# Output: 35
```

### Rules

- Parameters with default values must come after parameters without default values
- Default values are evaluated when the function is called, not when it's defined
- Default values can be any valid expression
