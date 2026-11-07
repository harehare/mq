```meta
env = ["RUST_BACKTRACE=1"]
```

# mq tasks

```
mq-task -f task.md <task>
```

## run

Run the CLI with the provided arguments.

```bash
cargo run -- $MX_ARGS
```

## playground

Start the web playground development server.

```meta
dir = "packages/mq-playground"
```

```bash
pnpm run dev
```

## chrome-extension

Start the Chrome extension development server.

```meta
dir = "packages/mq-chrome-extension"
```

```bash
pnpm run dev
```

## bench

Run benchmarks using codspeed.

```meta
dir = "crates/mq-lang"
depends = ["build-bench"]
```

```bash
cargo codspeed run
```

## bench-local

Run benchmarks locally.

```meta
dir = "crates/mq-lang"
```

```bash
cargo bench
```

## build

Build the project in release mode.

```bash
MQ_FORBID_DEBUGGER=1 cargo build --release -p mq-run --bin mq
cargo build --release -p mq-run --bin mq-dbg --features="debugger"
cargo build --release -p mq-lsp
cargo build --release -p mq-crawler -p mq-bench
cargo build --release -p mq-test
cargo build --release -p mq-check --features="cli"
cargo build --release -p mq-lint --features="cli"
cargo build --release -p mq-formatter
```

## build-target

Build for a specific target architecture.

```meta
params = ["target"]
```

```bash
MQ_FORBID_DEBUGGER=1 cargo build --release --target "$MX_PARAM_TARGET" -p mq-run --bin mq
cargo build --release --target "$MX_PARAM_TARGET" -p mq-run --bin mq-dbg --features="debugger"
cargo build --release --target "$MX_PARAM_TARGET" -p mq-lsp
cargo build --release --target "$MX_PARAM_TARGET" -p mq-crawler -p mq-bench
cargo build --release --target "$MX_PARAM_TARGET" -p mq-test
cargo build --release --target "$MX_PARAM_TARGET" -p mq-check --features="cli"
cargo build --release --target "$MX_PARAM_TARGET" -p mq-lint --features="cli"
cargo build --release --target "$MX_PARAM_TARGET" -p mq-formatter
```

## dump-bytecode

Dump Tarn bytecode for a query via mq-dbg, verifying debugger => debug-trace wiring (pass the query after --).

```meta
params = ["query"]
```

```bash
cargo run -p mq-run --bin mq-dbg --features="debugger" -- -C --dump-bytecode -I null "$MX_PARAM_QUERY"
```

## vm-profile

Profile production Tarn bytecode without debugger-only boundaries. The profile changes dispatch cost, so use it to find hot instructions, not to measure elapsed time (pass the query after --).

```meta
params = ["query"]
```

```bash
cargo run -p mq-run --bin mq --features="vm-profile" -- --vm-profile -I null "$MX_PARAM_QUERY"
```

## build-bench

Build benchmarks with codspeed.

```meta
dir = "crates/mq-lang"
```

```bash
cargo codspeed build
```

## build-wasm

Build WebAssembly package for web use. Skipped when no crate source changed and the output exists.

```meta
dir = "crates/mq-wasm"
sources = ["../**/*.rs", "../**/*.mq", "../**/Cargo.toml", "../../Cargo.lock"]
outputs = ["../../packages/mq-web/mq-wasm/mq_wasm_bg.wasm"]
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
if = 'which("pnpm")'
sources = ["src/**/*", "mq-wasm/*", "package.json", "tsup.config.ts", "tsconfig.json"]
outputs = ["dist/core.js"]
```

```bash
pnpm run build
```

## build-chrome-extension

Build the Chrome extension (loadable unpacked from .output/chrome-mv3).

```meta
dir = "packages/mq-chrome-extension"
if = 'which("pnpm")'
```

```bash
pnpm run build
```

## build-node-wasm

Build @mqlang/node wasm package. Skipped when no crate source changed and the output exists.

```meta
dir = "crates/mq-wasm"
sources = ["../**/*.rs", "../**/*.mq", "../**/Cargo.toml", "../../Cargo.lock"]
outputs = ["../../packages/mq-nodejs/mq-wasm/mq_wasm_bg.wasm"]
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
if = 'which("pnpm")'
sources = ["src/**/*", "mq-wasm/*", "package.json", "tsup.config.ts", "tsconfig.json"]
outputs = ["dist/index.js"]
```

```bash
pnpm run build
```

## build-packages

Build mq-web, @mqlang/node and the Chrome extension concurrently.

```meta
depends = ["build-web", "build-node", "build-chrome-extension"]
parallel = true
```

## fmt

Run formatting.

```bash
cargo fmt --all -- --check
```

