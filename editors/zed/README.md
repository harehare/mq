# mq Extension for Zed

This extension provides language support for [mq](https://mqlang.org/) in the Zed editor.

mq is a jq-like command-line tool for Markdown processing. It allows you to easily slice, filter, map, and transform Markdown files.

## Features

- Syntax highlighting for `.mq` files using Tree-sitter grammar
- Language Server Protocol (LSP) support via `mq-lsp`
- Auto-completion and diagnostics
- Code navigation and hover information

## Installation

1. Open Zed
2. Open the Extensions panel (`cmd-shift-x` on macOS)
3. Search for "mq"
4. Click "Install"

The extension will automatically download and install the `mq-lsp` language server on first use.

## Usage

Create or open a `.mq` file to activate the extension. The language server will provide:

- Syntax highlighting
- Code completion
- Diagnostics and error checking
- Hover information
- Go to definition

## Configuration

The extension will automatically detect and use `mq-lsp` if it's in your PATH. Otherwise, it will download the appropriate binary for your platform from the [mq GitHub releases](https://github.com/harehare/mq/releases).

## Contributing

Contributions are welcome! Please see the [mq repository](https://github.com/harehare/mq) for more information.

## License

This extension is provided under the MIT License. See the [LICENSE](../../LICENSE) file for details.
