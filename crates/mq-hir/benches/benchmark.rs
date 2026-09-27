//! HIR construction and resolution benchmarks.

use mq_hir::Hir;
use url::Url;

fn main() {
    divan::main();
}

fn user_code() -> String {
    let mut code = String::new();
    for i in 0..128 {
        code.push_str(&format!(
            "def f{i}(x, y):\n  let z = x + y\n  | if (z > {i}): upcase(to_string(z)) else: downcase(\"{i}\");\n"
        ));
    }
    code.push_str("| f0(1, 2) | map([1, 2, 3], fn(v): v + 1;)");
    code
}

/// Fresh `Hir`, including the builtin module.
#[divan::bench]
fn hir_add_code_cold(bencher: divan::Bencher) {
    let code = user_code();
    bencher.bench_local(|| {
        let mut hir = Hir::default();
        hir.add_code(Some(Url::parse("file:///bench.mq").unwrap()), &code)
    });
}

/// Re-adding a source, as on an LSP edit.
#[divan::bench]
fn hir_add_nodes_on_edit(bencher: divan::Bencher) {
    let code = user_code();
    let url = Url::parse("file:///bench.mq").unwrap();
    let (nodes, _) = mq_lang::parse_recovery(&code);
    let mut hir = Hir::default();
    hir.add_nodes(url.clone(), &nodes);
    bencher.bench_local(|| hir.add_nodes(url.clone(), &nodes));
}

#[divan::bench]
fn hir_find_symbol_in_position(bencher: divan::Bencher) {
    let code = user_code();
    let mut hir = Hir::default();
    let (source_id, _) = hir.add_code(Some(Url::parse("file:///bench.mq").unwrap()), &code);
    let position = mq_lang::Position { line: 120, column: 10 };
    bencher.bench_local(|| hir.find_symbol_in_position(source_id, position));
}
