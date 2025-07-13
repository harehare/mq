# mq-mcp

Model Context Protocol (MCP) server implementation for mq. This crate provides an MCP server that allows AI assistants to process Markdown and HTML content using mq's query language.

## Implementation

The server implements four MCP tools:

- `html_to_markdown`: Converts HTML to Markdown and executes mq queries
- `extract_markdown`: Executes mq queries on Markdown content  
- `available_functions`: Returns available functions for mq queries
- `available_selectors`: Returns available selectors for mq queries

### Tool Parameters

#### html_to_markdown
- `html` (string): HTML content to process
- `query` (optional string): mq query to execute

#### extract_markdown  
- `markdown` (string): Markdown content to process
- `query` (string): mq query to execute

#### available_functions
No parameters. Returns JSON with function names, descriptions, parameters, and example queries.

#### available_selectors
No parameters. Returns JSON with selector names, descriptions, and parameters.

## Configuration

### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "mq-mcp": {
      "command": "/path/to/mq",
      "args": ["mcp"]
    }
  }
}
```

### VS Code with MCP Extension

Add to `.vscode/settings.json`:

```json
{
  "mcp": {
    "servers": {
      "mq-mcp": {
        "type": "stdio", 
        "command": "/path/to/mq",
        "args": ["mcp"]
      }
    }
  }
}
```

Replace `/path/to/mq` with the actual path to your mq binary.

## License

This project is licensed under the MIT License
