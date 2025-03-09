set working-directory := '.'

export RUST_BACKTRACE := "1"

run *args:
    cargo run -- {{args}}

[working-directory: 'crates/mq-lsp']
start-lsp:
    cargo watch -x run

[working-directory: 'crates/mq-lang']
bench: build-bench
    cargo codspeed run

build:
    cargo build --release --workspace

[working-directory: 'crates/mq-lang']
build-bench:
    cargo codspeed build

[working-directory: 'crates/mq-wasm']
build-wasm:
    wasm-pack build --release --target web --out-dir ../../playground/src/mq-wasm

test:
    cargo fmt --all -- --check
    cargo clippy --workspace
    cargo test --examples
    cargo test --workspace

test-cov:
    cargo llvm-cov --open --html --workspace

test-fazz:
    cargo +nightly fuzz run interpreter

[working-directory: 'crates/mq-wasm']
test-wasm:
    wasm-pack test --chrome --headless

deps:
    cargo +nightly udeps

docs:
  cargo readme --project-root crates/mq-lang --output README.md
  cargo readme --project-root crates/mq-lsp --output README.md
  cargo readme --project-root crates/mq-repl --output README.md
  cargo readme --project-root crates/mq-hir --output README.md
  cargo readme --project-root crates/mq-markdown --output README.md
  cargo readme --project-root crates/mq-formatter --output README.md
  cargo readme --project-root crates/mq-wasm --output README.md
  ./scripts/update_doc.sh
