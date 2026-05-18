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
b"hello"            # 5-byte sequence [104, 101, 108, 108, 111]
b"\xf0\x9f\x99\x82" # 4-byte emoji encoded as raw bytes
b""                 # empty byte sequence
```

### Allowed characters

Only **ASCII** characters (code points 0–127) may appear unescaped inside a byte literal. Non-ASCII characters such as `é` or `😊` must be written using `\xNN` hex escapes:

```mq
# Correct — use \xNN for non-ASCII bytes
b"\xc3\xa9"    # UTF-8 encoding of 'é' (2 bytes: 0xc3, 0xa9)

# Wrong — non-ASCII characters are not accepted in b"..."
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
b"abc" | len          # 3  — byte length, not character count
b"abc" | type         # "bytes"
b"abc" == b"abc"      # true
b"abc" | is_empty     # false
b""    | is_empty     # true
```

## Accessing Values

### Array Index Access

Arrays can be accessed using square bracket notation with zero-based indexing:

```mq
let arr = [1, 2, 3, 4, 5]

arr[0]     # Returns 1 (first element)
arr[2]     # Returns 3 (third element)
arr[6]     # Returns None
```

You can also use the `get` function explicitly:

```mq
get(arr, 0)    # Same as arr[0]
arr | get(2)    # Same as arr[2]
```

### Array Slice Access

Arrays support slice notation to extract subarrays using the `arr[start:end]` syntax:

```mq
let arr = [1, 2, 3, 4, 5]

arr[1:4]    # Returns [2, 3, 4] (elements from index 1 to 3)
arr[0:3]    # Returns [1, 2, 3] (first three elements)
arr[2:5]    # Returns [3, 4, 5] (elements from index 2 to end)
```

Slice indices work as follows:
- `start`: The starting index (inclusive)
- `end`: The ending index (exclusive)
- Both indices are zero-based
- If `start` or `end` is out of bounds, it will be clamped to valid range

```mq
let arr = [1, 2, 3, 4, 5]

arr[0:2]    # Returns [1, 2]
arr[3:10]   # Returns [4, 5] (end index clamped to array length)
arr[2:2]    # Returns [] (empty slice when start equals end)
```

### Dictionary Key Access

Dictionaries can be accessed using square bracket notation with keys:

```mq
let d = {"name": "Alice", "age": 30, "city": "Tokyo"}

d["name"]   # Returns "Alice"
d["age"]    # Returns 30
d["city"]   # Returns "Tokyo"
```

You can also use the `get` function explicitly:

```mq
get(d, "name")   # Same as di["name"]
d | get("age")    # Same as d["age"]
```

### Dynamic Access

Both arrays and dictionaries support dynamic access using variables:

```mq
let arr = [10, 20, 30]
| let index = 1
| arr[index]     # Returns 20

let d = {"x": 100, "y": 200}
| let key = "x"
| d[key]      # Returns 100
```

## Environment Variables

A module handling environment-specific functionality.

- `__FILE__`: Contains the path to the file currently being processed.
- `__FILE_NAME__`: Contains the name of the file currently being processed (without the path).
- `__FILE_STEM__`: Contains the stem of the file currently being processed (filename without extension).

