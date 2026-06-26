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

Or if you prefer using Nix (see [Using Nix](#using-nix) below for details):

```sh
direnv allow  # or: nix develop
```

## Using Nix

If you have [Nix](https://nixos.org/) with flakes enabled, the repository includes a `flake.nix` that provides a fully reproducible development environment with Rust (matching `rust-toolchain.toml`), `just`, `wasm-pack`, `rust-analyzer`, and all other required tools.

The repository also includes a `.envrc` file, so if you have [direnv](https://direnv.net/) installed, run the following once and the environment will be activated automatically whenever you enter the directory:

```sh
direnv allow
```

If you prefer not to use direnv, you can enter the shell manually:

```sh
nix develop
```

## Common development tasks

Here are some useful commands to help you during development:

```sh
# Run the CLI with the provided arguments
just run '.code'

# Run formatting, linting and all tests
just test-all

# Run formatter and linter
just lint

# Build the project in release mode
just build

# Update documentation
just docs
```

Check the `just --list` for more available commands and build options.
