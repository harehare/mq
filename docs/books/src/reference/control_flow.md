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

```js
 if (eq(x, 1)):
   do "one" | upcase();
 elif (eq(x, 2)):
   do "TWO" | downcase();
 else:
   do
    "other" | upcase()
   end
```

```js
 if (eq(x, 1)):
   "one"
```

The if expression can be nested and chained with elif and else clauses.
The conditions must evaluate to boolean values.

## While Expression

The while loop repeatedly executes code while a condition is true:

```ruby
let x = 5 |
while (x > 0):
  let x = x - 1 | x
end
# => 0
```

You can use `break: <expr>` to return a value from a while loop:

```ruby
let x = 10 |
while (x > 0):
  let x = x - 1 |
  if(eq(x, 3)):
    break: "Found three!"
  else:
    x
end
# => "Found three!"
```

## Foreach Expression

The foreach loop iterates over elements in an array:

```ruby
let items = array(1, 2, 3) |
foreach (x, items):
   sub(x, 1)
end
# => array(0, 1, 2)
```

You can use `break: <expr>` to exit early and return a specific value instead of an array:

```ruby
let items = array(1, 2, 3, 4, 5) |
foreach (x, items):
  if(x > 3):
    break: "Found value greater than 3"
  else:
    x
end
# => "Found value greater than 3"
```

This is useful for implementing search operations:

```ruby
let items = array("apple", "banana", "cherry") |
foreach (item, items):
  if(contains(item, "ban")):
    break: item
  else:
    none
end
# => "banana"
```

Foreach loops are useful for:

- Processing arrays element by element
- Mapping operations across collections
- Filtering and transforming data
- Searching for specific elements with early exit

## Loop Expression

The loop expression creates an infinite loop that continues until explicitly terminated with `break`:

```ruby
var x = 0 |
loop:
  x = x + 1 |
  if(x > 5):
    break
  else:
    x
end
# => 5
```

The loop can be controlled using `break` to exit the loop and `continue` to skip to the next iteration:

```ruby
var x = 0 |
loop:
  x = x + 1 |
  if(x < 3):
    continue
  elif(x > 5):
    break
  else:
    x
end
# => 5
```

### Break with Value

The `break` statement can return a value from a loop using the `break: <expr>` syntax. This allows loops to be used as expressions that produce a specific value when exited:

```ruby
var x = 0 |
loop:
  x = x + 1 |
  if(x > 5):
    break: "Found it!"
  else:
    x
end
# => "Found it!"
```

You can return any type of value:

```ruby
var x = 0 |
loop:
  x = x + 1 |
  if(x > 5):
    break: array(x, "iterations")
  else:
    x
end
# => array(6, "iterations")
```

This feature works in all loop types (loop, while, foreach) and is useful for:

- Returning computed results from loops
- Implementing search patterns that return the found value
- Creating loops that act as expressions with meaningful return values

Loop expressions are useful for:

- Implementing infinite loops with conditional exits
- Creating retry mechanisms
- Processing until a specific condition is met
- Complex iteration patterns that don't fit while or foreach
