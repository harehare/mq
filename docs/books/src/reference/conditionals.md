# Conditionals

mq supports standard conditional operations through the following functions:

- `and(a, b)`, `a && b` - Returns true if both `a` and `b` are true
- `or(a, b), a || b` - Returns true if either `a` or `b` is true
- `not(a), !a` - Returns true if `a` is false

### Examples

```mq
# Basic comparisons
and(true, true, true)
true && true && true
# => true
or(true, false, true)
true || false || true
# => true
not(false)
!false
# => true
```
