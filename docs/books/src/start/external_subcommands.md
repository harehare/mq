# External Subcommands

You can extend `mq` with custom subcommands by placing executable files starting with `mq-` in `~/.mq/bin/` or anywhere in your `PATH`.

## Creating a Custom Subcommand

Create an executable file with the `mq-` prefix in `~/.mq/bin/` or a directory on your `PATH`:

```sh
# Create a custom subcommand
cat > ~/.mq/bin/mq-hello << 'EOF'
#!/bin/bash
echo "Hello from mq-hello!"
echo "Arguments: $@"
EOF
chmod +x ~/.mq/bin/mq-hello

# Use the custom subcommand
mq hello world
# Output: Hello from mq-hello!
#         Arguments: world
```

## Command Resolution

When you run `mq <subcommand>`, mq searches for an executable named `mq-<subcommand>` in the following order:

1. `~/.mq/bin/` directory
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

- [mq-check](https://github.com/harehare/mq-check) - A syntax and semantic checker for mq files.
- [mq-conv](https://github.com/harehare/mq-conv) - A CLI tool for converting various file formats to Markdown.
- [mq-docs](https://github.com/harehare/mq-docs) - A documentation generator for mq functions, macros, and selectors.
- [mq-edit](https://github.com/harehare/mq-edit) - A terminal-based Markdown and code editor with WYSIWYG rendering and LSP support.
- [mq-mcp](https://github.com/harehare/mq-mcp) - Model Context Protocol (MCP) server implementation for AI assistants.
- [mq-task](https://github.com/harehare/mq-task) - Task runner using mq for Markdown-based task definitions.
- [mq-tui](https://github.com/harehare/mq-tui) - Terminal User Interface (TUI) for interactive mq query.
- [mq-update](https://github.com/harehare/mq-update) - Update mq binary to the latest version.
- [mq-view](https://github.com/harehare/mq-view) - Viewer for Markdown content.
