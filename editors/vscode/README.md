# mq support for Visual Studio Code

This extension adds support for the mq to Visual Studio Code.
[mq](https://github.com/meros-debray/mq) is a jq like tool for markdown processing.

This extension provides essential coding assistance for `.mq` files, helping you write and maintain mq code efficiently.

## Features

- Smart code completion
- Go to definition navigation
- Hover information
- Document symbol navigation
- Code formatting
- Syntax highlighting
- Run selected code (execute mq queries directly from the editor)

## Available Commands

- `mq: Install LSP Server`: Installs the mq Language Server Protocol server
- `mq: Start LSP Server`: Starts the mq Language Server Protocol server
- `mq: Set selected text as input`: Sets the currently selected text as input for mq queries
- `mq: Show input text`: Show the currently input for mq queries
- `mq: Run selected text`: Executes the selected mq query against the current input data

## Configuration

The extension can be configured through Visual Studio Code settings:

- `mq-lsp.lspPath`: Path to the mq language server binary
- `mq-lsp.showExamplesInNewFile`: To Show/Hide examples in new file

You can customize these settings in your VS Code settings.json file or through the Settings UI.

## Playground

An [Online Playground](https://harehare.github.io/mq/playground) is available, powered by WebAssembly.

## Example

### Basic Example

```jq
# Extract all headings
.h
```

### Advanced Examples

```jq
# Extract code blocks with their language
.code("js")
```

```jq
# Find paragraphs containing specific text
select(contains("important"))
```

```jq
# List all links with their text
.links[] | {text, url}
```

```jq
# Define and use a custom function
def important_headings():
    .h | select(contains("Important"));
| important_headings()
```

For more detailed usage and examples, refer to the [documentation](https://github.com/harehare/mq/blob/master/docs/README.md).

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
