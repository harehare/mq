set working-directory := '.'

export RUST_BACKTRACE := "1"

run *args:
    cargo run -- {{args}}

[working-directory: 'crates/mq-lsp']
start-lsp:
    cargo watch -x run

bench:
    cargo bench -p mq-lang

build:
    cargo build --release --workspace

[working-directory: 'crates/mq-wasm']
build-wasm:
    wasm-pack build --release --target web --out-dir ../../playground/src/mq-wasm

[working-directory: 'crates/mq-wasm']
test-wasm:
    wasm-pack test --chrome --headless

test-fazz:
    cargo +nightly fuzz run interpreter

test:
    cargo fmt --all -- --check
    cargo clippy --workspace
    cargo test --workspace

deps:
    cargo +nightly udeps

readme:
  cargo readme --project-root crates/mq-lang --output README.md
  cargo readme --project-root crates/mq-lsp --output README.md
  cargo readme --project-root crates/mq-repl --output README.md
  cargo readme --project-root crates/mq-hir --output README.md
  cargo readme --project-root crates/mq-md --output README.md
  cargo readme --project-root crates/mq-formatter --output README.md
  cargo readme --project-root crates/mq-wasm --output README.md
