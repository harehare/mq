set working-directory := '.'

export RUST_BACKTRACE := "1"

# Run the CLI with the provided arguments
run *args:
    cargo run -- {{args}}

# Start the Language Server in watch mode
[working-directory: 'crates/mq-lsp']
start-lsp:
    cargo watch -x run

# Start the MCP server in watch mode
[working-directory: 'crates/mq-mcp']
start-mcp:
    cargo watch -x run

# Start the web playground development server
[working-directory: 'packages/playground']
playground:
    npm run dev

# Run benchmarks using codspeed
[working-directory: 'crates/mq-lang']
bench: build-bench
    cargo codspeed run

# Run benchmarks locally
[working-directory: 'crates/mq-lang']
bench-local:
    cargo bench

# Build the project in release mode
build:
    cargo build --release -p mq-cli -p mq-mcp -p mq-lsp

# Build for a specific target architecture
build-target target:
    cargo build --release --target {{target}} -p mq-cli -p mq-mcp -p mq-lsp

# Build benchmarks with codspeed
[working-directory: 'crates/mq-lang']
build-bench:
    cargo codspeed build

# Build WebAssembly package for web use
[working-directory: 'crates/mq-wasm']
build-wasm:
    wasm-pack build --release --target web --out-dir ../../packages/mq-web/mq-wasm
    rm ../../packages/mq-web/mq-wasm/README.md
    rm ../../packages/mq-web/mq-wasm/package.json

# Build mq-web package
[working-directory: 'packages/mq-web']
build-web: build-wasm
    npm run build

# Build mq-python package for Python
[working-directory: 'crates/mq-python']
build-python:
    rm ../../target/wheels/* || true
    maturin build --release --target aarch64-unknown-linux-gnu --zig
    maturin build --release --target x86_64-unknown-linux-gnu --zig
    maturin build --release --target aarch64-apple-darwin --zig
    maturin build --release --target x86_64-apple-darwin --zig
    maturin build --release --target x86_64-pc-windows-gnu --zig --features pyo3/generate-import-lib

# Publish test mq-python package for Python
[working-directory: 'crates/mq-python']
publish-python-test: build-python
    twine upload --repository testpypi ../../target/wheels/*

# Publish mq-python package for Python
[working-directory: 'crates/mq-python']
publish-python: build-python
    twine upload --repository pypi ../../target/wheels/*

# Run formatting, linting and all tests
test: lint
    cargo test --examples
    cargo test --workspace --all-features

# Run tests with code coverage reporting
test-cov:
    cargo llvm-cov --open --html --workspace --all-features

# Run fuzzing tests
test-fuzz:
    cargo +nightly fuzz run interpreter

# Run WebAssembly tests in Chrome
[working-directory: 'crates/mq-wasm']
test-wasm:
    wasm-pack test --chrome

# Run formatter and linter
lint:
    cargo fmt --all -- --check
    cargo clippy  --all-targets --all-features --workspace -- -D clippy::all

# Check for unused dependencies
deps:
    cargo +nightly udeps

# Update documentation
docs:
  cargo readme --project-root crates/mq-lang --output README.md
  cargo readme --project-root crates/mq-lsp --output README.md
  cargo readme --project-root crates/mq-repl --output README.md
  cargo readme --project-root crates/mq-hir --output README.md
  cargo readme --project-root crates/mq-markdown --output README.md
  cargo readme --project-root crates/mq-formatter --output README.md
  cargo readme --project-root crates/mq-wasm --output README.md
  ./scripts/update_doc.sh
