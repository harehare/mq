# Types and Values

## Values

- `42` (a number)
- `"Hello, world!"` (a string)
- `b"abc"` (a bytes literal)
- `:value` (a symbol)
- `[1, 2, 3]`, `array(1, 2, 3)` (an array)
- `{"a": 1, "b": 2, "c": 3}`, `dict(["a", 1], ["b", 2], ["c", 3])` (a dictionary)
- `true`, `false` (a boolean)
- `None`

## Types

| Type         | Description                                                                                                       | Examples                                        |
| ------------ | ----------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| **Number**   | Represents numeric values.                                                                                        | `1`, `3.14`, `-42`                              |
| **String**   | Represents sequences of characters, including Unicode code points and escape sequences in the form of `\{0x000}`. | `"hello"`, `"123"`, `"😊"`, `"\u{1F600}"`        |
| **Bytes**    | Represents a raw byte sequence. Written with a `b` prefix. Only ASCII characters are allowed unescaped.          | `b"abc"`, `b"\xf0\x9f\x99\x82"`, `b""`         |
| **Symbol**   | Represents immutable, interned identifiers prefixed with `:`. Used for constant values and keys.                  | `:value`, `:success`, `:error`, `:ok`           |
| **Boolean**  | Represents truth values.                                                                                          | `true`, `false`                                 |
| **Array**    | Represents ordered collections of values.                                                                         | `[1, 2, 3]`, `array(1, 2, 3)`                   |
| **Dict**     | Represents key-value mappings (dictionaries).                                                                     | `{"a": 1, "b": 2}`, `dict(["a", 1], ["b", 2])`  |
| **Function** | Represents executable code.                                                                                       | `def foo(): 42; let name = def foo(): 42;`      |

## Byte String Literals

Byte string literals use the `b"..."` syntax and represent raw sequences of bytes (`u8` values).

```mq
b"hello"              # 5-byte sequence [104, 101, 108, 108, 111]
b"\xf0\x9f\x99\x82"   # 4-byte emoji encoded as raw bytes
b""                   # empty byte sequence
```

### Allowed characters

Only **ASCII** characters (code points 0–127) may appear unescaped inside a byte literal. Non-ASCII characters such as `é` or `😊` must be written using `\xNN` hex escapes:

```mq
# Correct: use \xNN for non-ASCII bytes, here the UTF-8 encoding of 'é' (0xc3, 0xa9)
b"\xc3\xa9" | len
# => 2

# Wrong: non-ASCII characters are not accepted in b"..."
# b"é"          ← syntax error; use \xNN escapes instead
```

### Supported escape sequences

| Escape | Byte value |
| ------ | ---------- |
| `\\`   | `0x5c` (backslash) |
| `\"`   | `0x22` (double quote) |
| `\n`   | `0x0a` (newline) |
| `\r`   | `0x0d` (carriage return) |
| `\t`   | `0x09` (tab) |
| `\0`   | `0x00` (null) |
| `\xNN` | Arbitrary byte (two hex digits) |

### Common operations

```mq
# Byte length, not character count
b"abc" | len
# => 3
b"abc" | type
# => "bytes"
b"abc" == b"abc"
# => true
b"abc" | is_empty
# => false
b"" | is_empty
# => true
```

## Accessing Values

### Array Index Access

Arrays can be accessed using square bracket notation with zero-based indexing:

```mq
let arr = [1, 2, 3, 4, 5] |
arr[0]
# => 1

# Third element
let arr = [1, 2, 3, 4, 5] |
arr[2]
# => 3

# Out of range
let arr = [1, 2, 3, 4, 5] |
arr[6] | is_none
# => true

# The index must be an integer
let arr = [1, 2, 3, 4, 5] |
arr[1.5] | is_none
# => true
```

Indexing a value that cannot be indexed, such as `1[0]`, is a runtime error. `None[0]` returns `None`.

You can also use the `get` function explicitly:

```mq
let arr = [1, 2, 3, 4, 5] |
get(arr, 0)    # Same as arr[0]
# => 1

let arr = [1, 2, 3, 4, 5] |
arr | get(2)   # Same as arr[2]
# => 3
```

