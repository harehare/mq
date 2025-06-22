<div align="center">
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

# mq for Visual Studio Code

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

## Configuration

The extension can be configured through Visual Studio Code settings:

- `mq-lsp.lspPath`: Path to the mq language server binary
- `mq-lsp.showExamplesInNewFile`: To Show/Hide examples in new file
- `editor.semanticHighlighting.enabled`: Set to `true` to enable semantic token highlighting for improved code visualization

You can customize these settings in your VS Code settings.json file or through the Settings UI.

## Example

### Basic Example

```python
# Extract all headings
.h
```

### Advanced Examples

```python
# Extract code blocks with their language
.code("js")
```

```python
# Find paragraphs containing specific text
select(contains("important"))
```

```python
# Define and use a custom function
def important_headings():
    .h | select(contains("Important"));
| important_headings()
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
