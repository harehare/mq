# mq-mcp (Markdown Command Processor)

[![crates.io](https://img.shields.io/crates/v/mq-mcp.svg)](https://crates.io/crates/mq-mcp)
[![docs.rs](https://docs.rs/mq-mcp/badge.svg)](https://docs.rs/mq-mcp)

## Overview

The `mq-mcp` crate provides the MCP functionality for the `mq`, a jq-like processor for Markdown files. It handles the evaluation and execution of commands against Markdown documents.

## Features

- Command parsing and execution
- Integration with the `mq-markdown` parser
- Command pipeline processing
- Transformation of Markdown content

## Using with code editors

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
