# Until Expression

The until loop repeatedly executes code until a condition becomes true:

```python
let i = 10 |
until (eq(i, 0)):
  # Do something
  let i = sub(i, 1) | i;
# => 0
```

Until loops are similar to while loops but continue until the condition becomes true
instead of while the condition remains true.
