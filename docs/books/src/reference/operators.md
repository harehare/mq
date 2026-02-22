# Operator

## Pipe Operator

A functional operator that allows chaining multiple filter operations together.

### Usage

The pipe operator (`|`) enables sequential processing of filters, where the output of one filter becomes the input of the next filter.

### Examples

```mq
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

## Shift Operators

The shift operators (`<<` and `>>`) perform different operations depending on the type of the operand.

### Left Shift (`<<`)

The left shift operator (`<<`) maps to the `shift_left(value, amount)` builtin function.

| Operand type     | Behavior                                                        |
|------------------|-----------------------------------------------------------------|
| Number           | Bitwise left shift: multiplies the value by `2^amount`          |
| String           | Removes `amount` characters from the **start** of the string    |
| Markdown Heading | Decreases the heading depth by `amount` (promotes the heading, e.g. `##` → `#`), minimum depth is 1 |

#### Examples

```mq
# Bitwise left shift on numbers
1 << 2
# => 4

shift_left(1, 3)
# => 8

# Remove characters from the start of a string
shift_left("hello", 2)
# => "llo"

"hello" << 2
# => "llo"

# Promote a heading (decrease depth)
let md = do to_markdown("## Heading 2") | first(); |
md << 1
# => # Heading 2
```

### Right Shift (`>>`)

The right shift operator (`>>`) maps to the `shift_right(value, amount)` builtin function.

| Operand type     | Behavior                                                        |
|------------------|-----------------------------------------------------------------|
| Number           | Bitwise right shift on the truncated integer value (shifts the bits right by `amount`) |
| String           | Removes `amount` characters from the **end** of the string      |
| Markdown Heading | Increases the heading depth by `amount` (demotes the heading, e.g. `#` → `##`), maximum depth is 6 |

#### Examples

```mq
# Bitwise right shift on numbers
4 >> 2
# => 1

shift_right(8, 2)
# => 2

# Remove characters from the end of a string
shift_right("hello", 2)
# => "hel"

"hello" >> 2
# => "hel"

# Demote a heading (increase depth)
let md = do to_markdown("# Heading 1") | first(); |
md >> 1
# => ## Heading 1
```

## .. Operator

The range operator (`..`) creates sequences of consecutive values between a start and end point.

### Usage

The range operator generates arrays of values from a starting point to an ending point (inclusive). It works with both numeric values and characters.

### Examples

```mq
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
