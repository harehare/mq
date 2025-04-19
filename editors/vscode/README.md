# mq support for Visual Studio Code

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
- `mq: Set selected text as input`: Sets the currently selected text as input for mq queries
- `mq: Show input text`: Show the currently input for mq queries
- `mq: Execute mq file`: Executes the selected mq file to the text in active editor

## Configuration

The extension can be configured through Visual Studio Code settings:

- `mq-lsp.lspPath`: Path to the mq language server binary
- `mq-lsp.showExamplesInNewFile`: To Show/Hide examples in new file
- `editor.semanticHighlighting.enabled`: Set to `true` to enable semantic token highlighting for improved code visualization

You can customize these settings in your VS Code settings.json file or through the Settings UI.

## Playground

An [Online Playground](https://mqlang.org/playground) is available, powered by WebAssembly.

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
# List all links with their text
.links[] | {text, url}
```

```python
# Define and use a custom function
def important_headings():
    .h | select(contains("Important"));
| important_headings()
```

For more detailed usage and examples, refer to the [documentation](https://mqlang.org/book/).

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
