# Comparisons

mq provides comparison functionality through built-in functions.

### Basic Comparisons

Standard comparison operators are supported:

- `eq(a, b), a == b` - Returns true if `a` equals `b`
- `ne(a, b), a != b` - Returns true if `a` does not equal `b`
- `gt(a, b), a > b` - Returns true if `a` is greater than `b`
- `gte(a, b), a >= b` - Returns true if `a` is greater than or equal to `b`
- `lt(a, b), a < b` - Returns true if `a` is less than `b`
- `lte(a, b), a <= b` - Returns true if `a` is less than or equal to `b`

### Examples

```python
# Basic comparisons
1 == 1
# => true
2 > 1
# => true
"a" <= "b"
# => true

# String comparisons
"hello" == "hello"
# => true
"xyz" > "abc"
# => true

# Numeric comparisons
5.5 >= 5.0
# => true
-1 < 0
# => true

# Logical operations
and(true, false)
# => false
or(true, false)
# => true
not(false)
# => true

# Complex conditions
and(x > 0, x < 10)
# =>  true if 0 < x < 10
```
