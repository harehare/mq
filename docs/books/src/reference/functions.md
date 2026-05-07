# Functions

mq supports named functions defined with `def`, anonymous functions defined with `fn`, and the `->` arrow shorthand.

## Named Functions

Named functions are defined with `def` and can be called by name throughout the program.

### Syntax

A function body can be terminated with `;` or `end`:

```
def function_name(parameters):
  program;

def function_name(parameters):
  program
end
```

### Examples

```mq
# Using semicolon terminator
def double(x):
  mul(x, 2);

# Using end terminator
def double(x):
  mul(x, 2)
end

# Function with conditional logic
def is_positive(x):
  gt(x, 0);

# Composition of functions
def add_then_double(x, y):
  add(x, y) | double(self);
```

### Default Parameters

```mq
def function_name(param1, param2=default_value):
  program;
```

```mq
# Function with default parameter
def greet(name, greeting="Hello"):
  greeting + " " + name;

# Using end terminator
def greet(name, greeting="Hello"):
  greeting + " " + name
end

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

## Anonymous Functions

Anonymous functions (lambda expressions) are defined with `fn` or the `->` shorthand, and can be passed as arguments, assigned to variables, or used inline.

### Syntax

A function body can be terminated with `;` or `end`:

```
fn(parameters): program;

fn(parameters): program end
```

The `->` syntax is a shorthand alias for `fn`:

```
->(parameters): program;

->(parameters): program end
```

### Examples

```mq
# Basic anonymous function
nodes | map(fn(x): add(x, "1");)

# Using end terminator
nodes | map(fn(x): add(x, "1") end)

# Using arrow syntax
nodes | map(->(x): add(x, "1");)

# As a callback
nodes | .[] | sort_by(fn(x): to_text(x);)

# Assigned to a variable
let multiply = fn(x, factor=2): x * factor;
```

### Default Parameters

```mq
fn(param1, param2=default_value): program;
```

```mq
# Anonymous function with default parameter
let multiply = fn(x, factor=2): x * factor;

# Using end terminator
let multiply = fn(x, factor=2): x * factor end

# Using default value
multiply(10)
# Multiplies by 2 (default factor)

# Overriding default value
multiply(10, 3)
# Multiplies by 3

# Using in callbacks
[1, 2] | map(fn(x, prefix="Item: "): prefix + to_text(x);)
```

## Rules

- Parameters with default values must come after parameters without default values
- Default values are evaluated when the function is called, not when it is defined
- Default values can be any valid expression
