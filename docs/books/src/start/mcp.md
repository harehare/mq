# MCP

The mq MCP server enables integration with AI applications that support the Model Context Protocol (MCP). This server provides tools for processing Markdown content using mq queries.

## Overview

The MCP server exposes four main tools:

- `html_to_markdown` - Converts HTML to Markdown and applies mq queries
- `extract_markdown` - Extracts content from Markdown using mq queries
- `available_functions` - Lists available mq functions
- `available_selectors` - Lists available mq selectors

## Configuration

### Claude Desktop

Add the following to your Claude Desktop configuration file:

```json
{
  "mcpServers": {
    "mq": {
      "command": "/path/to/mq",
      "args": ["mcp"]
    }
  }
}
```

### Claude Code

```bash
$ claude mcp add mq-mcp -- mq mcp
```

### VS Code

Add the following to your VS Code settings:

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

## Usage

### Converting HTML to Markdown

The `html_to_markdown` tool converts HTML content to Markdown format and applies an optional mq query:

```
html_to_markdown({
  "html": "<h1>Title</h1><p>Content</p>",
  "query": ".h1"
})
```

### Extracting from Markdown

The `extract_markdown` tool processes Markdown content with mq queries:

```
extract_markdown({
  "markdown": "# Title\n\nContent",
  "query": ".h1"
})
```

### Getting Available Functions

The `available_functions` tool returns all available mq functions:

```
available_functions()
```

Returns JSON with function names, descriptions, parameters, and examples.

### Getting Available Selectors

The `available_selectors` tool returns all available mq selectors:

```
available_selectors()
```

Returns JSON with selector names, descriptions, and parameters.

## Query Examples

Common mq queries you can use with the MCP tools:

- `.h1` - Select all h1 headings
- `select(.code.lang == "js")` - Select JavaScript code blocks
- `.text` - Extract all text content
- `select(.h1, .h2)` - Select h1 and h2 headings
- `select(not(.code))` - Select everything except code blocks
