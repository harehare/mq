<div align="center">
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

<h1 align="center">mq for Visual Studio Code</h1>

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

- `mq-lsp.lspPath`: Path to the mq language server binary
- `mq-lsp.showExamplesInNewFile`: To Show/Hide examples in new file
- `editor.semanticHighlighting.enabled`: Set to `true` to enable semantic token highlighting for improved code visualization

You can customize these settings in your VS Code settings.json file or through the Settings UI.

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
$ mq 'select(not(.code("js")))'
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
