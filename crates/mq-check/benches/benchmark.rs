//! Type checker benchmarks, per input and per pipeline stage.
//!
//! Per-phase times inside `check` come from `examples/phase_profile.rs` (`just bench-check-phases`).

mod inputs;

use inputs::{INPUTS, input, is_builtin_source};
use mq_check::TypeChecker;
use mq_hir::Hir;
use url::Url;

fn main() {
    divan::main();
}

fn url() -> Url {
    Url::parse("file:///bench.mq").unwrap()
}

fn hir_for(name: &str, code: &str) -> Hir {
    let mut hir = Hir::default();
    hir.builtin.disabled = is_builtin_source(name);
    hir.add_code(Some(url()), code);
    hir
}

#[divan::bench(args = INPUTS)]
fn parse(bencher: divan::Bencher, name: &str) {
    let code = input(name);
    bencher.bench_local(|| mq_lang::parse_recovery(&code));
}

/// Fresh `Hir` including builtin.mq. For `builtin` the builtins are disabled instead.
#[divan::bench(args = INPUTS, sample_count = 5)]
fn hir_cold(bencher: divan::Bencher, name: &str) {
    let code = input(name);
    bencher.bench_local(|| hir_for(name, &code));
}

/// Cost of loading builtin.mq alone.
#[divan::bench]
fn hir_builtin_only() -> Hir {
    let mut hir = Hir::default();
    hir.add_code(None, "");
    hir
}

#[divan::bench(args = INPUTS, sample_count = 5)]
fn check(bencher: divan::Bencher, name: &str) {
    let code = input(name);
    let hir = hir_for(name, &code);
    bencher.bench_local(|| TypeChecker::new().check(&hir));
}

fn elif_chain(n: usize) -> String {
    let mut code = String::from("def f(a): if (a): 1");
    for i in 0..n {
        code.push_str(&format!(" elif (a == {i}): {i}"));
    }
    code.push_str(" else: 0;");
    code
}

/// Long elif chains must stay near-linear (was cubic: 4000 branches took 14s).
#[divan::bench(args = [1000, 2000, 4000], sample_count = 5)]
fn check_elif_chain(bencher: divan::Bencher, n: usize) {
    let code = elif_chain(n);
    let hir = hir_for("elif", &code);
    bencher.bench_local(|| TypeChecker::new().check(&hir));
}
