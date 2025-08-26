# TOML Functions

The TOML module provides functions for parsing, processing, and converting TOML (Tom's Obvious Minimal Language) data.

## Including the TOML Module

To use the TOML functions, include the module at the top of your mq script:

```mq
include "toml"
```

## Functions

### `toml_parse(input)`

Parses TOML content and returns the parsed data structure.

**Parameters:**
- `input`: String containing the TOML data

**Returns:**
- Dictionary representing the parsed TOML structure

**Example:**
```mq
include "toml"

# Parse TOML configuration
| "[server]
name = \"example\"
port = 8080
enabled = true

[[database]]
host = \"localhost\"
port = 5432" | toml_parse()
# Returns: {"server": {"name": "example", "port": 8080, "enabled": true}, "database": [{"host": "localhost", "port": 5432}]}
```

### `toml_stringify(data)`

Converts a data structure to a TOML string representation.

**Parameters:**
- `data`: Dictionary or data structure to convert to TOML

**Returns:**
- String containing the TOML representation

**Example:**
```mq
include "toml"

# Convert data to TOML
| {"server": {"name": "example", "port": 8080}, "enabled": true} | toml_stringify()
# Returns: "[server]\nname = \"example\"\nport = 8080\nenabled = true"
```

### `toml_to_json(data)`

Converts TOML data to a JSON string representation.

**Parameters:**
- `data`: TOML data structure to convert

**Returns:**
- String containing the JSON representation

**Example:**
```mq
include "toml"

# Convert TOML data to JSON
| {"name": "example", "port": 8080, "enabled": true} | toml_to_json()
# Returns: {"name":"example","port":8080,"enabled":true}
```

### `toml_to_markdown_table(data)`

Converts TOML data to a Markdown table format.

**Parameters:**
- `data`: TOML data structure to convert

**Returns:**
- String containing the Markdown table

**Example:**
```mq
include "toml"

# Convert to Markdown table
| {"name": "example", "port": 8080, "enabled": true} | toml_to_markdown_table()
# Returns:
# | Key | Value |
# | --- | --- |
# | name | example |
# | port | 8080 |
# | enabled | true |
```

## TOML Format Support

The TOML parser follows TOML v1.0.0 specification and supports:

- Basic key/value pairs with various data types
- Nested tables and dotted keys
- Arrays and array of tables
- Inline tables
- Multiline strings (basic and literal)
- Numbers (integers, floats, infinity, NaN)
- Booleans
- Comments
- Quoted and bare keys
- Escape sequences in strings
- RFC 4648 Base64 encoding support
- Mixed quoted and unquoted fields

## Data Type Mapping

TOML data types are mapped to mq data types as follows:

- **String**: Mapped to mq strings with escape sequence support
- **Integer**: Mapped to mq numbers
- **Float**: Mapped to mq numbers (including special values like inf, -inf, nan)
- **Boolean**: Mapped to mq booleans (true/false)
- **Array**: Mapped to mq arrays
- **Table**: Mapped to mq dictionaries
- **Array of Tables**: Mapped to mq arrays containing dictionaries
