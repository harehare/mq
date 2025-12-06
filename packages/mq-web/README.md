<div>
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

<h1 align="center">mq-web</h1>

A TypeScript/JavaScript package for running [mq](https://github.com/harehare/mq) (a jq-like command-line tool for Markdown processing) in web environments using WebAssembly.

## Installation

```bash
npm install mq-web
```

## Quick Start

```typescript
import { run, format, htmlToMarkdown } from "mq-web";

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

#### `diagnostics(code)`

Get diagnostics for mq code.

- `code`: string - The mq code to analyze

Returns: `Promise<ReadonlyArray<Diagnostic>>` - Array of diagnostic messages

#### `definedValues(code)`

Get defined values (functions, selectors, variables) from mq code.

- `code`: string - The mq code to analyze

Returns: `Promise<ReadonlyArray<DefinedValue>>` - Array of defined values

#### `htmlToMarkdown(html, options?)`

Convert HTML to markdown format.

- `html`: string - The HTML content to convert
- `options`: Partial<ConversionOptions> - Conversion options

Returns: `Promise<string>` - The converted markdown content

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

### HTML to Markdown Conversion

```typescript
import { htmlToMarkdown } from "mq-web";

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

### Working with OPFS (Origin Private File System)

mq-web supports importing custom modules from OPFS (Origin Private File System), allowing you to create and reuse mq modules in web environments. When you call the `run()` function, mq-web automatically loads all `.mq` files from OPFS and makes them available for import.

#### Creating Module Files in OPFS

```typescript
import { run } from "mq-web";

// Get the OPFS root directory
const root = await navigator.storage.getDirectory();

// Create a module file
const fileHandle = await root.getFileHandle("utils.mq", { create: true });
const writable = await fileHandle.createWritable();
await writable.write(`
  def double(x): x * 2;
  def triple(x): x * 3;
`);
await writable.close();

// Create another module file
const textUtilsHandle = await root.getFileHandle("text_utils.mq", { create: true });
const textWritable = await textUtilsHandle.createWritable();
await textWritable.write(`
  def shout(text): text | upcase() | s"\${self}!!!";
`);
await textWritable.close();
```

#### Importing OPFS Modules

Once you've created `.mq` module files in OPFS, you can import and use them in your mq code. The `run()` function automatically preloads all `.mq` files from OPFS:

```typescript
import { run } from "mq-web";

// Import and use the module
const markdown = "5";

const result1 = await run(`
  import "utils"
  | to_number() | utils::double()
`, markdown);
// Output: 10

const result2 = await run(`
  import "text_utils"
  | text_utils::shout()
`, "hello world");
// Output: HELLO WORLD!!!
```

#### Module Resolution Rules

- Module files must have a `.mq` extension in OPFS (e.g., `utils.mq`)
- When importing, use the module name without the extension (e.g., `import "utils"`)
- Modules are automatically preloaded from the OPFS root directory when you call `run()`
- Use the `module_name::function_name()` syntax to call functions from imported modules

## License

MIT License - see the main [mq](https://github.com/harehare/mq) repository for details.
