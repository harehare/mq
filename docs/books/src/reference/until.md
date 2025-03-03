# Until Expression

The until loop repeatedly executes code until a condition becomes true:

```python
let x = 5 |
until(gt(x, 0)):
  let x = sub(x, 1) | x;
# => 0
```

Until loops are similar to while loops but continue until the condition becomes true
instead of while the condition remains true.
