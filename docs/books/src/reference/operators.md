# Operator

## Pipe Operator

A functional operator that allows chaining multiple filter operations together.

### Usage

The pipe operator (`|`) enables sequential processing of filters, where the output of one filter becomes the input of the next filter.

### Examples

```js
# Basic pipe usage
42 | add(1) | mul(2)
# => 86

# Multiple transformations
let mul2 = def mul2(x): mul(x, 2);
let gt4 = def gt4(x): gt(x, 4);
array(1, 2, 3) | map(mul2) | filter(gt4)
# => [6]

# Function composition
let double = def _double(x): mul(x, 2);
let add_one = def _add_one(x): add(x, 1);
5 | double(self) | add_one(self)
# => 11
```

## .. Operator

The range operator (`..`) creates sequences of consecutive values between a start and end point.

### Usage

The range operator generates arrays of values from a starting point to an ending point (inclusive). It works with both numeric values and characters.

### Examples

```js
# Numeric ranges
1..5
# => [1, 2, 3, 4, 5]

# Character ranges
'a'..'e'
# => ["a", "b", "c", "d", "e"]

# Using ranges with other operations
1..3 | map(fn(x): mul(x, 2);)
# => [2, 4, 6]

# Reverse ranges
5..1
# => [5, 4, 3, 2, 1]

# Single element range
3..3
# => [3]
```
