# Assignment Operators

Assignment operators are used to assign values to variables and combine assignment with arithmetic or logical operations.

## Simple Assignment

The basic assignment operator (`=`) assigns a value to a variable.

### Usage

```js
let x = 10 |
let name = "mq" |
let items = [1, 2, 3]
```

## Update Operator (`|=`)

The update operator (`|=`) applies an expression to a selected value and updates it in place.

### Usage

```js
<selector.value> |= expr
```

The left side specifies what to update using a selector, and the right side is the expression that transforms the value.

### Examples

```js
# Update a code block
.code.value |= "test" | to_text()
# => test

# Update a header level
.h.depth |= 3 | .h.depth
# => 3
```

## Compound Assignment Operators

Compound assignment operators combine an arithmetic or logical operation with assignment, providing a shorthand for updating variables.

### Addition Assignment (`+=`)

Adds a value to a variable and assigns the result back to the variable.

```js
var x = 10 |
x += 5
# => x is now 15

var count = 0 |
count += 1
# => count is now 1
```

### Subtraction Assignment (`-=`)

Subtracts a value from a variable and assigns the result back to the variable.

```js
var x = 10 |
x -= 3
# => x is now 7

var balance = 100 |
balance -= 25
# => balance is now 75
```

### Multiplication Assignment (`*=`)

Multiplies a variable by a value and assigns the result back to the variable.

```js
var x = 5 |
x *= 3
# => x is now 15

var price = 100 |
price *= 1.1
# => price is now 110
```

### Division Assignment (`/=`)

Divides a variable by a value and assigns the result back to the variable.

```js
var x = 20 |
x /= 4
# => x is now 5

var total = 100 |
total /= 2
# => total is now 50
```

### Modulo Assignment (`%=`)

Computes the remainder of dividing a variable by a value and assigns the result back to the variable.

```js
var x = 17 |
x %= 5
# => x is now 2

var count = 23 |
count %= 10
# => count is now 3
```

### Floor Division Assignment (`//=`)

Divides a variable by a value, floors the result (rounds down to the nearest integer), and assigns it back to the variable.

```ruby
var x = 17 |
x //= 5
# => x is now 3

var count = 23 |
count //= 10
# => count is now 2
```
