# mq-mcp

The `mq-mcp` crate provides the MCP functionality for the `mq`, a jq-like processor for Markdown files. It handles the evaluation and execution of commands against Markdown documents.

## Features

- **Markdown Extraction**: Extract specific elements from markdown files using various selectors
- **Element Selectors**: Support for various markdown elements like headings, lists, code blocks, etc.
- **Query Functions**: Filter and transform extracted content with functions like contains, startsWith, etc.

### Supported Selectors

The server supports extracting various markdown elements including:

- Headings (h1-h5)
- Lists (regular and checked)
- Code blocks (regular and inline)
- Tables
- Math expressions
- HTML blocks
- Frontmatter (YAML/TOML)
- Blockquotes
- Links and images
- Formatting (emphasis, strong, strikethrough)
- And more

### Query Functions

Extracted content can be further processed with functions:

- `contains`: Check if content contains a substring
- `startsWith`/`endsWith`: Test if content starts or ends with specific text
- `test`: Use pattern matching against content
- `toHtml`: Convert markdown to HTML
- `replace`: Replace substrings within content

## Usage

### VS Code

To use `mq-mcp` with Visual Studio Code, add the following configuration to your `.vscode/settings.json`:

```json
{
  "mcp": {
    "servers": {
      "mq-mcp": {
        "type": "stdio",
        "command": "/path/to/mq-mcp",
        "args": []
      }
    }
  }
}
```

Replace `/path/to/mq-mcp` with the actual path to your `mq-mcp` binary.

### Claude

For integrating with Claude:

```json
{
  "mcpServers": {
    "mcp": {
      "mq-mcp": {
        "command": "/path/to/mq-mcp",
        "args": []
      }
    }
  }
}
```

## License

This project is licensed under the MIT License
