set working-directory := '.'

export RUST_BACKTRACE := "1"

run *args:
    cargo run -- {{args}}

[working-directory: 'crates/mdq-lsp']
start-lsp:
    cargo watch -x run

bench:
    cargo bench -p mdq-lang

build:
    cargo build --release --workspace


test:
    cargo fmt --all -- --check
    cargo clippy --workspace
    cargo test --workspace

readme:
  cargo readme --project-root crates/mdq-lang --output README.md
  cargo readme --project-root crates/mdq-lsp --output README.md
  cargo readme --project-root crates/mdq-repl --output README.md
  cargo readme --project-root crates/mdq-hir --output README.md
  cargo readme --project-root crates/mdq-md --output README.md
  cargo readme --project-root crates/mdq-formatter --output README.md
