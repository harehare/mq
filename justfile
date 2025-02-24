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


test:
    cargo fmt --all -- --check
    cargo clippy --workspace
    cargo test --workspace

readme:
  cargo readme --project-root crates/mq-lang --output README.md
  cargo readme --project-root crates/mq-lsp --output README.md
  cargo readme --project-root crates/mq-repl --output README.md
  cargo readme --project-root crates/mq-hir --output README.md
  cargo readme --project-root crates/mq-md --output README.md
  cargo readme --project-root crates/mq-formatter --output README.md
