# Section Functions

The Section module provides functions for splitting, filtering, and manipulating Markdown documents by section. Sections are defined by headers (H1-H6) and include all content until the next header of the same level.

## Including the Section Module

To use the Section functions, include the module at the top of your mq script:

```mq
import "section"
```

## Functions

### `sections(md_nodes)`

Splits Markdown nodes into sections at each heading node.  
Each section starts with a heading node and includes all subsequent nodes up to the next heading of the same or higher level.

**Parameters:**
- `md_nodes`: Array of Markdown nodes to split

**Returns:**
- Array of section objects, where each section has:
  - `type`: Always `:section`
  - `header`: The heading node that starts the section
  - `children`: Array of all nodes in the section (including the header)

**Example:**
```mq
import "section"

nodes | section::sections()
```

### `split(md_nodes, level)`

Splits markdown nodes into sections based on header level. Each section includes a header and all content until the next header of the same level.

**Parameters:**
- `md_nodes`: Array of markdown nodes to split
- `level`: Header level (1-6) to split on

**Returns:**
- Array of section objects, where each section has:
  - `type`: Always `:section`
  - `header`: The header node
  - `children`: Array of all nodes in the section (including the header)

**Example:**

```mq
import "section"

nodes | section::split(2)
```

### `title_contains(sections, text)`

Filters sections by checking if the title contains the specified text.

**Parameters:**

- `sections`: Array of section objects
- `text`: String to search for in section titles

**Returns:**

- Array of sections whose titles contain the specified text

**Example:**

```mq
import "section"

# Find sections with "API" in the title
| nodes | section::split(2) | section::title_contains("API")
```

### `title_match(sections, pattern)`

Filters sections by matching the title against a regular expression pattern.

**Parameters:**

- `sections`: Array of section objects
- `pattern`: Regular expression pattern to match against section titles

**Returns:**

- Array of sections whose titles match the pattern

**Example:**

```mq
import "section"

# Find sections starting with "Chapter"
| nodes | section:::split(1) | section::title_match("^Chapter")
```

### `title(section)`

Extracts the title text from a section (header text without the # symbols).

**Parameters:**

- `section`: A section object

**Returns:**

- String containing the section title, or empty string if not a valid section

**Example:**

```mq
import "section"

# Get title of first H2 section
| nodes | section::split(2) | section::nth(0) | section::title()
```

### `content(section)`

Returns the content of a section, excluding the header.

**Parameters:**

- `section`: A section object

**Returns:**

- Array of markdown nodes (all nodes except the header)

**Example:**

```mq
import "section"

# Get content of the first section
| nodes | section::split(2) | section::nth(0) | section::content()
```

### `all_nodes(section)`

Returns all nodes of a section, including both the header and content.

**Parameters:**

- `section`: A section object

**Returns:**

- Array of all markdown nodes in the section

**Example:**

```mq
import "section"

# Get all nodes including header
| nodes | section::split(2) | section::nth(0) | section::all_nodes()
```

### `level(section)`

Returns the header level of a section.

**Parameters:**

- `section`: A section object

**Returns:**

- Integer from 1-6 representing the header level, or 0 if not a valid section

**Example:**

```mq
import "section"

# Get the level of each section
| nodes | section::split(2) | map(section::level)
```

### `nth(sections, n)`

Returns the nth section from an array of sections (0-indexed).

**Parameters:**

- `sections`: Array of section objects
- `n`: Index of the section to retrieve (0-based)

**Returns:**

- The section at index `n`, or `None` if index is out of bounds

**Example:**

```mq
import "section"

# Get the first section
| nodes | section::split(2) | section::nth(0)
```

### `titles(sections)`

Extracts titles from all sections in an array.

**Parameters:**

- `sections`: Array of section objects

**Returns:**

- Array of title strings

**Example:**

```mq
import "section"

# Get all H2 titles
| nodes | section::split(2) | section::titles()
```

### `toc(sections)`

Generates a table of contents from sections with proper indentation based on header level.

**Parameters:**

- `sections`: Array of section objects

**Returns:**

- Array of strings, each representing a TOC entry with appropriate indentation

**Example:**

```mq
import "section"

# Generate table of contents for all H2 sections
| nodes | section::split(2) | section::toc()
```

### `has_content(section)`

Checks if a section has any content beyond the header.

**Parameters:**

- `section`: A section object

**Returns:**

- Boolean: `true` if the section has content, `false` otherwise

**Example:**

```mq
import "section"

# Filter sections that have content
| nodes | section::split(2) | filter(section::has_content)
```

### `collect(sections)`

Converts section objects back to their original markdown node arrays. This is useful for outputting sections after manipulation.

**Parameters:**

- `sections`: Array of section objects

**Returns:**

- Array of markdown nodes with sections collected

**Example:**

```mq
import "section"

# Filter sections and convert back to markdown
| nodes | section::split(2) | section::title_contains("API") | section::collect()
```

## Usage Patterns

### Extracting Specific Sections

```mq
include "section"

# Get all content from "Installation" section
| h2() | split(2) | title_contains("Installation") | nth(0) | content()
```

### Filtering and Reorganizing Content

```mq
include "section"

# Get only API-related sections
| h2() | split(2) | title_match("^API") | collect()
```

### Building Navigation

```mq
include "section"

# Create a complete table of contents
| h1() + h2() + h3() | toc()
```

### Finding Empty Sections

```mq
include "section"

# List sections without content
| h2() | split(2) | filter(fn(s): !has_content(s);) | titles()
```

## Section Object Structure

Each section object returned by `split()` has the following structure:

```
{
  type: :section,
  header: <header_node>,
  children: [<content_nodes>...]
}
```

- `type`: Always the symbol `:section`
- `header`: The markdown header node
- `children`: Array of all nodes including the header and subsequent content
