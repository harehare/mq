# Include

Loads functions from an external file using the syntax `include "module_name"`.
The include directive searches for .mq files in the following locations:

- `$HOME/.mq` - User's home directory mq folder
- `$ORIGIN/../lib/mq` - Library directory relative to the source file
- `$ORIGIN/../lib` - Parent lib directory relative to the source file
- `$ORIGIN` - Current directory relative to the source file

```js
include "module_name"
```

### Examples

```python
# Include math functions from math.mq
include "math"
# Now we can use functions defined in math.mq
let result = add(2, 3)
```
