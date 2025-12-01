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

The while loop repeatedly executes code while a condition becomes true:

```ruby
let x = 5 |
while (x > 0):
  let x = x - 1 | x
end
# => 0
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

Foreach loops are useful for:

- Processing arrays element by element
- Mapping operations across collections
- Filtering and transforming data
