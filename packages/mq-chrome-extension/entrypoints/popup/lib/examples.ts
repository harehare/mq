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
      { name: "Images", code: ".image" },
      {
        name: "Images missing alt text",
        code: `select(.image.alt == "")`,
      },
    ],
  },
  {
    name: "Code Blocks",
    examples: [
      { name: "Code blocks", code: ".code" },
      { name: "Code languages", code: ".code.lang" },
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
| if (!is_none(level)): to_md_list(link, level - 1)`,
      },
      { name: "Uppercase headings", code: ".h | upcase()" },
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
