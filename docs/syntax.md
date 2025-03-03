# Syntax

## Pipe Operator

A functional operator that allows chaining multiple filter operations together.

### Usage

The pipe operator (`|`) enables sequential processing of filters, where the output of one filter becomes the input of the next filter.

### Examples

```jq
# Basic pipe usage
42 | add(1) | mul(2)
# => 86

# Multiple transformations
let mul2 = def mul2(x): mul(x, 2);
let gt4 = def gt4(x): gt(x, 4);
array(1, 2, 3) | map(mul2) | filter(gt4)
# => [6]

# Function composition
let double = def _double(x): mul(x, 2);
let add_one = def _add_one(x): add(x, 1);
5 | double(self) | add_one(self)
# => 11
```

## ? Operator

The ? operator is a safe navigation operator that provides null-safe operations.

### Usage

When applied to a None value, the ? operator prevents errors by returning None instead of raising an exception.

### Examples

```jq
# Safe access with ? operator
let x = None | x | add?(1)
# => None

# Chaining with ? operator
None | add?(1) | mul?(2)
# => None

# Normal operation when value exists
42 | add?(1)
# => 43
```

## Environment variables

Environment variables can be referenced using $XXX syntax, where XXX represents the name of the environment variable. For example:

- `$PATH` - References the PATH environment variable
- `$HOME` - References the HOME environment variable
- `$USER` - References the current user's username

This syntax is commonly used in shell scripts and configuration files to access system-level environment variables.

## Def Expression

The def expression defines reusable functions with parameters:

### Examples

```jq
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

## Let Expression

The let expression binds a value to an identifier for later use:

```jq
# Binds 42 to x
let x = 42
# Uses x in an expression
let y = add(x, 1)
# Binds `add` function to z
let z = def _add(x): add(x, 1); | z(1);
```

## If Expression

The if expression evaluates a condition and executes code based on the result:

```jq
 if (eq(x, 1)):
   "one"
 elif (eq(x, 2)):
   "two"
 else:
   "other"
```

The if expression can be nested and chained with elif and else clauses.
The conditions must evaluate to boolean values.

## While Expression

The while loop repeatedly executes code while a condition is true:

```jq
let i = 0 |
while (lt(i, 3)):
  # Do something
  let i = add(i, 1) | i;
# => [0, 1, 2, 3]
```

The `while` loop in this context returns an array containing all elements processed during the iteration. As the loop executes, it collects each processed value into an array, which is then returned as the final result once the loop condition becomes false.

Key points:

- Creates a new array from loop iterations
- Each loop cycle's result is added to the array
- Returns the complete array after all iterations
- Similar to map/collect functionality but with while loop control

## Until Expression

The until loop repeatedly executes code until a condition becomes true:

```jq
let i = 10 |
until (eq(i, 0)):
  # Do something
  let i = sub(i, 1) | i;
# => 0
```

Until loops are similar to while loops but continue until the condition becomes true
instead of while the condition remains true.

## Foreach Expression

The foreach loop iterates over elements in an array:

```jq
let items = array(1, 2, 3) |
foreach (x, items):
   # Do something
   sub(x, 1);
# => array(0, 1, 2)
```

Foreach loops are useful for:

- Processing arrays element by element
- Mapping operations across collections
- Filtering and transforming data

## Comments

Similar to jq, comments starting with `#` are doc-comments.

```jq
# doc-comment
let value = add(2, 3)
```

## Include

Loads functions from an external file using the syntax `include "module_name"`.
The include directive searches for .mq files in the following locations:

- `$HOME/.mq` - User's home directory mq folder
- `$ORIGIN/../lib/mq` - Library directory relative to the source file
- `$ORIGIN/../lib` - Parent lib directory relative to the source file

```jq
include "module_name"
```

### Examples

```jq
# Include math functions from math.mq
include "math"
# Now we can use functions defined in math.mq
let result = add(2, 3)
```

## Self

The current value being processed can be referenced as `self`. When there are insufficient arguments provided in a method call, the current value (`self`) is automatically passed as the first argument.

### Examples

```jq
# These expressions are equivalent
"hello" | upcase()
upcase("hello")
```
