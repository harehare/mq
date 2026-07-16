# mq tasks

```
mq-task -f task.md <task>
```

## run

Run the CLI with the provided arguments.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo run -- $MX_ARGS
```

## playground

Start the web playground development server.

```meta
dir = "packages/mq-playground"
env = ["RUST_BACKTRACE=1"]
```

```bash
pnpm run dev
```

## bench

Run benchmarks using codspeed.

```meta
dir = "crates/mq-lang"
depends = ["build-bench"]
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo codspeed run
```

## bench-local

Run benchmarks locally.

```meta
dir = "crates/mq-lang"
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo bench
```

## build

Build the project in release mode.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo build --release -p mq-run --bin mq
cargo build --release -p mq-run --bin mq-dbg --features="debugger"
cargo build --release -p mq-lsp -p mq-crawler -p mq-test
cargo build --release -p mq-check --features="cli"
cargo build --release -p mq-lint --features="cli"
cargo build --release -p mq-formatter
```

## build-target

Build for a specific target architecture.

```meta
params = ["target"]
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo build --release --target $MX_PARAM_TARGET -p mq-run --bin mq
cargo build --release --target $MX_PARAM_TARGET -p mq-run --bin mq-dbg --features="debugger"
cargo build --release --target $MX_PARAM_TARGET -p mq-lsp -p mq-crawler -p mq-test
cargo build --release --target $MX_PARAM_TARGET -p mq-check --features="cli"
cargo build --release --target $MX_PARAM_TARGET -p mq-lint --features="cli"
cargo build --release --target $MX_PARAM_TARGET -p mq-formatter
```

## build-bench

Build benchmarks with codspeed.

```meta
dir = "crates/mq-lang"
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo codspeed build
```

## build-wasm

Build WebAssembly package for web use.

```meta
dir = "crates/mq-wasm"
env = ["RUST_BACKTRACE=1"]
```

```bash
wasm-pack build --release --target web --out-dir ../../packages/mq-web/mq-wasm
rm ../../packages/mq-web/mq-wasm/README.md
rm ../../packages/mq-web/mq-wasm/package.json
```

## build-web

Build mq-web package.

```meta
dir = "packages/mq-web"
depends = ["build-wasm"]
env = ["RUST_BACKTRACE=1"]
```

```bash
pnpm run build
```

## build-node-wasm

Build @mqlang/node package.

```meta
dir = "crates/mq-wasm"
env = ["RUST_BACKTRACE=1"]
```

```bash
wasm-pack build --release --target nodejs --out-dir ../../packages/mq-nodejs/mq-wasm -- --no-default-features
rm ../../packages/mq-nodejs/mq-wasm/README.md
rm ../../packages/mq-nodejs/mq-wasm/package.json
```

## build-node

Build @mqlang/node package.

```meta
dir = "packages/mq-nodejs"
depends = ["build-node-wasm"]
env = ["RUST_BACKTRACE=1"]
```

```bash
pnpm run build
```

## fmt

Run formatting.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo fmt --all -- --check
```

## test-mq

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo run -p mq-test -- crates/mq-lang/builtin_tests.mq crates/mq-lang/modules/*_test.mq
```

## test-doc

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo test --doc --workspace
```

## test-all-features

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo nextest run --workspace --all-features
```

## test

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo nextest run --workspace --all-features
```

## test-all

Run formatting, linting and all tests.

```meta
depends = ["fmt", "lint", "test-mq", "test-doc", "test-all-features", "test"]
```

## test-cov

Run tests with code coverage reporting.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo llvm-cov --open --html --workspace --all-features --ignore-filename-regex 'crates/mq-(crawler|test|wasm|web-api|dap|python|lsp/src/capabilities\.rs|repl/src/repl\.rs)'
```

## test-fuzz

Run fuzzing tests.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo +nightly fuzz run interpreter
```

## test-wasm

Run WebAssembly tests in Chrome.

```meta
dir = "crates/mq-wasm"
env = ["RUST_BACKTRACE=1"]
```

```bash
wasm-pack test --chrome --headless
```

## lint

Run formatter and linter.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo clippy --all-targets --all-features --workspace -- -D clippy::all
```

## deps

Check for unused dependencies.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cargo +nightly udeps
```

## docs

Update documentation.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
./scripts/update_doc.sh
```

## bump-version

Bump version for all crates.

```meta
dir = "scripts"
env = ["RUST_BACKTRACE=1"]
```

```bash
./bump_version.sh
```

## publish

Publish crates.

```meta
env = ["RUST_BACKTRACE=1"]
```

```bash
cp -r crates/mq-run/assets crates/mq-hir
cp -r crates/mq-run/assets crates/mq-lang
cp -r crates/mq-run/assets crates/mq-markdown
cp -r crates/mq-run/assets crates/mq-repl
cargo publish --workspace
```
