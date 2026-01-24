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
        code: `# Hello world.
select(.h || .list || .code) + " world"`,
        markdown: `# Hello

- Hello

\`\`\`
Hello
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
];

// Flatten examples for backward compatibility
export const EXAMPLES = EXAMPLE_CATEGORIES.flatMap(
  (category) => category.examples,
);
