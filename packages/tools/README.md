# Tools

A web-based collection of tools for transforming and processing Markdown documents, powered by `mq-web`.

## Features

A versatile set of tools to handle various Markdown-related tasks. Select a tool from the dropdown menu, input your text, and get the result instantly.

### Available Tools

- **HTML to Markdown**: Convert HTML content to Markdown format.
- **Markdown to TOC**: Generate a Table of Contents from Markdown headings.
- **CSV to Markdown Table**: Convert CSV data into a Markdown table.
- **JSON to Markdown**: Convert JSON data to a Markdown table.
- **Markdown Link Extractor**: Extract all links (URLs) from a Markdown document.
- **Markdown to HTML**: Convert Markdown to HTML format.

## Development

### Prerequisites

- Node.js and npm
- `mq-web` package

### Getting Started

1. **Install dependencies:**
   ```bash
   npm install
   ```

2. **Run the development server:**
   ```bash
   npm run dev
   ```
   The application will be available at `http://localhost:5173` (or another port if 5173 is in use).

### How to Use

1. Select the desired tool from the dropdown menu.
2. Enter your source text into the left input area.
3. The conversion happens automatically with a 500ms delay after typing.
4. The result will appear in the right output area.
5. You can toggle between raw text view and a rendered HTML preview for the output.
6. Use the tree view toggle (🌲) to see the document outline for Markdown headings.
7. Toggle between light and dark mode with the theme button (☀️/🌙).

## Adding a New Tool

To add a new tool, simply add a new `Tool` object to the `tools` array in `src/tools.ts`. The UI will update automatically.
A tool definition looks like this:
```typescript
{
  id: 'unique-tool-id',
  name: 'Display Name of Tool',
  description: 'A short description of what the tool does.',
  path: '/tool-path',
  transform: (input: string) => Promise<string>; // The conversion logic using mq-web
}
```
