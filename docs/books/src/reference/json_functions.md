# JSON Functions

The JSON module provides functions for parsing, processing, and converting JSON (JavaScript Object Notation) data.

## Including the JSON Module

To use the JSON functions, include the module at the top of your mq script:

```mq
include "json"
```

## Functions

### `json_parse(input)`

Parses a JSON string and returns the parsed data structure.

**Parameters:**
- `input`: String containing the JSON data

**Returns:**
- Parsed data structure (dict, array, or scalar value depending on the JSON content)

**Example:**
```mq
include "json"

# Parse JSON object
| "{\"name\": \"John\", \"age\": 30, \"city\": \"New York\"}" | json_parse()
# Returns: {"name": "John", "age": 30, "city": "New York"}

# Parse JSON array
| "[\"item1\", \"item2\", \"item3\"]" | json_parse()
# Returns: ["item1", "item2", "item3"]

# Parse complex JSON
| "{\"users\": [{\"name\": \"John\", \"age\": 30}, {\"name\": \"Jane\", \"age\": 25}]}" | json_parse()
# Returns: {"users": [{"name": "John", "age": 30}, {"name": "Jane", "age": 25}]}

# Parse JSON with boolean and null values
| "{\"active\": true, \"score\": null, \"verified\": false}" | json_parse()
# Returns: {"active": true, "score": null, "verified": false}
```

### `json_stringify(data)`

Converts a data structure to a JSON string representation.

**Parameters:**
- `data`: Data structure to convert (dict, array, or scalar value)

**Returns:**
- String containing the formatted JSON data

**Example:**
```mq
include "json"

# Convert dict to JSON
| {"name": "John", "age": 30, "city": "New York"} | json_stringify()
# Returns: "{\"name\": \"John\", \"age\": 30, \"city\": \"New York\"}"

# Convert array to JSON
| ["item1", "item2", "item3"] | json_stringify()
# Returns: "[\"item1\", \"item2\", \"item3\"]"

# Convert nested structure to JSON
| {"users": [{"name": "John", "age": 30}, {"name": "Jane", "age": 25}]} | json_stringify()
# Returns: "{\"users\": [{\"name\": \"John\", \"age\": 30}, {\"name\": \"Jane\", \"age\": 25}]}"

# Convert scalar values
| "Hello World" | json_stringify()
# Returns: "\"Hello World\""

| 42 | json_stringify()
# Returns: "42"

| true | json_stringify()
# Returns: "true"

| None | json_stringify()
# Returns: "null"
```

### `json_to_markdown_table(data)`

Converts a JSON data structure to a Markdown table format.

**Parameters:**
- `data`: JSON data structure to convert (dict, array, or scalar value)

**Returns:**
- String containing the Markdown table

**Example:**
```mq
include "json"

# Convert dict to Markdown table
| {"name": "John", "age": 30, "city": "New York"} | json_to_markdown_table()
# Returns:
# | Key | Value |
# | --- | --- |
# | name | John |
# | age | 30 |
# | city | New York |

# Convert array of dicts to Markdown table
| [{"name": "John", "age": 30}, {"name": "Jane", "age": 25}] | json_to_markdown_table()
# Returns:
# | name | age |
# | --- | --- |
# | John | 30 |
# | Jane | 25 |

# Convert scalar value to Markdown table
| "Hello World" | json_to_markdown_table()
# Returns:
# | Value |
# | --- |
# | Hello World |
```


## Type Conversion

The JSON parser automatically converts values to appropriate mq types:

- **Strings**: JSON strings become mq strings
- **Numbers**: JSON numbers become mq numbers (integers or floats)
- **Booleans**: JSON `true`/`false` become mq booleans
- **Null**: JSON `null` becomes mq `None`
- **Arrays**: JSON arrays become mq arrays
- **Objects**: JSON objects become mq dictionaries
