# External Subcommands

You can extend `mq` with custom subcommands by placing executable files starting with `mq-` in `~/.local/bin/` or anywhere in your `PATH`.

## Command Resolution

When you run `mq <subcommand>`, mq searches for an executable named `mq-<subcommand>` in the following order:

1. `~/.local/bin/` directory
2. Directories in `PATH`

The first match found is used.

## Listing Available Subcommands

Use the `--list` flag to see all available subcommands:

```sh
mq --list
```

This makes it easy to add your own tools and workflows to `mq` without modifying the core binary.

## External Tools

The following external tools are available to extend mq's functionality:

- [mq-check](https://github.com/harehare/mq/blob/main/crates/mq-check/README.md) - A syntax and semantic checker for mq files.
- [mq-conv](https://github.com/harehare/mq-conv) - A CLI tool for converting various file formats to Markdown.
- [mq-crawler](https://github.com/harehare/mq/blob/main/crates/mq-crawler/README.md) - A web crawler that extracts structured data from websites and outputs it in Markdown format.
- [mq-docs](https://github.com/harehare/mq-docs) - A documentation generator for mq functions, macros, and selectors.
- [mq-fmt](https://github.com/harehare/mq-fmt) - Formatter for mq query language (.mq) files.
- [mq-http](https://github.com/harehare/mq-http) - A lightweight HTTP server that executes mq scripts for each request.
- [mq-lsp](https://github.com/harehare/mq/tree/main/crates/mq-lsp/README.md) - Language Server Protocol (LSP) implementation for mq query files, providing IDE features like completion, hover, and diagnostics.
- [mq-mcp](https://github.com/harehare/mq-mcp) - Model Context Protocol (MCP) server implementation for AI assistants.
- [mq-serve](https://github.com/harehare/mq-serve) - A browser-based Markdown viewer with mq query support.
- [mq-task](https://github.com/harehare/mq-task) - Task runner using mq for Markdown-based task definitions.
- [mq-tui](https://github.com/harehare/mq-tui) - Terminal User Interface (TUI) for interactive mq query.
- [mq-update](https://github.com/harehare/mq-update) - Update mq binary to the latest version.
- [mq-view](https://github.com/harehare/mq-view) - Viewer for Markdown content.

## AI Assistant Integration

- **MCP**: [mq-mcp](https://github.com/harehare/mq-mcp) provides a Model Context Protocol server, enabling mq to be used from any MCP-compatible AI assistant.
- **Skill**: The [processing-markdown](https://github.com/harehare/mq/blob/main/skills/processing-markdown/SKILL.md) skill adds mq-aware assistance directly to your AI coding workflow.
