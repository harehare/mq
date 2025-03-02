# Types and Values

## Values

- `42` (a number)
- `"Hello, world!"` (a string)
- `[1, 2, 3]` (an array)
- `{ "name": "Alice", "age": 30 }` (an object)

## Types

| Type         | Description                                                                                                       | Examples                                   |
| ------------ | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| **Number**   | Represents numeric values.                                                                                        | `1`, `3.14`, `-42`                         |
| **String**   | Represents sequences of characters, including Unicode code points and escape sequences in the form of `\{0x000}`. | `"hello"`, `"123"`, `"😊"`, `"\u{1F600}"`  |
| **Boolean**  | Represents truth values.                                                                                          | `true`, `false`                            |
| **Array**    | Represents ordered collections of values.                                                                         | `array(1, 2, 3)`                           |
| **Function** | Represents executable code.                                                                                       | `def foo(): 42; let name = def foo(): 42;` |

## Environment Variables

A module handling environment-specific functionality.

- `__FILE__`: Contains the path to the file currently being processed.
