export type ExampleQuery = {
  name: string;
  code: string;
};

export type ExampleCategory = {
  name: string;
  examples: readonly ExampleQuery[];
};

/**
 * Curated queries grouped by category, adapted from the mq cookbook
 * (docs/books/src/cookbook) and packages/mq-playground/src/examples.ts.
 * Kept to selectors/functions that are useful against an arbitrary page
 * extracted by the extension, rather than the playground's fixed samples.
 */
export const EXAMPLE_CATEGORIES: readonly ExampleCategory[] = [
  {
    name: "Basics",
    examples: [
      { name: "All elements", code: "." },
      { name: "Headings", code: ".h" },
      { name: "Lists", code: ".list" },
      { name: "Tables", code: ".table" },
      { name: "Blockquotes", code: ".blockquote" },
    ],
  },
  {
    name: "Links & Images",
    examples: [
      { name: "Links", code: ".link" },
      { name: "Link URLs", code: ".link.url" },
      {
        name: "External links",
        code: `.link.url | select(starts_with("http"))`,
      },
      {
        name: "Relative links",
        code: `.link.url | select(!starts_with("http") && !starts_with("#"))`,
      },
      {
        name: "Unique link URLs",
        code: "nodes | pluck(.link.url) | uniq",
      },
      { name: "Links as Markdown list", code: ".link | to_md_list(1)" },
      { name: "Images", code: ".image" },
      { name: "Image URLs", code: ".image.url" },
      {
        name: "Images missing alt text",
        code: `select(.image.alt == "")`,
      },
    ],
  },
  {
    name: "Tables",
    examples: [
      {
        name: "First table as CSV",
        code: `nodes
| import "table"
| table::tables
| first
| table::to_csv`,
      },
      {
        name: "First table as records",
        code: `nodes
| import "table"
| table::tables
| first
| table::to_array`,
      },
    ],
  },
  {
    name: "Code Blocks",
    examples: [
      { name: "Code blocks", code: ".code" },
      { name: "Code languages", code: ".code.lang" },
      {
        name: "Unique code languages",
        code: "nodes | pluck(.code.lang) | uniq",
      },
      {
        name: "Shell commands",
        code: `select(.code.lang == "bash" || .code.lang == "sh" || .code.lang == "shell")
| to_text()`,
      },
      { name: "Inline code", code: ".code_inline" },
      { name: "Exclude code blocks", code: "select(!.code)" },
    ],
  },
  {
    name: "Headings & Structure",
    examples: [
      { name: "Top-level headings", code: ".h(1)" },
      { name: "H2 & H3 headings", code: ".h(2, 3)" },
      {
        name: "Table of contents",
        code: `.h
| let text = to_text()
| let anchor = downcase(replace(text, " ", "-"))
| let link = to_link("#" + anchor, text, "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, level - 1)
| to_string()`,
      },
      {
        name: "Outline",
        code: `.h
| let level = .h.depth
| if (!is_none(level)): to_md_list(to_text(), level - 1)
| to_string()`,
      },
      {
        name: "Question headings",
        code: `.h | select(ends_with(to_text(), "?"))`,
      },
      { name: "Uppercase headings", code: ".h | upcase()" },
    ],
  },
  {
    name: "Sections",
    examples: [
      {
        name: "Section by title",
        code: `# Change "Installation" to a heading on the page.
nodes
| import "section"
| section::section("Installation")
| section::collect()`,
      },
      {
        name: "Code in section",
        code: `# Change "Installation" to a heading on the page.
nodes
| import "section"
| section::section("Installation")
| .code
| section::collect()`,
      },
      {
        name: "H2 section titles",
        code: `nodes
| import "section"
| section::sections()
| section::by_level(2)
| section::titles()`,
      },
    ],
  },
  {
    name: "Stats & Tasks",
    examples: [
      {
        name: "Word count",
        code: `nodes
| map(fn(n): to_text(n) | split(" ") | len;)
| fold(0, fn(acc, x): acc + x;)`,
      },
      {
        name: "Reading time",
        code: `nodes
| map(fn(n): to_text(n) | split(" ") | len;)
| fold(0, fn(acc, x): acc + x;)
| let words = self
| s"\${words} words, \${ceil(words / 200)} min read"`,
      },
      {
        name: "Document statistics",
        code: `nodes
| let headers = count_by(fn(x): x | select(.h);)
| let paragraphs = count_by(fn(x): x | select(.text);)
| let code_blocks = count_by(fn(x): x | select(.code);)
| let links = count_by(fn(x): x | select(.link);)
| s"Headers: \${headers}, Paragraphs: \${paragraphs}, Code: \${code_blocks}, Links: \${links}"`,
      },
      { name: "Completed tasks", code: ".done" },
      {
        name: "Task progress",
        code: `nodes
| let total = count_by(fn(x): x | select(.list);)
| let done = count_by(fn(x): x | select(.list.checked == true);)
| s"\${done}/\${total} done"`,
      },
    ],
  },
];

// Flattened for lookup and backward compatibility.
export const EXAMPLE_QUERIES: readonly ExampleQuery[] = EXAMPLE_CATEGORIES.flatMap(
  (category) => category.examples,
);
