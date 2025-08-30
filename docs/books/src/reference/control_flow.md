# Control flow

## If Expression

The if expression evaluates a condition and executes code based on the result:

```js
 if (eq(x, 1)):
   "one"
 elif (eq(x, 2)):
   "two"
 else:
   "other"
```

```python
 if (eq(x, 1)):
   "one"
```

The if expression can be nested and chained with elif and else clauses.
The conditions must evaluate to boolean values.

## While Expression

The while loop repeatedly executes code while a condition is true:

```ruby
let i = 0 |
while (lt(i, 3)):
  let i = add(i, 1) | i
end
# => [1, 2, 3]
```

The `while` loop in this context returns an array containing all elements processed during the iteration. As the loop executes, it collects each processed value into an array, which is then returned as the final result once the loop condition becomes false.

Key points:

- Creates a new array from loop iterations
- Each loop cycle's result is added to the array
- Returns the complete array after all iterations
- Similar to map/collect functionality but with while loop control

## Until Expression

The until loop repeatedly executes code until a condition becomes true:

```ruby
let x = 5 |
until(gt(x, 0)):
  let x = sub(x, 1) | x
end
# => 0
```

Until loops are similar to while loops but continue until the condition becomes true
instead of while the condition remains true.


## Foreach Expression

The foreach loop iterates over elements in an array:

```ruby
let items = array(1, 2, 3) |
foreach (x, items):
   sub(x, 1)
end
# => array(0, 1, 2)
```

Foreach loops are useful for:

- Processing arrays element by element
- Mapping operations across collections
- Filtering and transforming data