## test-mq

Run bundled mq tests through the Tarn bytecode VM.

```bash
cargo run -p mq-test -- crates/mq-lang/builtin_tests.mq crates/mq-lang/modules/*_test.mq
```

## bench-mq

Time bundled mq benches through the Tarn bytecode VM.

```bash
cargo run --release -p mq-bench -- crates/mq-lang/benches/mq_benches.mq
```

## bench-check

Time mq-check per input and pipeline stage (divan).

```bash
cargo bench -p mq-check --bench benchmark
```

## bench-check-phases

Print the median time of each mq-check phase per input.

```bash
cargo run --release -p mq-check --example phase_profile
```

## test-conformance

Step through the .mq test suite and check that runtime values fit the inferred types of let bindings.

```bash
cargo test --release -p mq-check --test runtime_conformance_test -- --ignored
```

## fuzz-check

Fuzz the checker against the runtime. FUZZ_COUNT programs (default 5000), FUZZ_NOISE percent of sub-expressions with a wrong type (0 = all programs well typed), FUZZ_SEED.

```bash
cargo test --release -p mq-check --test differential_fuzz_test -- --ignored --nocapture
```

## test-gfm-spec

Check -U round-trip fidelity against the GFM spec examples (fetches spec.txt over the network).

```meta
retries = 2
retry_delay = "2s"
```

```bash
cargo test -p mq-markdown --test gfm_roundtrip_fidelity -- --ignored --nocapture
```

## test-doc

Run doctests.

```bash
cargo test --doc --workspace
```

## test-all-features

Run workspace tests with all features.

```bash
cargo nextest run --workspace --all-features
```

## test-lang-cached

Workspace dependencies enable the debugger feature, which excludes the production bytecode cache. Test mq-lang separately to cover both Rc and Arc cache implementations.

```bash
cargo nextest run -p mq-lang --lib --features mqc
cargo nextest run -p mq-lang --lib --features mqc,sync
```

## test

Run workspace tests.

```meta
depends = ["test-lang-cached"]
```

```bash
cargo nextest run --workspace
```

## check

Run formatting check and linter concurrently.

```meta
depends = ["fmt", "lint"]
parallel = true
```

## test-all

Run formatting, linting and all tests.

```meta
depends = ["check", "test-mq", "test-doc", "test-all-features", "test"]
```

## test-cov

Run tests with code coverage reporting.

```bash
cargo llvm-cov --open --html --workspace --all-features --ignore-filename-regex 'crates/mq-(crawler|test|bench|wasm|web-api|dap|python|lsp/src/capabilities\.rs|repl/src/repl\.rs)'
```

## test-fuzz

Run fuzzing tests against the Tarn bytecode VM (targets: interpreter, mqc; pass the target after --).

```meta
params = ["target=interpreter"]
```

```bash
set -eu
if cargo +nightly -V >/dev/null 2>&1; then
    cargo +nightly fuzz run "$MX_PARAM_TARGET"
elif [ -n "${FUZZ_RUSTUP_TOOLCHAIN_BIN:-}" ]; then
    PATH="$FUZZ_RUSTUP_TOOLCHAIN_BIN:$PATH" cargo-fuzz run "$MX_PARAM_TARGET"
elif command -v rustup >/dev/null 2>&1; then
    PATH="$(dirname "$(rustup which --toolchain nightly cargo)"):$PATH" cargo-fuzz run "$MX_PARAM_TARGET"
else
    echo "error: a nightly Rust toolchain is required to fuzz; run 'rustup toolchain install nightly'" >&2
    exit 1
fi
```

## test-wasm

Run WebAssembly tests in Chrome (main thread and dedicated worker).

```meta
dir = "crates/mq-wasm"
```

```bash
wasm-pack test --chrome --headless
wasm-pack test --chrome --headless -- --features test-in-worker
```

## lint

Run formatter and linter.

```bash
cargo clippy --all-targets --all-features --workspace -- -D clippy::all
```

## deps

Check for unused dependencies.

```bash
cargo +nightly udeps
```

## docs

Update documentation.

```bash
./scripts/update_doc.sh
```

## bump-version

Bump version for all crates (bump: major|minor|patch|X.Y.Z, default: patch).

```meta
dir = "scripts"
params = ["bump=patch"]
```

```bash
./bump_version.sh "$MX_PARAM_BUMP"
```

## publish

Publish crates.

```bash
cp -r crates/mq-run/assets crates/mq-hir
cp -r crates/mq-run/assets crates/mq-lang
cp -r crates/mq-run/assets crates/mq-markdown
cp -r crates/mq-run/assets crates/mq-repl
cargo publish --workspace
```
