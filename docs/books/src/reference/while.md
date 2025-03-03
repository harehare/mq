# While Expression

The while loop repeatedly executes code while a condition is true:

```python
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
