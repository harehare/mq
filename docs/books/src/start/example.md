# Example

This page demonstrates practical examples of mq queries for common Markdown processing tasks. Each example includes the query, explanation, and typical use cases.

## Basic Element Selection

### Select All Headings

Extract all headings from a markdown document:

```js
.h
```

**Input example**:

```markdown
# Main Title
## Section 1
### Subsection 1.1
## Section 2
```

**Output**: Returns all heading elements with their levels and text.

### Extract Specific Table Row

Extract the second row from a markdown table:

```js
.[1][]
```
**Input example**:

```markdown
| Name  | Age | City |
| ----- | --- | ---- |
| Alice | 30  | NYC  |
| Bob   | 25  | LA   |
```

**Output**: Returns `["Bob", "25", "LA"]`

### Extract Specific List

Extract the second list from the document:

```js
.[1]
```

## Code Block Operations

### Exclude Code Blocks

Filter out all code blocks from a document, keeping only prose content:

```js
select(!.code)
```

**Input example**:

```markdown
This is a paragraph.

```js
console.log("code");
```

Another paragraph.
```

**Output**: Returns only the paragraph elements, excluding the code block.

### Extract JavaScript Code Blocks

Select only code blocks with a specific language:

```js
select(.code.lang == "js")
```

**Input example**:

```markdown
````js
const x = 1;
````

````python
x = 1
````

````js
const y = 2;
````
```

**Output**: Returns only the two JavaScript code blocks.

### Extract Language Names

Get a list of all programming languages used in code blocks:

```js
.code.lang
```

**Example output**: `["js", "python", "rust", "bash"]`

## Link and MDX Operations

### Extract MDX Components

Select all MDX components (JSX-like elements in Markdown):

```python
select(is_mdx())
```

**Input example**:
```markdown
Regular paragraph.

<CustomComponent prop="value" />

Another paragraph.

<AnotherComponent>
  Content
</AnotherComponent>
```

**Output**: Returns only the MDX component elements.

### Extract URLs from Links

Get all URLs from markdown links:

```js
.link.url
```

**Input example**:
```markdown
Check out [mq](https://mqlang.org) and [GitHub](https://github.com).
```

**Example output**: `["https://mqlang.org", "https://github.com"]`

## Advanced Markdown Processing

### Generate Table of Contents

Create a hierarchical table of contents from headings:

```js
.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, level - 1)
```

**Input example**:

```markdown
# Introduction
## Getting Started
### Installation
## Usage
```

**Output**:

```markdown
- [Introduction](#introduction)
  - [Getting Started](#getting-started)
    - [Installation](#installation)
  - [Usage](#usage)
```

### Generate XML Sitemap

Create an XML sitemap from markdown files:

```scala
def sitemap(item, base_url):
    let path = replace(to_text(item), ".md", ".html")
    | let loc = base_url + path
    | s"<url>
  <loc>${loc}</loc>
  <priority>1.0</priority>
  </url>"
end
```

**Usage example**:

```bash
$ mq 'sitemap(__FILE__, "https://example.com")' docs/**/*.md
```

**Example output**:
```xml
<url>
  <loc>https://example.com/docs/intro.html</loc>
  <priority>1.0</priority>
</url>
```

## Custom Functions and Programming

### Define Custom Function

Create reusable functions for complex transformations:

```ruby
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | s"${first_char}${rest_str}";
  | join("")
end
| snake_to_camel("hello_world")
```

**Example input**: `"user_name"`
**Example output**: `"UserName"`

### Map Over Arrays

Transform each element in an array:

```js
map([1, 2, 3, 4, 5], fn(x): x + 1;)
```

**Example output**: `[2, 3, 4, 5, 6]`

### Filter Arrays

Select elements that meet a condition:

```js
filter([5, 15, 8, 20, 3], fn(x): x > 10;)
```

**Example output**: `[15, 20]`

### Fold Arrays

Combine array elements into a single value:

```js
fold([1, 2, 3, 4], 0, fn(acc, x): acc + x;)
```

**Example output**: `10`

## File Processing

### CSV to Markdown Table

Convert CSV data to a formatted markdown table:

```bash
$ mq 'include "csv" | csv_parse(true) | csv_to_markdown_table()' example.csv
```

**Use case**: Convert spreadsheet data to markdown format for documentation. The `csv_parse(true)` treats the first row as headers.

**Input example** (example.csv):

```csv
Name,Age,City
Alice,30,NYC
Bob,25,LA
```

**Example output**:

```markdown
| Name  | Age | City |
| ----- | --- | ---- |
| Alice | 30  | NYC  |
| Bob   | 25  | LA   |
```

### Merge Multiple Files

Combine multiple markdown files with file path separators:

```bash
$ mq -S 's"\n${__FILE__}\n"' 'identity()' docs/books/**/**.md
```

The `-S` flag adds a separator between files, and `__FILE__` is a special variable containing the current file path.

**Example output**:
```markdown
docs/intro.md

# Introduction
...

docs/usage.md

# Usage
...
```

### Process Files in Parallel

Process large numbers of files efficiently:

```bash
$ mq -P 5 '.h1' docs/**/*.md
```

## LLM Workflows

### Extract Context for LLM Prompts

Extract specific sections to create focused context for LLM inputs:

```js
select(.h || .code) | self[:10]
```

**Example**: Extract first 10 sections with headings or code for a code review prompt.

### Document Statistics

```bash
$ mq -A 'let headers = count_by(fn(x): x | select(.h);)
| let paragraphs = count_by(fn(x): x | select(.text);)
| let code_blocks = count_by(fn(x): x | select(.code);)
| let links = count_by(fn(x): x | select(.link);)
| s"Headers: ${headers}, Paragraphs: ${paragraphs}, Code: ${code_blocks}, Links: ${links}"'' docs/books/**/**.md
```
### Generate Documentation Index

```js
.h
| let level = .h.level
| let text = to_text(self)
| let indent = repeat("  ", level - 1)
| let anchor = downcase(replace(text, " ", "-"))
| if (!is_empty(text)): s"${indent}- [${text}](#${anchor})"
```
