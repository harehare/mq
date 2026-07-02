<div align="center">
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

<h1 align="center">mq for Visual Studio Code</h1>

<div align="center">

[![Visual Studio Marketplace Version](https://img.shields.io/visual-studio-marketplace/v/harehare.vscode-mq?style=flat-square&label=VS%20Marketplace&logo=visualstudiocode)](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq)
[![Visual Studio Marketplace Installs](https://img.shields.io/visual-studio-marketplace/i/harehare.vscode-mq?style=flat-square&logo=visualstudiocode)](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq)
[![Open VSX Version](https://img.shields.io/open-vsx/v/harehare/vscode-mq?style=flat-square&logo=eclipseide)](https://open-vsx.org/extension/harehare/vscode-mq)

</div>

This extension adds support for the mq to Visual Studio Code.
[mq](https://github.com/harehare/mq) is a jq like tool for markdown processing.

This extension provides essential coding assistance for `.mq` files, helping you write and maintain mq code efficiently.

## Features

- Smart code completion
- Go to definition navigation
- Hover information
- Document symbol navigation
- Code formatting
- Syntax highlighting
- Execute mq script directly from the editor

## Available Commands

- `mq: Install LSP Server`: Installs the mq Language Server Protocol server
- `mq: Start LSP Server`: Starts the mq Language Server Protocol server
- `mq: Run selected text`: Executes the currently selected text in the editor as an mq
- `mq: Execute mq file`: Executes the selected mq file to the text in active editor
- `mq: Execute query`: Executes the input mq query on the text in the active editor
- `mq: Debug current file`: Launches the mq debugger for the currently open file, allowing you to step through mq code, inspect variables, and analyze execution flow directly within the editor.

## Configuration

The extension can be configured through Visual Studio Code settings:

- `mq.lspPath`: Path to the mq language server binary
- `mq.showExamplesInNewFile`: To Show/Hide examples in new file
- `mq.enableCodeLens`: Enable/disable the CodeLens for running mq queries
- `mq.typeCheck.enableTypeCheck`: Enable type checking diagnostics (passes `--enable-type-checking` to `mq-lsp`)
- `mq.typeCheck.strictArray`: Require arrays to contain elements of a single type (passes `--strict-array` to `mq-lsp`, requires `mq.typeCheck.enableTypeCheck`)
- `mq.lint.enableLint`: Enable `mq-lint` diagnostics (correctness, style, complexity, selector, and module rules)
- `mq.lint.disabledRules`: Lint rule IDs to disable (e.g. `"naming_convention"`, `"shadow_variable"`). Only effective when `mq.lint.enableLint` is `true`
- `editor.semanticHighlighting.enabled`: Set to `true` to enable semantic token highlighting for improved code visualization

You can customize these settings in your VS Code settings.json file or through the Settings UI.

### Type Checking

Enable type checking to get real-time type errors and richer hover type information:

```json
{
  "mq.typeCheck.enableTypeCheck": true,
  "mq.typeCheck.strictArray": true
}
```

### Linting

Enable `mq-lint` diagnostics (correctness, style, complexity, selector, and module rules):

```json
{
  "mq.lint.enableLint": true,
  "mq.lint.disabledRules": ["naming_convention", "shadow_variable"]
}
```

## Example

### Basic Example

```sh
# code
$ mq '.code | select(contains("name"))'
# Extracts the language name from code blocks.
$ mq '.code.lang'
# Extracts the url from link.
$ mq '.link.url'
# table
$ mq '.[][] | select(contains("name"))'
# list or header
$ mq 'or(.[], .h) | select(contains("name"))'
# Exclude js code
$ mq '.code | select(.code.lang != "js")'
# CSV to markdown table
$ mq 'include "csv" | nodes | csv_parse(true) | csv_to_markdown_table()' example.csv
```

### Advanced Usage

You can chain multiple operations to perform complex transformations:

```sh
# Markdown TOC
$ mq '.h | let link = to_link("#" + to_text(self), to_text(self), "") | let level = .h.level | if (not(is_none(level))): to_md_list(link, level)' docs/books/**/*.md
# String Interpolation
$ mq 'let name = "Alice" | let age = 30 | s"Hello, my name is ${name} and I am ${age} years old."'
# Merging Multiple Files
$ mq -S 's"\n${__FILE__}\n"' 'identity()' docs/books/**/**.md
# Extract all code blocks from an HTML file
$ mq '.code' example.html
# Convert HTML to Markdown and filter headers
$ mq 'select(or(.h1, .h2))' example.html
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
