# Pattern Matching

The match expression enables pattern matching on values, providing a powerful way to destructure and handle different data types.

## Basic Syntax

```ruby
match (value):
  | pattern: body
end
```

The match expression evaluates the value and compares it against a series of patterns. The first matching pattern's body is executed.

## Literal Patterns

Match against specific values:

```ruby
match (x):
  | 1: "one"
  | 2: "two"
  | _: "other"
end
```

Literal patterns support:
- Numbers: `1`, `2.5`, `-10`
- Strings: `"hello"`, `"world"`
- Booleans: `true`, `false`
- None: `none`

## Type Patterns

Match based on value type using the `:type_name` syntax:

```ruby
match (value):
  | :string: "is string"
  | :number: "is number"
  | :array: "is array"
  | :dict: "is dict"
  | :bool: "is boolean"
  | :none: "is none"
  | _: "other type"
end
```

Available type patterns:
- `:string` - Matches string values
- `:number` - Matches numeric values
- `:array` - Matches array values
- `:dict` - Matches dictionary values
- `:bool` - Matches boolean values
- `:markdown` - Matches markdown values
- `:none` - Matches none value

## Array Patterns

Destructure arrays and bind elements to variables:

```ruby
match (arr):
  | []: "empty array"
  | [x]: x
  | [x, y]: add(x, y)
  | [first, second, third]: first
  | [first, ..rest]: first
end
```

Array patter features:
- Match exact length: `[x, y]` matches arrays with exactly 2 elements
- Rest pattern: `..rest` captures remaining elements
- Empty array: `[]` matches empty arrays
- Variable binding: Elements are bound to named variables

### Rest Pattern Example

```ruby
match (arr):
  | [head, ..tail]: tail
end
# array(1, 2, 3, 4) => array(2, 3, 4)
```

## Dict Patterns

Destructure dictionaries and extract values:

```ruby
match (obj):
  | {name, age}: name
  | {x, y}: add(x, y)
  | {}: "empty dict"
  | _: "no match"
end
```

Dict pattern features:
- Extract specific keys: `{name, age}` binds values to variables
- Partial matching: Matches dicts that have at least the specified keys
- Empty dict: `{}` matches empty dictionaries

### Example with Object

```ruby
let person = {"name": "Alice", "age": 30, "city": "Tokyo"} |
match (person):
  | {name, age}: s"${name} is ${age} years old"
  | {name}: name
  | _: "unknown"
end
# => "Alice is 30 years old"
```

## Variable Binding

Bind the matched value to a variable:

```ruby
match (value):
  | x: x + 1
end
```

Variable binding captures the entire value and makes it available in the body expression.

## Wildcard Pattern

The underscore `_` matches any value:

```ruby
match (x):
  | 1: "one"
  | 2: "two"
  | _: "something else"
end
```

Use the wildcard pattern as the last arm to handle all remaining cases.

## Guards

Add conditions to patterns using `if`:

```ruby
match (n):
  | x if (x > 0): "positive"
  | x if (x < 0): "negative"
  | _: "zero"
end
```

Guards allow you to:
- Add complex conditions to patterns
- Filter matched values
- Combine pattern matching with boolean logic

### Guard Examples

```ruby
# Match even numbers
match (n):
  | x if (x % 2 == 0): "even"
  | _: "odd"
end

# Match array with positive numbers
match (arr):
  | [x, ..] if (x > 0): "starts with positive"
  | _: "other"
end
```

## Multiple Arms

Combine multiple patterns for comprehensive matching:

```ruby
match (value):
  | 0: "zero"
  | x if (x > 0): "positive"
  | x if (x < 0): "negative"
  | :string: "text"
  | []: "empty array"
  | [x, ..rest]: "non-empty array"
  | {}: "empty dict"
  | _: "something else"
end
```

## Pattern Matching vs If Expressions

Pattern matching provides several advantages over if expressions:

### Using If Expressions

```ruby
if (type_of(x) == "number"):
  if (x == 0):
    "positive number"
  elif (x < 0):
    "negative number"
  else:
    "zero"
elif (type_of(x) == "array"):
  if (len(x) == 0):
    "empty array"
  else:
    "non-empty array"
else:
  "other"
```

### Using Pattern Matching

```ruby
match (x):
  | n if (n > 0): "positive number"
  | n if (n < 0): "negative number"
  | 0: "zero"
  | []: "empty array"
  | [_, ..rest]: "non-empty array"
  | _: "other"
end
```

## Practical Examples

### Processing Different Data Types

```ruby
def describe(value):
  match (value):
    | :none: "nothing"
    | :bool: "true or false"
    | x if (gt(x, 100)): "big number"
    | :number: "small number"
    | "": "empty string"
    | :string: "text"
    | []: "empty list"
    | [x]: s"list with one item: ${x}"
    | [_, ..rest]: "list with multiple items"
    | {}: "empty object"
    | _: "dictionary"
  end
```

### Extracting Data from Structures

```ruby
def get_first_name(user):
  match (user):
    | {name}: name
    | _: "unknown"
  end
```

### Handling API Responses

```ruby
def handle_response(response):
  match (response):
    | {status, data} if (eq(status, 200)): data
    | {status, error} if (eq(status, 404)): s"Not found: ${error}"
    | {status, error} if (eq(status, 500)): s"Server error: ${error}"
    | _: "Unknown response"
  end
```
