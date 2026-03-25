# Example

This page demonstrates practical examples of mq queries for common Markdown processing tasks. Each example includes the query, explanation, and typical use cases.

## Basic Element Selection

### Select All Headings

Extract all headings from a markdown document:

```mq
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

```mq
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

```mq
.[1]
```

## Code Block Operations

### Exclude Code Blocks

Filter out all code blocks from a document, keeping only prose content:

```mq
select(!.code)
```

**Input example**:

````markdown
This is a paragraph.

```js
console.log("code");
```

Another paragraph.
````

**Output**: Returns only the paragraph elements, excluding the code block.

### Extract JavaScript Code Blocks

Select only code blocks with a specific language:

```mq
select(.code.lang == "js")
```

**Input example**:

````markdown
```js
const x = 1;
```

```python
x = 1
```

```js
const y = 2;
```
````

**Output**: Returns only the two JavaScript code blocks.

### Extract Language Names

Get a list of all programming languages used in code blocks:

```mq
.code.lang
```

**Example output**: `["js", "python", "rust", "bash"]`

## Link and MDX Operations

### Extract MDX Components

Select all MDX components (JSX-like elements in Markdown):

```mq
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

```mq
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

```mq
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

```mq
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

## Section Operations

The section module provides functions for splitting and filtering Markdown documents by section. There are three ways to use it:

| Style | Syntax | Notes |
| ----- | ------ | ----- |
| `import` | `import "section"` then `section::fn()` | Namespaced — recommended |
| `include` | `include "section"` then `fn()` | No namespace prefix |
| `-A` flag | `mq -A 'section::fn()'` | Aggregate mode: processes all nodes at once |

> **Note**: Section functions need all document nodes at once. Use `-A` on the command line, or `nodes` in inline queries.

### Extract Sections by Title

**`-A` flag** (command line):

```bash
$ mq -A 'section::section("Installation")' README.md
```

**`import` + `nodes`** (inline query or script):

```mq
import "section"
| nodes
| section::section("Installation")
```

**`include`** (no namespace prefix):

```mq
include "section"
| nodes
| section("Installation")
```

Section objects are automatically expanded to Markdown nodes in CLI output, so `collect()` is not needed.

> **Note (code usage)**: When using the section module from Rust or other code (not the CLI), section objects are plain dicts and must be explicitly converted with `section::collect()`:
>
> ```mq
> import "section"
> | nodes
> | section::section("Installation")
> | section::collect()
> ```

**Input example**:

```markdown
# Introduction

Welcome to the project.

## Installation

Run the following command.

## Usage

Use the tool like this.
```

**Output**:

```markdown
## Installation

Run the following command.
```

### Extract Body Only

Use `bodies()` to get section content without the header:

```bash
$ mq -A 'section::section("Installation") | section::bodies() | first()' README.md
```

**Output**: Returns only the body nodes of the "Installation" section, without the `##` header.

### Filter by Heading Level

Use `by_level()` to filter sections by heading level. Accepts a number or a range:

```bash
# h2 sections only
$ mq -A 'section::sections() | section::by_level(2)' README.md

# h1 and h2 sections (1..2 includes both)
$ mq -A 'section::sections() | section::by_level(1..2)' README.md
```

**Input example**:

```markdown
# Chapter 1

Intro.

## Section 1.1

Detail.

# Chapter 2

Content.
```

`by_level(1)` **output**:

```markdown
# Chapter 1

Intro.

# Chapter 2

Content.
```

### Split Document by Header Level

Split a document into sections at a specific heading level and flatten back to Markdown:

```bash
$ mq -A 'section::split(2) | section::collect()' README.md
```

Or with `nodes`:

```mq
import "section"
| nodes
| section::split(2)
| section::collect()
```

### Generate Table of Contents from Sections

```bash
$ mq -A 'section::sections() | section::toc()' README.md
```

**Input example**:

```markdown
# Introduction

## Getting Started

### Prerequisites

## Advanced Usage
```

**Output**: `["- Introduction", "  - Getting Started", "    - Prerequisites", "  - Advanced Usage"]`

### Filter Sections with Content

Filter sections that have content beyond the header:

```bash
$ mq -A 'section::sections() | filter(fn(s): section::has_content(s);) | section::titles()' README.md
```

**Input example**:

```markdown
# Introduction

Welcome to the project.

## Empty Section

## Usage

Use the tool like this.
```

**Output**: `["Introduction", "Usage"]`

## Table Operations

The table module provides functions for extracting and transforming Markdown tables.

> **Note**: Table functions need all document nodes at once. Use `-A` on the command line, or `nodes` in inline queries. Unlike the section module, `import "table"` must be written explicitly.

### Extract Tables

**`-A` flag** (command line):

```bash
$ mq -A 'import "table" | table::tables()' README.md
```

**`import` + `nodes`** (inline query or script):

```mq
import "table"
| nodes
| table::tables()
```

Table objects are automatically expanded to Markdown nodes in CLI output, so `to_markdown()` is not needed.

> **Note (code usage)**: When using the table module from Rust or other code (not the CLI), table objects are plain dicts and must be explicitly converted with `table::to_markdown()`:
>
> ```mq
> import "table"
> | nodes
> | table::tables()
> | table::to_markdown()
> ```

**Input example**:

```markdown
| Name  | Age |
| ----- | --- |
| Alice | 30  |
| Bob   | 25  |
```

**Output**:

```markdown
| Name  | Age |
| ----- | --- |
| Alice | 30  |
| Bob   | 25  |
```

### Add a Row to a Table

```bash
$ mq -A 'import "table" | table::tables() | first() | table::add_row(["Charlie", "35"])' README.md
```

**Input example**:

```markdown
| Name  | Age |
| ----- | --- |
| Alice | 30  |
```

**Output**:

```markdown
| Name    | Age |
| ------- | --- |
| Alice   | 30  |
| Charlie | 35  |
```

### Convert Table to CSV

```bash
$ mq -A 'import "table" | table::tables() | first() | table::to_csv()' README.md
```

**Output**: Returns the table as a CSV string.

## Custom Functions and Programming

### Define Custom Function

Create reusable functions for complex transformations:

```mq
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

```mq
map([1, 2, 3, 4, 5], fn(x): x + 1;)
```

**Example output**: `[2, 3, 4, 5, 6]`

### Filter Arrays

Select elements that meet a condition:

```mq
filter([5, 15, 8, 20, 3], fn(x): x > 10;)
```

**Example output**: `[15, 20]`

### Fold Arrays

Combine array elements into a single value:

```mq
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

```mq
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

```mq
.h
| let level = .h.level
| let text = to_text(self)
| let indent = repeat("  ", level - 1)
| let anchor = downcase(replace(text, " ", "-"))
| if (!is_empty(text)): s"${indent}- [${text}](#${anchor})"
```

## Frontmatter Operations

Extract frontmatter metadata from markdown files:

```mq
import "yaml" | if (.yaml): yaml::yaml_parse() | get(:title)
```
