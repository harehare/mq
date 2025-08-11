# YAML Functions

The YAML module provides functions for parsing, processing, and converting YAML (YAML Ain't Markup Language) data.

## Including the YAML Module

To use the YAML functions, include the module at the top of your mq script:

```mq
include "yaml"
```

## Functions

### `yaml_parse(input)`

Parses a YAML string and returns the parsed data structure.

**Parameters:**
- `input`: String containing the YAML data

**Returns:**
- Parsed data structure (dict, array, or scalar value depending on the YAML content)

**Example:**
```mq
include "yaml"

# Parse YAML object
| "name: John\nage: 30\ncity: New York" | yaml_parse()
# Returns: {"name": "John", "age": 30, "city": "New York"}

# Parse YAML array
| "- item1\n- item2\n- item3" | yaml_parse()
# Returns: ["item1", "item2", "item3"]

# Parse complex YAML
| "users:\n  - name: John\n    age: 30\n  - name: Jane\n    age: 25" | yaml_parse()
# Returns: {"users": [{"name": "John", "age": 30}, {"name": "Jane", "age": 25}]}
```

### `yaml_stringify(data)`

Converts a data structure to a YAML string representation.

**Parameters:**
- `data`: Data structure to convert (dict, array, or scalar value)

**Returns:**
- String containing the formatted YAML data

**Example:**
```mq
include "yaml"

# Convert dict to YAML
| {"name": "John", "age": 30, "city": "New York"} | yaml_stringify()
# Returns: "name: John\nage: 30\ncity: New York"

# Convert array to YAML
| ["item1", "item2", "item3"] | yaml_stringify()
# Returns: "- item1\n- item2\n- item3"

# Convert nested structure to YAML
| {"users": [{"name": "John", "age": 30}, {"name": "Jane", "age": 25}]} | yaml_stringify()
# Returns: "users:\n  - name: John\n    age: 30\n  - name: Jane\n    age: 25"
```

### `yaml_keys(data, prefix)`

Returns all keys in a YAML data structure, including nested keys, with dot notation.

**Parameters:**
- `data`: YAML data structure (dict or array)
- `prefix`: String prefix to add to the keys (use empty string for no prefix)

**Returns:**
- Array of strings containing all keys with dot notation for nested structures

**Example:**
```mq
include "yaml"

# Get keys from nested YAML structure
| let data = {"user": {"name": "John", "profile": {"age": 30, "city": "New York"}}}
| data | yaml_keys("")
# Returns: ["user", "user.name", "user.profile", "user.profile.age", "user.profile.city"]

# Get keys with prefix
| data | yaml_keys("root")
# Returns: ["root.user", "root.user.name", "root.user.profile", "root.user.profile.age", "root.user.profile.city"]
```

### `yaml_to_json(data)`

Converts a YAML data structure to a JSON string representation.

**Parameters:**
- `data`: YAML data structure to convert

**Returns:**
- String containing the JSON representation

**Example:**
```mq
include "yaml"

# Convert YAML data to JSON
| {"name": "John", "age": 30, "active": true} | yaml_to_json()
# Returns: "{\"name\": \"John\", \"age\": 30, \"active\": true}"

# Convert array to JSON
| ["item1", "item2", "item3"] | yaml_to_json()
# Returns: "[\"item1\", \"item2\", \"item3\"]"
```

### `yaml_to_markdown_table(data)`

Converts a YAML data structure to a Markdown table format.

**Parameters:**
- `data`: YAML data structure to convert (dict, array, or scalar value)

**Returns:**
- String containing the Markdown table

**Example:**
```mq
include "yaml"

# Convert dict to Markdown table
| {"name": "John", "age": 30, "city": "New York"} | yaml_to_markdown_table()
# Returns:
# | Key | Value |
# | --- | --- |
# | name | John |
# | age | 30 |
# | city | New York |

# Convert array of dicts to Markdown table
| [{"name": "John", "age": 30}, {"name": "Jane", "age": 25}] | yaml_to_markdown_table()
# Returns:
# | name | age |
# | --- | --- |
# | John | 30 |
# | Jane | 25 |
```

## YAML Format Support

The YAML parser supports YAML 1.2 specifications and handles:

- Scalars: strings, numbers, booleans, and null values
- Collections: sequences (arrays) and mappings (dictionaries)
- Nested structures with proper indentation
- Quoted and unquoted strings
- Multi-line strings with literal block scalars (`|`)
- Boolean values: `true`, `false`, `yes`, `no` (case-insensitive)
- Null values: `null`, `~`, or empty values
- Comments (lines starting with `#`)
- Proper handling of special characters and escape sequences

## Type Conversion

The YAML parser automatically converts values to appropriate types:

- **Strings**: Quoted or unquoted text
- **Numbers**: Integers and floating-point numbers
- **Booleans**: `true`/`false`, `yes`/`no`, `True`/`False`, etc.
- **Null**: `null`, `~`, or empty values become `None`
- **Arrays**: YAML sequences become mq arrays
- **Objects**: YAML mappings become mq dictionaries
