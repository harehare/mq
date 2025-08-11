# CSV Functions

The CSV module provides functions for parsing, processing, and converting CSV (Comma-Separated Values) and TSV (Tab-Separated Values) data.

## Including the CSV Module

To use the CSV functions, include the module at the top of your mq script:

```mq
include "csv"
```

## Functions

### `csv_parse(input, has_header)`

Parses CSV content using a comma as the delimiter.

**Parameters:**
- `input`: String containing the CSV data
- `has_header`: Boolean indicating whether the first row contains headers

**Returns:**
- If `has_header` is `true`: Array of dictionaries where keys are column headers
- If `has_header` is `false`: Array of arrays containing raw row data

**Example:**
```mq
include "csv"

# Parse CSV with headers
| "name,age,city\nJohn,30,New York\nJane,25,Boston" | csv_parse(true)
# Returns: [{"name": "John", "age": "30", "city": "New York"}, {"name": "Jane", "age": "25", "city": "Boston"}]

# Parse CSV without headers
| "John,30,New York\nJane,25,Boston" | csv_parse(false)
# Returns: [["John", "30", "New York"], ["Jane", "25", "Boston"]]
```

### `csv_parse_with_delimiter(input, delimiter, has_header)`

Parses CSV content with a custom delimiter.

**Parameters:**
- `input`: String containing the CSV data
- `delimiter`: String specifying the field delimiter
- `has_header`: Boolean indicating whether the first row contains headers

**Returns:**
- If `has_header` is `true`: Array of dictionaries where keys are column headers
- If `has_header` is `false`: Array of arrays containing raw row data

**Example:**
```mq
include "csv"

# Parse semicolon-separated values
| "name;age;city\nJohn;30;New York\nJane;25;Boston" | csv_parse_with_delimiter(";", true)
# Returns: [{"name": "John", "age": "30", "city": "New York"}, {"name": "Jane", "age": "25", "city": "Boston"}]
```

### `tsv_parse(input, has_header)`

Parses TSV (Tab-Separated Values) content.

**Parameters:**
- `input`: String containing the TSV data
- `has_header`: Boolean indicating whether the first row contains headers

**Returns:**
- If `has_header` is `true`: Array of dictionaries where keys are column headers
- If `has_header` is `false`: Array of arrays containing raw row data

**Example:**
```mq
include "csv"

# Parse TSV with headers
| "name	age	city\nJohn	30	New York\nJane	25	Boston" | tsv_parse(true)
# Returns: [{"name": "John", "age": "30", "city": "New York"}, {"name": "Jane", "age": "25", "city": "Boston"}]
```

### `csv_stringify(data, delimiter)`

Converts data to a CSV string with a specified delimiter.

**Parameters:**
- `data`: Array of dictionaries or array of arrays to convert
- `delimiter`: String specifying the field delimiter

**Returns:**
- String containing the formatted CSV data

**Example:**
```mq
include "csv"

# Convert array of dictionaries to CSV
| [{"name": "John", "age": "30"}, {"name": "Jane", "age": "25"}] | csv_stringify(",")
# Returns: "name,age\nJohn,30\nJane,25"

# Convert array of arrays to CSV
| [["name", "age"], ["John", "30"], ["Jane", "25"]] | csv_stringify(",")
# Returns: "name,age\nJohn,30\nJane,25"
```

### `csv_to_markdown_table(data)`

Converts CSV data to a Markdown table format.

**Parameters:**
- `data`: Array of dictionaries or array of arrays

**Returns:**
- String containing the Markdown table

**Example:**
```mq
include "csv"

# Convert to Markdown table
| [{"name": "John", "age": "30"}, {"name": "Jane", "age": "25"}] | csv_to_markdown_table()
# Returns:
# | name | age |
# | --- | --- |
# | John | 30 |
# | Jane | 25 |
```

### `csv_to_json(data)`

Converts CSV data to a JSON string.

**Parameters:**
- `data`: Array of dictionaries or array of arrays

**Returns:**
- String containing the JSON representation

**Example:**
```mq
include "csv"

# Convert to JSON
| [{"name": "John", "age": "30"}, {"name": "Jane", "age": "25"}] | csv_to_json()
# Returns: [{"name":"John","age":"30"},{"name":"Jane","age":"25"}]
```

## CSV Format Support

The CSV parser follows RFC 4180 specifications and supports:

- Quoted fields with embedded commas, newlines, and quotes
- Escaped quotes within quoted fields (double quotes)
- Custom delimiters
- Header row processing
- Mixed quoted and unquoted fields
