<div>
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

<h1 align="center">@mqlang/node</h1>

A Node.js package for running [mq](https://github.com/harehare/mq) (a jq-like command-line tool for Markdown processing) using WebAssembly.

## Installation

```bash
npm install @mqlang/node
```

## Quick Start

```typescript
import { run, format, htmlToMarkdown } from "@mqlang/node";

// Transform markdown list style
const markdown = `- First item
- Second item
- Third item`;

const result = await run(".[]", markdown, { listStyle: "star" });
console.log(result);
// Output:
// * First item
// * Second item
// * Third item

// Format mq code
const formatted = await format("map(to_text)|select(gt(5))");
console.log(formatted);
// Output: map(to_text) | select(gt(5))

// Convert HTML to Markdown
const html = "<h1>Hello World</h1><p>This is a paragraph.</p>";
const md = await htmlToMarkdown(html);
console.log(md);
// Output:
// # Hello World
//
// This is a paragraph.
```

## API Reference

### Functions

#### `run(code, content, options?)`

Run an mq script on markdown content.

- `code`: string - The mq script to execute
- `content`: string - The markdown content to process
- `options`: Partial<Options> - Processing options

Returns: `Promise<string>` - The processed output

#### `format(code)`

Format mq code.

- `code`: string - The mq code to format

Returns: `Promise<string>` - The formatted code

#### `diagnostics(code, enableTypeCheck?)`

Get diagnostics for mq code.

- `code`: string - The mq code to analyze
- `enableTypeCheck`: boolean (optional) - Enable type checking

Returns: `Promise<ReadonlyArray<Diagnostic>>` - Array of diagnostic messages

#### `inlayHints(code)`

Get inlay type hints for mq code.

- `code`: string - The mq code to analyze

Returns: `Promise<ReadonlyArray<InlayHint>>` - Array of inlay hints

#### `definedValues(code, module?)`

Get defined values (functions, selectors, variables) from mq code.

- `code`: string - The mq code to analyze
- `module`: string (optional) - Module name

Returns: `Promise<ReadonlyArray<DefinedValue>>` - Array of defined values

#### `toAst(code)`

Convert mq code to its AST (Abstract Syntax Tree) representation.

- `code`: string - The mq code to convert

Returns: `Promise<string>` - The AST representation

#### `htmlToMarkdown(html, options?)`

Convert HTML to markdown format.

- `html`: string - The HTML content to convert
- `options`: Partial<ConversionOptions> - Conversion options

Returns: `Promise<string>` - The converted markdown content

#### `toHtml(markdownInput)`

Convert Markdown to HTML.

- `markdownInput`: string - The markdown content to convert

Returns: `Promise<string>` - The converted HTML content

## Examples

### Extract and Transform Headings

```typescript
import { run } from "@mqlang/node";

const markdown = `# Main Title
Some content here.

## Section 1
More content.

### Subsection
Even more content.`;

// Extract all headings
const headings = await run(".[] | select(.h)", markdown);
```

### List Transformations

```typescript
import { run } from "@mqlang/node";

const markdown = `- Apple
- Banana
- Cherry`;

// Change list style
const starList = await run(".[]", markdown, { listStyle: "star" });
// Output: * Apple\n* Banana\n* Cherry

// Filter list items
const filtered = await run('.[] | select(test(to_text(), "^A"))', markdown);
// Output: - Apple

// Transform list items
const uppercase = await run(".[] | upcase()", markdown);
// Output: - APPLE\n- BANANA\n- CHERRY
```

### HTML to Markdown Conversion

```typescript
import { htmlToMarkdown } from "@mqlang/node";

const html = `
<article>
  <h1>Welcome to mq</h1>
  <p>A <strong>powerful</strong> tool for <em>Markdown</em> processing.</p>
  <ul>
    <li>Easy to use</li>
    <li>Fast performance</li>
    <li>Web-compatible</li>
  </ul>
</article>
`;

const markdown = await htmlToMarkdown(html);
console.log(markdown);
// Output:
// # Welcome to mq
//
// A **powerful** tool for _Markdown_ processing.
//
// - Easy to use
// - Fast performance
// - Web-compatible
```

### Error Handling

```typescript
import { run, diagnostics } from "@mqlang/node";

try {
  const result = await run("invalid syntax", "content");
} catch (error) {
  console.error("Runtime error:", error.message);
}

// Get detailed diagnostics
const diags = await diagnostics("invalid syntax");
diags.forEach((diag) => {
  console.log(`Error at line ${diag.startLine}: ${diag.message}`);
});
```

## License

MIT License - see the main [mq](https://github.com/harehare/mq) repository for details.

