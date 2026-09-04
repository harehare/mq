set working-directory := '.'

export RUST_BACKTRACE := "1"

# Run the CLI with the provided arguments
run *args:
    cargo run -- {{args}}

# Start the web playground development server
[working-directory: 'packages/mq-playground']
playground:
    pnpm run dev

# Start the Chrome extension development server
[working-directory: 'packages/mq-chrome-extension']
chrome-extension:
    pnpm run dev

# Run benchmarks using codspeed
[working-directory: 'crates/mq-lang']
bench: build-bench
    cargo codspeed run

# Run benchmarks locally
[working-directory: 'crates/mq-lang']
bench-local:
    cargo bench

# Run the shared mq-lang benchmark suite on the tree-walking evaluator.
bench-tree:
    cargo bench -p mq-lang --bench benchmark

# Run the same mq-lang benchmark suite on Tarn. `tarn` routes Engine::eval to the VM.
bench-vm:
    cargo bench -p mq-lang --bench benchmark --features tarn

# Run the same shared benchmark on the tree-walker and VM in sequence.
# Example: just bench-compare eval_compiled_fibonacci
bench-compare filter:
    cargo bench -p mq-lang --bench benchmark {{filter}}
    cargo bench -p mq-lang --bench benchmark --features tarn {{filter}}

# Build the project in release mode
build:
    cargo build --release -p mq-run --bin mq
    cargo build --release -p mq-run --bin mq-dbg --features="debugger"
    cargo build --release -p mq-lsp -p mq-crawler -p mq-test
    cargo build --release -p mq-check --features="cli"
    cargo build --release -p mq-lint --features="cli"
    cargo build --release -p mq-formatter

# Build for a specific target architecture
build-target target:
    cargo build --release --target {{target}} -p mq-run --bin mq
    cargo build --release --target {{target}} -p mq-run --bin mq-dbg --features="debugger"
    cargo build --release --target {{target}} -p mq-lsp -p mq-crawler -p mq-test
    cargo build --release --target {{target}} -p mq-check --features="cli"
    cargo build --release --target {{target}} -p mq-lint --features="cli"
    cargo build --release --target {{target}} -p mq-formatter

# Dumps Tarn bytecode for a query via mq-dbg, verifying debugger => debug-trace wiring.
# Example: just dump-bytecode '1 + 2'
dump-bytecode query:
    cargo run -p mq-run --bin mq-dbg --features="debugger" -- --dump-bytecode -I null '{{query}}'

# Build benchmarks with codspeed. Runs against the tarn VM backend, not the tree-walker.
[working-directory: 'crates/mq-lang']
build-bench:
    cargo codspeed build --features tarn

# Build WebAssembly package for web use
[working-directory: 'crates/mq-wasm']
build-wasm:
    wasm-pack build --release --target web --out-dir ../../packages/mq-web/mq-wasm
    rm ../../packages/mq-web/mq-wasm/README.md
    rm ../../packages/mq-web/mq-wasm/package.json

# Build mq-web package
[working-directory: 'packages/mq-web']
build-web: build-wasm
    pnpm run build

# Build the Chrome extension (loadable unpacked from .output/chrome-mv3)
[working-directory: 'packages/mq-chrome-extension']
build-chrome-extension:
    pnpm run build

# Build @mqlang/node package
[working-directory: 'crates/mq-wasm']
build-node-wasm:
    wasm-pack build --release --target nodejs --out-dir ../../packages/mq-nodejs/mq-wasm -- --no-default-features
    rm ../../packages/mq-nodejs/mq-wasm/README.md
    rm ../../packages/mq-nodejs/mq-wasm/package.json

# Build @mqlang/node package
[working-directory: 'packages/mq-nodejs']
build-node: build-node-wasm
    pnpm run build

# Run formatting
fmt:
    cargo fmt --all -- --check

# Run bundled mq tests through the tree-walking evaluator.
test-mq-tree:
    cargo run -p mq-test -- crates/mq-lang/builtin_tests.mq crates/mq-lang/modules/*_test.mq

# Run the identical bundled mq tests through Tarn.
test-mq-vm:
    cargo run -p mq-test --features tarn -- crates/mq-lang/builtin_tests.mq crates/mq-lang/modules/*_test.mq

# Keep both execution engines as a required validation gate until cutover.
test-mq: test-mq-tree test-mq-vm

# Check -U round-trip fidelity against the GFM spec examples (fetches spec.txt over the network)
test-gfm-spec:
    cargo test -p mq-markdown --test gfm_roundtrip_fidelity -- --ignored --nocapture

test-doc:
    cargo test --doc --workspace

test-all-features:
    cargo nextest run --workspace --all-features

test:
    cargo nextest run --workspace --all-features

# Run formatting, linting and all tests
test-all: fmt lint test-mq test-doc test-all-features test

# Run tests with code coverage reporting
test-cov:
    cargo llvm-cov --open --html --workspace --all-features --ignore-filename-regex 'crates/mq-(crawler|test|wasm|web-api|dap|python|lsp/src/capabilities\.rs|repl/src/repl\.rs)'

# Run fuzzing tests against the tree-walking evaluator
test-fuzz:
    cargo +nightly fuzz run interpreter

# Run fuzzing tests against the tarn bytecode VM
test-fuzz-tarn:
    cargo +nightly fuzz run tarn --features tarn

# Run WebAssembly tests in Chrome
[working-directory: 'crates/mq-wasm']
test-wasm:
    wasm-pack test --chrome --headless

# Run formatter and linter
lint:
    cargo clippy  --all-targets --all-features --workspace -- -D clippy::all

# Check for unused dependencies
deps:
    cargo +nightly udeps

# Update documentation
docs:
  ./scripts/update_doc.sh

# Bump version for all crates (bump: major|minor|patch|X.Y.Z, default: patch)
bump-version bump="patch":
    cd scripts && ./bump_version.sh {{bump}}

# Publish crates
publish:
    cp -r crates/mq-run/assets crates/mq-hir
    cp -r crates/mq-run/assets crates/mq-lang
    cp -r crates/mq-run/assets crates/mq-markdown
    cp -r crates/mq-run/assets crates/mq-repl
    cargo publish --workspace
