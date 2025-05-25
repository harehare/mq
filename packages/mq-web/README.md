<div align="center">
  <img src="https://mqlang.org/assets/logo.svg" style="width: 128px; height: 128px;"/>
</div>

<div>
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

# mq-web

A TypeScript/JavaScript package for running [mq](https://github.com/harehare/mq) (a jq-like command-line tool for Markdown processing) in web environments using WebAssembly.

## Installation

```bash
npm install mq-web
```

## Quick Start

```typescript
import { run, format } from "mq-web";

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

#### `diagnostics(code)`

Get diagnostics for mq code.

- `code`: string - The mq code to analyze

Returns: `Promise<Diagnostic[]>` - Array of diagnostic messages

#### `definedValues(code)`

Get defined values (functions, selectors, variables) from mq code.

- `code`: string - The mq code to analyze

Returns: `Promise<DefinedValue[]>` - Array of defined values

## Examples

### Extract and Transform Headings

```typescript
import { run } from "mq-web";

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
import { run } from "mq-web";

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

### Error Handling

```typescript
import { run, diagnostics } from "mq-web";

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