### Index Access with `.[...]`

On arrays, strings and dicts, `.[expr]` reads the same as `self[expr]`: negative indices count from the end, and an out-of-range or non-integer index returns `None`.

```mq
[10, 20, 30] | .[-1]
# => 30
"abc" | .[1]
# => "b"
{"a": 1} | .["a"]
# => 1
[10, 20, 30] | .[1.5] | is_none
# => true
```

On Markdown values, `.[n]` selects the list item with index `n` instead (see [Selectors](selectors.md)).

### Array Slice Access

Arrays support slice notation to extract subarrays using the `arr[start:end]` syntax:

```mq
# Elements from index 1 to 3
let arr = [1, 2, 3, 4, 5] |
arr[1:4]
# => [2, 3, 4]

# First three elements
let arr = [1, 2, 3, 4, 5] |
arr[0:3]
# => [1, 2, 3]

# Elements from index 2 to the end
let arr = [1, 2, 3, 4, 5] |
arr[2:5]
# => [3, 4, 5]
```

Slice indices work as follows:
- `start`: The starting index (inclusive)
- `end`: The ending index (exclusive)
- Both indices are zero-based
- If `start` or `end` is out of bounds, it will be clamped to valid range

```mq
let arr = [1, 2, 3, 4, 5] |
arr[0:2]
# => [1, 2]

# The end index is clamped to the array length
let arr = [1, 2, 3, 4, 5] |
arr[3:10]
# => [4, 5]

# The slice is empty when start equals end
let arr = [1, 2, 3, 4, 5] |
arr[2:2] | is_empty
# => true
```

### Dictionary Key Access

Dictionaries can be accessed using square bracket notation with keys:

```mq
let d = {"name": "Alice", "age": 30, "city": "Tokyo"} |
d["name"]
# => "Alice"

let d = {"name": "Alice", "age": 30, "city": "Tokyo"} |
d["age"]
# => 30
```

You can also use the `get` function explicitly:

```mq
let d = {"name": "Alice", "age": 30, "city": "Tokyo"} |
get(d, "name")   # Same as d["name"]
# => "Alice"

let d = {"name": "Alice", "age": 30, "city": "Tokyo"} |
d | get("age")   # Same as d["age"]
# => 30
```

A dictionary literal can be indexed directly:

```mq
{"a": 1, "b": {"c": 2}}["a"]
# => 1
{"a": 1, "b": {"c": 2}}["b"]["c"]
# => 2
```

### String Index Access

Strings can be indexed by Unicode scalar value. Negative indices count from the end. An out-of-range or non-integer index returns `None`:

```mq
"abc"[0]
# => "a"
"abc"[-1]
# => "c"
"abc"[3] | is_none
# => true
"abc"[1:3]
# => "bc"
```

Combining characters and emoji sequences span several scalar values. Use `graphemes` to split by user-perceived character.

### Spread Operator

The `...` spread operator expands an array or dict inline inside an array or
dict literal.

```mq
let a = [1, 2, 3]
| let b = [4, 5, 6]
| [...a, ...b]
# => [1, 2, 3, 4, 5, 6]

let a = [1, 2, 3]
| [0, ...a, 99]
# => [0, 1, 2, 3, 99]
```

```mq
let base = {x: 1, y: 2}
| {...base, y: 99, z: 3}
# => {"x": 1, "y": 99, "z": 3}
```

When the same key appears more than once, later keys override earlier ones,
including keys coming from a spread. Spreading `None` contributes nothing;
spreading any other non-array (in `[...]`) or non-dict (in `{...}`) value is a
type error.

### Dynamic Access

Both arrays and dictionaries support dynamic access using variables:

```mq
let arr = [10, 20, 30]
| let index = 1
| arr[index]
# => 20

let d = {"x": 100, "y": 200}
| let key = "x"
| d[key]
# => 100
```

## Environment Variables

A module handling environment-specific functionality.

- `__FILE__`: Contains the path to the file currently being processed.
- `__FILE_NAME__`: Contains the name of the file currently being processed (without the path).
- `__FILE_STEM__`: Contains the stem of the file currently being processed (filename without extension).

