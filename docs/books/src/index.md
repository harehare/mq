# Introduction

This guide will teach you about `mq`, a command-line tool for querying and transforming Markdown files using a syntax inspired by [`jq`](https://jqlang.github.io/jq/). You'll learn how to select specific elements, filter content, apply transformations, and compose these operations into powerful one-liners or reusable scripts.

Let's get started.

## Installation

The quickest way to install `mq` is via the install script:

```bash
curl -sSL https://mqlang.org/install.sh | bash
```

On macOS and Linux, you can also use Homebrew:

```bash
brew install mq
```

For other installation methods including Cargo, pre-built binaries, Docker, and more, see the [Install](start/install.md) page.

## Your First Query

Once installed, let's try a simple query. Save this file as `hello.md`:

```markdown
# Hello

Welcome to **mq**.

## Getting Started

Install it, then run your first query.

## Features

- Select headings
- Filter nodes
- Transform content
```

Now run `mq` to extract all headings:

```bash
$ mq '.h' hello.md
# Hello
## Getting Started
## Features
```

Use `to_text()` to get just the heading text:

```bash
$ mq '.h | to_text' hello.md
Hello
Getting Started
Features
```

You can narrow it down to a specific level, for example only `h2`:

```bash
$ mq '.h2 | to_text' hello.md
Getting Started
Features
```

Queries are composable with `|`, just like a Unix pipeline.

## What's Next

With the basics covered, the [Getting Started](start/index.md) section walks through installation options, syntax, and common patterns. When you're ready to look up specific behavior, the [Reference](reference/index.md) covers every selector, operator, and built-in function in detail.

