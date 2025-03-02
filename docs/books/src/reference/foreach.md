# Foreach Expression

The foreach loop iterates over elements in an array:

```js
let items = array(1, 2, 3);
foreach (x, items):
   # Do something
   sub(x, 1);
# => array(0, 1, 2)
```

Foreach loops are useful for:

- Processing arrays element by element
- Mapping operations across collections
- Filtering and transforming data
