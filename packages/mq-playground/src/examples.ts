import * as mq from "mq-web";

export type ExampleCategory = {
  name: string;
  examples: {
    name: string;
    code: string;
    markdown: string;
    isUpdate: boolean;
    format: mq.Options["inputFormat"];
  }[];
};

export const EXAMPLE_CATEGORIES: ExampleCategory[] = [
  {
    name: "Basic Element Selection",
    examples: [
      {
        name: "Hello World",
        code: `.code("js") | to_text`,
        markdown: `# Hello, World!

\`\`\`js
console.log("Hello, World!")
\`\`\`

\`\`\`python
print("Hello, World!")
\`\`\`

\`\`\`rust
println!("Hello, World!")
\`\`\`
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Extract heading",
        code: `.h`,
        markdown: `# Heading 1

## Heading 2

### Heading 3

Some text here.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Extract table",
        code: `.[1][]`,
        markdown: `# Product List

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Laptop  | Electronics | $1200 | 45 |
| Monitor | Electronics | $350 | 28 |
| Chair   | Furniture | $150 | 73 |
| Desk    | Furniture | $200 | 14 |
| Keyboard | Accessories | $80 | 35 |

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Mouse   | Accessories | $25 | 50 |
| Headphones | Electronics | $120 | 32 |
| Bookshelf | Furniture | $180 | 17 |
| USB Cable | Accessories | $12 | 89 |
| Coffee Maker | Appliances | $85 | 24 |
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Extract list",
        code: `.[] | select(.list.level == 1)`,
        markdown: `# Product List

- Electronics
  - Laptop: $1200
  - Monitor: $350
  - Headphones: $120
- Furniture
  - Chair: $150
  - Desk: $200
  - Bookshelf: $180
- Accessories
  - Keyboard: $80
  - Mouse: $25
  - USB Cable: $12
`,
        isUpdate: false,
        format: "markdown",
      },
    ],
  },
  {
    name: "Code Block Operations",
    examples: [
      {
        name: "Extract js code",
        code: `select(.code.lang == "js")`,
        markdown: `# Sample codes
\`\`\`js
console.log("Hello, World!");
\`\`\`

\`\`\`python
print("Hello, World!")
\`\`\`

\`\`\`js
console.log("Hello, World!");
\`\`\`
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Exclude code",
        code: `select(!.code)`,
        markdown: `# Sample codes
\`\`\`js
console.log("Hello, World!");
\`\`\`

Some text here.

\`\`\`python
print("Hello, World!")
\`\`\`

More text here.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Extract language name",
        code: `.code.lang`,
        markdown: `# Sample codes
\`\`\`js
console.log("Hello, World!");
\`\`\`

\`\`\`python
print("Hello, World!")
\`\`\`

\`\`\`rust
println!("Hello, World!");
\`\`\`
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Extract all languages",
        code: `nodes | pluck(.code.lang)`,
        markdown: `# Sample codes
\`\`\`js
console.log("Hello, World!");
\`\`\`

\`\`\`python
print("Hello, World!")
\`\`\`

\`\`\`rust
println!("Hello, World!");
\`\`\`
`,
        isUpdate: false,
        format: "markdown",
      },
    ],
  },
  {
    name: "Link and MDX Operations",
    examples: [
      {
        name: "Extract MDX",
        code: `select(is_mdx())`,
        markdown: `import {Chart} from './snowfall.js'
import { isDarkMode } from '../../../textusm/frontend/src/ts/utils';
export const year = 2023

# Last year's snowfall

In {year}, the snowfall was above average.

<Chart color="#fcb32c" year={year} />
`,
        isUpdate: false,
        format: "mdx",
      },
      {
        name: "Extract link URL",
        code: `.link.url`,
        markdown: `# Links

Here is a [link to GitHub](https://github.com).
Another [link to documentation](https://docs.example.com).
And a [relative link](./readme.md).
`,
        isUpdate: false,
        format: "markdown",
      },
    ],
  },
  {
    name: "Advanced Markdown Processing",
    examples: [
      {
        name: "Markdown TOC",
        code: `.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (not(is_none(level))): to_md_list(link, level)`,
        markdown: `# [header1](https://example.com)

- item 1
- item 2

## header2

- item 1
- item 2

### header3

- item 1
- item 2

#### header4

- item 1
- item 2`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Generate sitemap",
        code: `def sitemap(item, base_url):
  let path = replace(to_text(item), ".md", ".html")
  | let loc = base_url + path
  | s"<url>
  <loc>\${loc}</loc>
  <priority>1.0</priority>
</url>"
end
| .[]
| sitemap("https://example.com/")`,
        markdown: `# Summary

- [Chapter1](chapter1.md)
- [Chapter2](Chapter2.md)
  - [Chapter3](Chapter3.md)
- [Chapter4](Chapter4.md)
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Extract frontmatter data",
        code: `import "yaml" | if (.yaml): yaml::yaml_parse() | get(:title)`,
        markdown: `---
title: "Sample Document"
author: "John Doe"
date: "2024-01-01"
---

# Sample Document

This is a sample document with frontmatter.

`,
        isUpdate: false,
        format: "markdown",
      },
    ],
  },
  {
    name: "Section Operations",
    examples: [
      {
        name: "Extract section by title",
        code: `# With -A flag in CLI, import, nodes, and collect are handled automatically.
# e.g., mq -A 'section::section("Installation")' file.md
import "section" | nodes | section::section("Installation") | section::collect()`,
        markdown: `# Introduction

Welcome to the project.

## Installation

Run the following command:

\`\`\`bash
npm install mq
\`\`\`

## Usage

Use the tool like this.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Filter nodes within section",
        code: `# Extract only code blocks inside a specific section.
# e.g., mq -A 'section::section("Installation") | .code' file.md
import "section" | nodes | section::section("Installation") | .code | section::collect()`,
        markdown: `# Introduction

Welcome to the project.

## Installation

Run the following command:

\`\`\`bash
npm install mq
\`\`\`

You can also use yarn:

\`\`\`bash
yarn add mq
\`\`\`

## Usage

Use the tool like this.

\`\`\`bash
mq '.h1' file.md
\`\`\`
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Extract section with depth",
        code: `# With -A flag in CLI, import, nodes, and collect are handled automatically.
# e.g., mq -A 'section::section("API", true)' file.md
import "section" | nodes | section::section("API", true) | section::collect()`,
        markdown: `# Introduction

Some intro text.

# API

## Endpoints

\`GET /users\`

\`POST /users\`

## Authentication

Use Bearer tokens.

# Contributing

How to contribute.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Split by header level",
        code: `import "section" | nodes | section::split(2) | section::titles()`,
        markdown: `# Main Title

## Section 1

Content of section 1.

## Section 2

Content of section 2.

## Section 3

Content of section 3.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Filter by heading level",
        code: `import "section" | nodes | section::sections() | section::by_level(2) | section::titles()`,
        markdown: `# Chapter 1

## Section 1.1

Content here.

## Section 1.2

More content.

# Chapter 2

## Section 2.1

Another section.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Table of contents",
        code: `include "section" | nodes | sections() | toc()`,
        markdown: `# Introduction

Some text.

## Getting Started

More text.

### Prerequisites

Details here.

## Advanced Usage

Advanced content.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Filter sections with content",
        code: `include "section" | nodes | sections() | filter(fn(s): has_content(s);) | titles()`,
        markdown: `# Introduction

Welcome to the project.

## Empty Section

## Usage

Use the tool like this.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Map section titles",
        code: `include "section" | nodes | map_sections(fn(h, _): to_text(h) | upcase();)`,
        markdown: `# Introduction

Welcome to the project.

## Installation

Run \`npm install mq\`.

## Usage

Use the tool like this.
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Get nth section",
        code: `include "section" | nodes | sections() | nth(1) | body()`,
        markdown: `# Introduction

First section content.

## Installation

Run \`npm install mq\`.

## Usage

Use the tool like this.
`,
        isUpdate: false,
        format: "markdown",
      },
    ],
  },
  {
    name: "Table Module",
    examples: [
      {
        name: "Extract table structures",
        code: `include "table" | nodes | tables() | map(fn(t): t | to_markdown();) | flatten()`,
        markdown: `# Product List

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Laptop  | Electronics | $1200 | 45 |
| Monitor | Electronics | $350 | 28 |
| Chair   | Furniture | $150 | 73 |
| Desk    | Furniture | $200 | 14 |
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Sort table rows",
        code: `include "table" | nodes | tables() | map(fn(t): t | sort_rows(0) | to_markdown();) | flatten()`,
        markdown: `# Product List

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Laptop  | Electronics | $1200 | 45 |
| Chair   | Furniture | $150 | 73 |
| Monitor | Electronics | $350 | 28 |
| Desk    | Furniture | $200 | 14 |
| Keyboard | Accessories | $80 | 35 |
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Filter table rows",
        code: `include "table" | nodes | tables() | map(fn(t): t | filter_rows(fn(row): contains(to_text(row[1]), "Electronics");) | to_markdown();) | flatten()`,
        markdown: `# Product List

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Laptop  | Electronics | $1200 | 45 |
| Monitor | Electronics | $350 | 28 |
| Chair   | Furniture | $150 | 73 |
| Desk    | Furniture | $200 | 14 |
| Keyboard | Accessories | $80 | 35 |
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Add row to table",
        code: `include "table" | nodes | tables() | map(fn(t): t | add_row(["Webcam", "Electronics", "$60", "55"]) | to_markdown();) | flatten()`,
        markdown: `# Product List

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Laptop  | Electronics | $1200 | 45 |
| Monitor | Electronics | $350 | 28 |
`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Export table to CSV",
        code: `include "table" | nodes | tables() | map(fn(t): t | to_csv();)`,
        markdown: `# Product List

| Product | Category | Price | Stock |
|---------|----------|-------|-------|
| Laptop  | Electronics | $1200 | 45 |
| Monitor | Electronics | $350 | 28 |
| Chair   | Furniture | $150 | 73 |
`,
        isUpdate: false,
        format: "markdown",
      },
    ],
  },
  {
    name: "Fuzzy Search",
    examples: [
      {
        name: "Fuzzy match strings",
        code: `include "fuzzy" | ["Introduction", "Installation", "Quick Start", "Configuration", "API Reference"] | fuzzy_match("instal")`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Filter by match score",
        code: `include "fuzzy" | ["Introduction", "Installation", "Quick Start", "Configuration", "API Reference"] | fuzzy_filter("instal", 0.75)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Best fuzzy match",
        code: `include "fuzzy" | ["Introduction", "Installation", "Quick Start", "Configuration"] | fuzzy_best_match("instal")`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
    ],
  },
  {
    name: "Custom Functions and Programming",
    examples: [
      {
        name: "Custom function",
        code: `def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(word[1:len(word)])
      | s"\${first_char}\${rest_str}";
  | join("")
end
| snake_to_camel()`,
        markdown: `# sample_codes`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Map function",
        code: `map([1, 2, 3, 4, 5], fn(x): x * 2;)`,
        markdown: `# numbers`,
        isUpdate: false,
        format: "markdown",
      },
      {
        name: "Filter function",
        code: `filter([1, 2, 3, 4, 5], fn(x): x > 3;)`,
        markdown: `# numbers`,
        isUpdate: false,
        format: "markdown",
      },
    ],
  },
  {
    name: "File Processing",
    examples: [
      {
        name: "CSV to markdown table",
        code: `include "csv" | csv_parse(true) | csv_to_markdown_table()`,
        markdown: `Product, Category, Price, Stock
Laptop, Electronics, $1200, 45
Monitor, Electronics, $350, 28
Chair, Furniture, $150, 73
Desk,  Furniture, $200, 14
Keyboard, Accessories, $80, 35
`,
        isUpdate: false,
        format: "raw",
      },
      {
        name: "JSON to markdown table",
        code: `include "json" | json_parse() | json_to_markdown_table()`,
        markdown: `
    [
      { "Product": "Laptop", "Category": "Electronics", "Price": "$1200", "Stock": 45 },
      { "Product": "Monitor", "Category": "Electronics", "Price": "$350", "Stock": 28 },
      { "Product": "Chair", "Category": "Furniture", "Price": "$150", "Stock": 73 },
      { "Product": "Desk", "Category": "Furniture", "Price": "$200", "Stock": 14 },
      { "Product": "Keyboard", "Category": "Accessories", "Price": "$80", "Stock": 35 }
    ]
`,
        isUpdate: false,
        format: "raw",
      },
    ],
  },
  {
    name: "String Functions",
    examples: [
      {
        name: "Trim and normalize case",
        code: `trim() | upcase()`,
        markdown: `  hello, mq!  `,
        isUpdate: false,
        format: "raw",
      },
      {
        name: "Split and join",
        code: `split(",") | join(" | ")`,
        markdown: `apple,banana,cherry`,
        isUpdate: false,
        format: "raw",
      },
      {
        name: "Pad numbers",
        code: `["7", "42", "123"] | map(fn(x): lpad(x, 5, "0");)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Slugify text",
        code: `slugify()`,
        markdown: `Hello, World! This is mq`,
        isUpdate: false,
        format: "raw",
      },
    ],
  },
  {
    name: "Array and Collection Functions",
    examples: [
      {
        name: "Sort by property",
        code: `[{"name": "Bob", "age": 30}, {"name": "Alice", "age": 25}, {"name": "Carol", "age": 35}]
| sort_by(fn(x): get(x, "age");)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Group by predicate",
        code: `[1, 2, 3, 4, 5, 6, 7, 8]
| group_by(fn(x): if (x % 2 == 0): "even" else: "odd";)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Chunk into groups",
        code: `[1, 2, 3, 4, 5, 6, 7] | chunks(3)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Zip two arrays",
        code: `zip(["a", "b", "c"], [1, 2, 3])`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Partition by predicate",
        code: `[1, 2, 3, 4, 5, 6] | partition(fn(x): x % 2 == 0;)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
    ],
  },
  {
    name: "Functional Programming",
    examples: [
      {
        name: "Sum with fold",
        code: `[1, 2, 3, 4, 5] | fold(0, fn(acc, x): acc + x;)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Any and all conditions",
        code: `[1, 2, 3, 4, 5]
| {"any_gt_4": any(fn(x): x > 4;), "all_positive": all(fn(x): x > 0;)}`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Filter and convert with compact_map",
        code: `["1", "abc", "3", "xyz", "5"]
| compact_map(fn(x): if (is_regex_match(x, "^[0-9]+$")): to_number(x);)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
    ],
  },
  {
    name: "Date and Time",
    examples: [
      {
        name: "Parse relative dates",
        code: `date_relative(1705276800, "3 days ago") | strftime("%Y-%m-%d")`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Format dates with strftime",
        code: `from_date("2024-01-15T00:00:00Z") | strftime("%A, %B %d, %Y")`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Add to a date",
        code: `gmtime(from_date("2024-01-15T00:00:00Z"))
| date_add(2, "months")
| mktime()
| strftime("%Y-%m-%d")`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Days between two dates",
        code: `date_diff(gmtime(from_date("2024-01-01T00:00:00Z")), gmtime(from_date("2024-03-15T00:00:00Z")), "days")`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
    ],
  },
  {
    name: "Data Format Conversion",
    examples: [
      {
        name: "Parse TOML",
        code: `include "toml" | toml_parse()`,
        markdown: `title = "mq"
version = "0.7.0"

[author]
name = "harehare"
`,
        isUpdate: false,
        format: "raw",
      },
      {
        name: "Parse XML to markdown table",
        code: `include "xml" | xml_parse() | xml_to_markdown_table()`,
        markdown: `<book><title>mq Guide</title><price>20</price></book>`,
        isUpdate: false,
        format: "raw",
      },
      {
        name: "Parse gron output",
        code: `include "gron" | gron_parse()`,
        markdown: `json = {};
json.name = "mq";
json.tags = [];
json.tags[0] = "markdown";
json.tags[1] = "cli";
`,
        isUpdate: false,
        format: "raw",
      },
      {
        name: "Stringify to TOON",
        code: `include "toon"
| {"name": "mq", "version": "0.7.0", "tags": ["cli", "markdown"]}
| toon_stringify()`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
    ],
  },
  {
    name: "Semantic Versioning",
    examples: [
      {
        name: "Compare versions",
        code: `include "semver" | semver_gt(semver_parse("2.1.0"), semver_parse("2.0.5"))`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Sort version list",
        code: `include "semver"
| ["1.2.0", "1.10.0", "1.2.10"]
| map(semver_parse)
| semver_sort()
| map(semver_to_string)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
    ],
  },
  {
    name: "Markdown Builders",
    examples: [
      {
        name: "Build heading and list",
        code: `include "md"
| doc(h("Getting Started", 2), to_md_list(text("Install mq"), 1), to_md_list(text("Run your first query"), 1))`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Build a callout",
        code: `include "md" | doc(callout(text("Breaking change in 0.7.0"), "warning", "Heads up"))`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Build a link",
        code: `include "md" | doc(strong(text("Note:")), to_link("https://mqlang.org", "mq documentation", ""))`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
    ],
  },
  {
    name: "In-place Document Updates",
    examples: [
      {
        name: "Uppercase all headings",
        code: `.h | upcase()`,
        markdown: `# Introduction

Welcome to the project.

## Installation

Run npm install.

## Usage

Use the tool like this.
`,
        isUpdate: true,
        format: "markdown",
      },
      {
        name: "Promote heading levels",
        code: `.h | increase_header_depth()`,
        markdown: `# Introduction

Welcome to the project.

## Installation

Run npm install.

## Usage

Use the tool like this.
`,
        isUpdate: true,
        format: "markdown",
      },
    ],
  },
  {
    name: "Dictionary and Path Utilities",
    examples: [
      {
        name: "Pick specific fields",
        code: `{"name": "mq", "version": "0.7.0", "license": "MIT", "internal_id": 42}
| pick(["name", "version"])`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Get and set nested values",
        code: `{"author": {"name": "harehare", "location": {"country": "Japan"}}}
| set_path(["author", "location", "city"], "Tokyo")
| get_path(["author", "location"])`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
      {
        name: "Transform dict values",
        code: `{"a": 1, "b": 2} | with_entries(fn(e): [e[0], e[1] * 10];)`,
        markdown: ``,
        isUpdate: false,
        format: "null",
      },
    ],
  },
];

// Flatten examples for backward compatibility
export const EXAMPLES = EXAMPLE_CATEGORIES.flatMap(
  (category) => category.examples,
);
