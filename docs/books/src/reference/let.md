# Let Expression

The let expression binds a value to an identifier for later use:

```js
# Binds 42 to x
let x = 42
# Uses x in an expression
let y = x + 1
# Binds `add` function to z
let z = do let z = fn(x): x + 1; | z(1);
```
