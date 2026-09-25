//! Regression benchmarks for the CST parser (`mq_lang::CstParser`),
//! used by the formatter, LSP, and other tooling.
//!
//! This is a separate binary from `benchmark.rs` because it exercises the CST
//! parser specifically (`mq_lang::parse` in `benchmark.rs` goes through the
//! separate AST parser and never touches this code path).

use std::sync::LazyLock;

// Keep allocator behavior consistent with the `mq` CLI.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    divan::main();
}

#[divan::bench]
fn cst_parse_fibonacci() {
    let (nodes, _) = mq_lang::parse_recovery(
        "
     def fibonacci(x):
      if (x < 2):
        x
      else:
        fibonacci(x - 1) + fibonacci(x - 2); | fibonacci(20)",
    );
    debug_assert!(!nodes.is_empty());
}

/// A source with many independent top-level pipe statements, representative of a
/// typical multi-line `.mq` query.
#[divan::bench]
fn cst_parse_medium_program() {
    static CODE: LazyLock<String> = LazyLock::new(|| {
        (0..256)
            .map(|i| format!("upcase() | downcase() | ltrim() | trim(\"{i}\")"))
            .collect::<Vec<_>>()
            .join("\n")
    });
    let (nodes, _) = mq_lang::parse_recovery(&CODE);
    debug_assert!(!nodes.is_empty());
}

/// A single deeply-nested `def` body, exercising recursive `parse_program` calls
/// and long binary-operator chains.
#[divan::bench]
fn cst_parse_large_program() {
    static CODE: LazyLock<String> = LazyLock::new(|| {
        let mut defs = String::new();
        for i in 0..512 {
            defs.push_str(&format!("def f{i}(x): x + {i} end\n"));
        }
        defs.push_str("| f0(1)");
        defs
    });
    let (nodes, _) = mq_lang::parse_recovery(&CODE);
    debug_assert!(!nodes.is_empty());
}
