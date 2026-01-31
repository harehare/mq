# Self

The current value being processed can be referenced as `self` or `.` (dot). Both `self` and `.` behave identically. When there are insufficient arguments provided in a method call, the current value (`self`) is automatically passed as the first argument.

### Examples

```mq
# These expressions are equivalent
"hello" | upcase()
"hello" | upcase(self)
"hello" | upcase(.)
```
