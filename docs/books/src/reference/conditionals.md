# Conditionals

mq supports standard conditional operations through the following functions:

- `and(a, b)` - Returns true if both `a` and `b` are true
- `or(a, b)` - Returns true if either `a` or `b` is true
- `not(a)` - Returns true if `a` is false

### Examples

```python
# Basic comparisons
and(true, true, true) // true
or(true, false, true) // true
not(false) // true
```
