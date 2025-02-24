# Conditionals and Comparisons

## Conditionals

mq supports standard conditional operations through the following functions:

- `and(a, b)` - Returns true if both `a` and `b` are true
- `or(a, b)` - Returns true if either `a` or `b` is true
- `not(a)` - Returns true if `a` is false

## Comparisons

mq provides comparison functionality through built-in functions.

### Basic Comparisons

Standard comparison operators are supported:

- `eq(a, b)` - Returns true if `a` equals `b`
- `ne(a, b)` - Returns true if `a` does not equal `b`
- `gt(a, b)` - Returns true if `a` is greater than `b`
- `gte(a, b)` - Returns true if `a` is greater than or equal to `b`
- `lt(a, b)` - Returns true if `a` is less than `b`
- `lte(a, b)` - Returns true if `a` is less than or equal to `b`

### Logical Operators

Boolean logic operations are supported:

- `and(a, b)` - Logical AND
- `or(a, b)` - Logical OR
- `not(a)` - Logical NOT

### Examples

```jq
# Basic comparisons
eq(1, 1) // true
gt(2, 1) // true
lte("a", "b") // true

# String comparisons
eq("hello", "hello") // true
gt("xyz", "abc") // true

# Numeric comparisons
gte(5.5, 5.0) // true
lt(-1, 0) // true

# Logical operations
and(true, false) // false
or(true, false) // true
not(false) // true

# Complex conditions
and(gt(x, 0), lt(x, 10)) // true if 0 < x < 10
```
