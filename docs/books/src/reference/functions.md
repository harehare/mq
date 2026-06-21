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

### Variadic Parameters

A function can accept a variable number of arguments using a `*` prefix on its last parameter. The variadic parameter collects all remaining arguments into an array.

```mq
def function_name(param1, *rest):
  program;
```

```mq
# Collect all arguments into an array
def all_args(*args):
  args;

all_args(1, 2, 3)
# Output: [1, 2, 3]

# Combine regular and variadic parameters
def first_and_rest(a, *rest):
  rest;

first_and_rest(1, 2, 3)
# Output: [2, 3]

# Variadic parameter is an empty array when no extra arguments are passed
def first_and_rest(a, *rest):
  rest;

first_and_rest(1)
# Output: []
```

A variadic parameter:

- Must be the **last** parameter in the parameter list
- Can only be declared **once** per function
- Is **not allowed** in `macro` definitions

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

### Variadic Parameters

Anonymous functions support variadic parameters the same way named functions do:

```mq
let sum_all = fn(*args): sum(args);

sum_all(1, 2, 3)
# Output: 6
```

## Rules

- Parameters with default values must come after parameters without default values
- Default values are evaluated when the function is called, not when it is defined
- Default values can be any valid expression
- A variadic parameter (`*name`) must be the last parameter, can appear at most once, and is not allowed in `macro` definitions

## Pipeline Expressions As Arguments

Pipeline expressions can be passed directly as function arguments. The pipeline is treated as one argument until the next comma or closing parenthesis.

```mq
array("a" | upcase(), "b" | upcase())
# Output: ["A", "B"]
```

## Parenthesis-Free Calls

Functions with 0 or 1 required parameters can be called without parentheses when used as pipeline steps.

- A **0-argument function** invoked without `()` is called with no explicit arguments.
- A **1-argument function** invoked without `()` receives the current pipeline value as its implicit argument.

This only applies in **pipeline position** (as a pipeline step). When a function is passed as a value to another function (e.g., `map(arr, f)`), no auto-call occurs and the function reference is preserved.

```mq
# 0-arg function: called without parentheses
def greet(): "Hello!";
| greet # equivalent to greet()
# Output: "Hello!"

# 1-arg function: current value is passed implicitly
def double(x): x * 2;
| 5 | double      # equivalent to 5 | double(5), i.e., double(5)
# Output: 10

# Builtin functions also support paren-free calls
"hello world" | upcase    # equivalent to upcase("hello world")
# Output: "HELLO WORLD"

[1, None, 2] | compact | len  # chained paren-free calls
# Output: 2

# Function references are preserved when passed as arguments
map(["a", "b"], upcase)   # upcase is NOT auto-called here; it's passed as a callback
# Output: ["A", "B"]
```
