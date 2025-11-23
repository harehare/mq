# Install

## Quick Install

```bash
curl -sSL https://mqlang.org/install.sh | bash
# Install the debugger
curl -sSL https://mqlang.org/install.sh | bash -s -- --with-debug
```

The installer will:
- Download the latest mq binary for your platform
- Install it to `~/.mq/bin/`
- Update your shell profile to add mq to your PATH

## Cargo

```sh
cargo install --git https://github.com/harehare/mq.git mq-run --tag v0.5.2
# Latest Development Version
$ cargo install --git https://github.com/harehare/mq.git mq-run --bin mq
# Install the debugger
$ cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
```

## Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Intel)
curl -L https://github.com/harehare/mq/releases/download/v0.5.2/mq-x86_64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# macOS (Apple Silicon)
curl -L https://github.com/harehare/mq/releases/download/v0.5.2/mq-aarch64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux x86_64
curl -L https://github.com/harehare/mq/releases/download/v0.5.2/mq-x86_64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux arm64
curl -L https://github.com/harehare/mq/releases/download/v0.5.2/mq-aarch64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.5.2/mq-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq.exe"
```

## Homebrew

```sh
# Using Homebrew (macOS and Linux)
$ brew install harehare/tap/mq
```

## Docker

```sh
$ docker run --rm ghcr.io/harehare/mq:0.5.2
```

## Visual Studio Code Extension

You can install the VSCode extension from the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq).

### GitHub Actions

You can use mq in your GitHub Actions workflows with the [Setup mq](https://github.com/marketplace/actions/setup-mq) action:

```yaml
steps:
  - uses: actions/checkout@v4
  - uses: harehare/setup-mq@v1
  - run: mq '.code' README.md
```

## MCP (Model Context Protocol) server

mq supports an MCP server for integration with LLM applications.

See the [MCP documentation](https://github.com/harehare/mq-mcp/blob/main/README.md) for more information.

## Python

You can use mq in Python through the [`markdown-query`](https://pypi.org/project/markdown-query/) package:

```sh
# Install from PyPI
$ pip install markdown-query
```

## npm

You can use mq in npm through the [`mq-web`](https://www.npmjs.com/package/mq-web) package:

```sh
$ npm i mq-web
```

## Web crawler

```sh
# Using Homebrew (macOS and Linux)
$ brew install harehare/tap/mqcr
```
