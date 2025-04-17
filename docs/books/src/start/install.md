# Install

## Cargo

```sh
cargo install --git https://github.com/harehare/mq.git mq-cli --tag v0.1.1
```

## Binaries

You can download pre-built binaries from the [GitHub releases page](https://github.com/harehare/mq/releases):

```sh
# macOS (Intel)
curl -L https://github.com/harehare/mq/releases/download/v0.1.1/mq-x86_64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# macOS (Apple Silicon)
curl -L https://github.com/harehare/mq/releases/download/v0.1.1/mq-aarch64-apple-darwin -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux x86_64
curl -L https://github.com/harehare/mq/releases/download/v0.1.1/mq-x86_64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Linux arm64
curl -L https://github.com/harehare/mq/releases/download/v0.1.1/mq-aarch64-unknown-linux-gnu -o /usr/local/bin/mq && chmod +x /usr/local/bin/mq

# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/harehare/mq/releases/download/v0.1.1/mq-x86_64-pc-windows-msvc.exe -OutFile "$env:USERPROFILE\bin\mq.exe"
```

## Homebrew

```sh
# Using Homebrew (macOS and Linux)
$ brew install harehare/tap/mq
```

## Docker

```sh
$ docker run --rm ghcr.io/harehare/mq:0.1.0
```

## Visual Studio Code Extension

You can install the VSCode extension from the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=harehare.vscode-mq).
