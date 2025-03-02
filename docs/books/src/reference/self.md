# Self

The current value being processed can be referenced as `self`. When there are insufficient arguments provided in a method call, the current value (`self`) is automatically passed as the first argument.

### Examples

```python
# These expressions are equivalent
"hello" | upcase()
"hello" | upcase(self)
```
