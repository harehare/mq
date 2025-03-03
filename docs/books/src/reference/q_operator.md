# ? Operator

The ? operator is a safe navigation operator that provides null-safe operations.

### Usage

When applied to a None value, the ? operator prevents errors by returning None instead of raising an exception.

### Examples

```python
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
