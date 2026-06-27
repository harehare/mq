<h1 align="center">mq Extension for Zed</h1>

This extension provides language support for [mq](https://mqlang.org/) in the [Zed](https://zed.dev/) editor.

[mq](https://github.com/harehare/mq) is a jq-like command-line tool for Markdown processing. It allows you to easily slice, filter, map, and transform Markdown files.

## Features

- Syntax highlighting for `.mq` files using Tree-sitter grammar
- Language Server Protocol (LSP) support via `mq-lsp`
- Auto-completion and diagnostics
- Code navigation and hover information with type display
- Inlay hints for inline type annotations
- Type checking support (configurable via settings)
- AI Assistant slash commands for querying Markdown files

## Installation

1. Open Zed
2. Open the Extensions panel (`cmd-shift-x` on macOS)
3. Search for "mq"
4. Click "Install"

The extension will automatically download and install the `mq-lsp` language server on first use.

To use the slash commands, `mq` must also be installed and available on your `$PATH`. See [mqlang.org](https://mqlang.org) for installation instructions.

## Usage

### Language Server

Create or open a `.mq` file to activate the extension. The language server will provide:

- Syntax highlighting
- Code completion
- Diagnostics and error checking
- Hover information
- Go to definition

### AI Assistant Slash Commands

The extension adds slash commands to Zed's AI Assistant panel (`cmd+?`) that use [mq](https://github.com/harehare/mq) to extract structured content from Markdown files.

| Command | Description |
|---|---|
| `/mq-outline [file]` | Extract heading structure |
| `/mq-code [file]` | Extract all code blocks |
| `/mq-todo [file]` | Extract unchecked task items |
| `/mq-changelog [file]` | Extract the latest section from a changelog |
| `/mq <query> [file]` | Run any mq query |

If `[file]` is omitted, the command runs across all `.md` and `.mdx` files in the current workspace. Tab completion is available for file paths.

**Examples:**

```
# Extract headings from a single file
/mq-outline docs/spec.md

# Collect all unchecked tasks across the workspace
/mq-todo

# Run a custom query
/mq ".h2 | select(.text | contains(\"API\"))" docs/
```

## Configuration

The extension will automatically detect and use `mq-lsp` if it's in your PATH. Otherwise, it will download the appropriate binary for your platform from the [mq GitHub releases](https://github.com/harehare/mq/releases).

### Type Checking

Enable type checking by adding the following to your Zed `settings.json` (`cmd-,`):

```json
{
  "lsp": {
    "mq-lsp": {
      "initialization_options": {
        "enableTypeCheck": true,
        "strictArray": false
      }
    }
  }
}
```

| Option | Default | Description |
|---|---|---|
| `enableTypeCheck` | `false` | Enable type checking; passes `--enable-type-checking` to `mq-lsp` |
| `strictArray` | `false` | Arrays must contain elements of a single type (requires `enableTypeCheck`) |

### Linting

Enable `mq-lint` diagnostics (correctness, style, complexity, selector, and module rules):

```json
{
  "lsp": {
    "mq-lsp": {
      "initialization_options": {
        "enableLint": true,
        "lintDisabledRules": ["naming_convention", "shadow_variable"]
      }
    }
  }
}
```

| Option | Default | Description |
|---|---|---|
| `enableLint` | `false` | Enable `mq-lint` diagnostics; passes `--enable-lint` to `mq-lsp` |
| `lintDisabledRules` | `[]` | Lint rule IDs to disable (requires `enableLint`) |

### Inlay Hints

Inlay hints show inferred types inline in the editor. Enable them in your Zed `settings.json`:

```json
{
  "inlay_hints": {
    "enabled": true
  }
}
```

### Custom Binary Path

To use a specific `mq-lsp` binary, configure it in your Zed `settings.json`:

```json
{
  "lsp": {
    "mq-lsp": {
      "binary": {
        "path": "/path/to/mq-lsp"
      }
    }
  }
}
```

## Contributing

Contributions are welcome! Please see the [mq repository](https://github.com/harehare/mq) for more information.

## License

This extension is provided under the MIT License. See the [LICENSE](LICENSE) file for details.
