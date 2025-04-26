# Development

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [just](https://github.com/casey/just) - a command runner
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) (optional, for WebAssembly support)

## Setting up the development environment

Clone the repository:

```sh
git clone https://github.com/harehare/mq.git
cd mq
```

Install development dependencies:

```sh
# Using cargo
cargo install just wasm-pack
```

Or if you prefer using asdf:

```sh
# Using asdf
asdf install
```

## Common development tasks

Here are some useful commands to help you during development:

```sh
# Run the CLI with the provided arguments
just run '.code'

# Run formatting, linting and all tests
just test

# Run formatter and linter
just lint

# Build the project in release mode
just build

# Update documentation
just docs
```

Check the `just --list` for more available commands and build options.
