<div align="center">
  <a href="https://mqlang.org">Visit the site 🌐</a>
  &mdash;
  <a href="https://mqlang.org/book">Read the book 📖</a>
  &mdash;
  <a href="https://mqlang.org/playground">Playground 🎮</a>
</div>

<h1 align="center">mq for JetBrains IDEs</h1>

This plugin adds support for [mq](https://github.com/harehare/mq) — a jq-like tool for Markdown processing — to
IntelliJ IDEA, WebStorm, PyCharm, GoLand, RustRover and other JetBrains IDEs. It uses
[LSP4IJ](https://github.com/redhat-developer/lsp4ij) to talk to the `mq-lsp` language server, the same one used by
the VS Code and Neovim integrations.

## Features

- Smart code completion, hover documentation, go to definition, document symbols and formatting for `.mq` files via `mq-lsp`
- Syntax highlighting (via the same TextMate grammar as the VS Code extension)
- Execute mq queries directly from the editor
- Debug `.mq` files with `mq-dbg` (Debug Adapter Protocol)
- One-click installer that downloads `mq-lsp` and `mq-dbg` from GitHub Releases

## Requirements

- A recent IntelliJ Platform IDE (2024.2+)
- The [LSP4IJ](https://plugins.jetbrains.com/plugin/23257-lsp4ij) plugin (installed automatically as a dependency)
- `mq-lsp` and `mq-dbg` — install them via the **mq > Install Servers** action, or manually:

  ```bash
  cargo install --git https://github.com/harehare/mq.git mq-lsp
  cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
  ```

## Available Actions

All actions live under the **Tools > mq** menu:

| Action                 | Description                                                    |
| ----------------------- | ---------------------------------------------------------------- |
| `New mq File`          | Create a new `.mq` file (with example queries)                 |
| `Run Selected Text`    | Run the current selection as an mq query against a chosen file |
| `Execute Query`        | Run a typed mq query against the active editor's text          |
| `Execute mq File`      | Run a chosen `.mq` file's content against the active editor    |
| `Debug Current File`   | Debug the active `.mq` file with `mq-dbg`                      |
| `Install Servers`      | Download `mq-lsp` / `mq-dbg` from GitHub Releases               |
| `Start/Stop/Restart LSP Server` | Control the `mq-lsp` language server instance          |

## Configuration

Open **Settings/Preferences > Languages & Frameworks > mq**:

- **mq-lsp path** / **mq-dbg path** — explicit executable paths (leave empty to auto-detect on `PATH`, or use the downloaded copy from **Install Servers**)
- **Show examples in new file** — toggle example queries inserted by `New mq File`
- **Enable type checking** / **Strict array mode** — passes `--enable-type-checking` / `--strict-array` to `mq-lsp`
- **Enable mq-lint diagnostics** / **Disabled lint rules** — passes `--enable-lint` / `--disable-lint-rule <id>` to `mq-lsp`

## Debugging

**mq > Debug Current File** saves the active `.mq` file, prompts for an input Markdown/MDX/HTML/CSV/TSV/text file,
and launches an `mq-dbg` Debug Adapter Protocol session (breakpoints, step, variables, call stack) using the
standard IntelliJ debugger UI. This creates a reusable **Debug Adapter Protocol** run configuration (server `mq`)
that you can also create and edit manually via **Run > Edit Configurations**.

## Building

Build with `./gradlew buildPlugin`.

## Known differences from the VS Code extension

- No CodeLens-equivalent inline "▶︎ Run Query" gutter icons yet — use the **Execute Query** / **Run Selected Text** actions instead.
- Workspace folders map to the project's base path rather than per-module content roots.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
