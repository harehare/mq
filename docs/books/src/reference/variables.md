# Variable Declarations

## Let

The `let` binds an immutable value to an identifier for later use:

```mq
# Binds 42 to x
let x = 42
# Uses x in an expression
| let y = x + 1
# Binds `add` function to z
| let z = do let z = fn(x): x + 1; | z(1);
```

Once a variable is declared with `let`, its value cannot be changed.

## Var

The `var` declares a mutable variable that can be reassigned:

```mq
# Declares a mutable variable
var counter = 0
# Reassigns the value
counter = counter + 1
# counter is now 1
```

Variables declared with `var` can be modified using the assignment operator (`=`):

```mq
var total = 100
| total = total - 25
# total is now 75

var message = "Hello"
| message = message + " World"
# message is now "Hello World"
```

## Destructuring Assignment

Both `let` and `var` support destructuring patterns on the left-hand side.

### Array Destructuring

```mq
let [a, b] = [1, 2] | a
# => 1

let [head, ..tail] = [1, 2, 3] | tail
# => [2, 3]
```

### Dict Destructuring

```mq
let {name, age} = {"name": "Alice", "age": 30} | name
# => "Alice"
```

### Mutable Destructuring

Using `var` allows reassigning destructured variables:

```mq
var [a, b] = [1, 2] | a = 99 | a
# => 99
```

## Choosing Between Let and Var

- Use `let` when you want to create an immutable binding (most cases)
- Use `var` when you need to modify the value after declaration (counters, accumulators, etc.)

