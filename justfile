set working-directory := '.'

export RUST_BACKTRACE := "1"

# Run the CLI with the provided arguments
run *args:
    cargo run -- {{args}}

# Start the web playground development server
[working-directory: 'packages/mq-playground']
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
    cargo build --release -p mq-run --bin mq
    cargo build --release -p mq-run --bin mq-dbg --features="debugger"
    cargo build --release -p mq-lsp -p mq-crawler

# Build for a specific target architecture
build-target target:
    cargo build --release --target {{target}} -p mq-run --bin mq
    cargo build --release --target {{target}} -p mq-run --bin mq-dbg --features="debugger"
    cargo build --release --target {{target}} -p mq-lsp -p mq-crawler

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

# Run formatting
fmt:
    cargo fmt --all -- --check

test-mq:
    cargo run -p mq-run --bin mq -- -f crates/mq-lang/builtin_tests.mq
    cargo run -p mq-run --bin mq -- -f crates/mq-lang/modules/module_tests.mq

test-doc:
    cargo test --doc --workspace

test-all-features:
    cargo nextest run --workspace --all-features

# Run formatting, linting and all tests
test: fmt lint test-mq test-doc test-all-features
    cargo nextest run --workspace

# Run tests with code coverage reporting
test-cov:
    cargo llvm-cov --open --html --workspace --all-features --ignore-filename-regex 'crates/mq-(crawler|test|wasm|web-api|dap|python|lsp/src/capabilities\.rs|repl/src/repl\.rs)'

# Run fuzzing tests
test-fuzz:
    cargo +nightly fuzz run interpreter

# Run WebAssembly tests in Chrome
[working-directory: 'crates/mq-wasm']
test-wasm:
    wasm-pack test --chrome

# Run formatter and linter
lint:
    cargo clippy  --all-targets --all-features --workspace -- -D clippy::all

# Check for unused dependencies
deps:
    cargo +nightly udeps

# Update documentation
docs:
  ./scripts/update_doc.sh

# Bump version for all crates
bump-version:
    cd scripts && ./bump_version.sh

# Publish crates
publish:
    cp -r crates/mq-run/assets crates/mq-hir
    cp -r crates/mq-run/assets crates/mq-lang
    cp -r crates/mq-run/assets crates/mq-markdown
    cp -r crates/mq-run/assets crates/mq-repl
    cargo publish --workspace
