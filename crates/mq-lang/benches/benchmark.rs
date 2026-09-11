//! Regression benchmarks for the parser and compiled Tarn VM.
//!
//! Each benchmark protects a distinct execution path. Exploratory, cold-start, and overlapping
//! microbenchmarks belong in ad-hoc profiling rather than this always-run suite.

use mq_lang::{Shared, SharedCell};
use std::sync::LazyLock;

// Keep allocator behavior consistent with the `mq` CLI. This is deliberately
// defined in the benchmark binary so library consumers retain control of their
// allocator choice.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    divan::main();
}

/// Measures steady-state execution after parsing and optimization on the selected engine.
fn bench_compiled<F>(bencher: divan::Bencher, engine: &mut mq_lang::DefaultEngine, code: &str, mut input: F)
where
    F: FnMut() -> Vec<mq_lang::RuntimeValue>,
{
    let compiled = engine.compile(code).unwrap();
    engine.eval_compiled(&compiled, input().into_iter()).unwrap();
    bencher.bench_local(|| engine.eval_compiled(&compiled, input().into_iter()).unwrap());
}

#[divan::bench]
fn eval_compiled_fibonacci(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    bench_compiled(
        bencher,
        &mut engine,
        "
     def fibonacci(x):
      if (x < 2):
        x
      else:
        fibonacci(x - 1) + fibonacci(x - 2); | fibonacci(20)",
        || vec![mq_lang::RuntimeValue::Number(20.into())],
    );
}

#[divan::bench]
fn eval_compiled_while(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    bench_compiled(
        bencher,
        &mut engine,
        "var i = 10000 | while(i > 0): i -= 1; | i",
        || vec![mq_lang::RuntimeValue::Number(1.into())],
    );
}

/// Measures the VM's specialized `foreach` control path with per-iteration arithmetic.
///
/// Keep this workload aligned with the long-standing regression benchmark so historical
/// measurements remain comparable.
#[divan::bench]
fn eval_compiled_foreach(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    bench_compiled(bencher, &mut engine, "foreach(x, range(0, 1000, 1)): x + 1;", || {
        vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))]
    });
}

/// Measures the API pattern used by line-oriented callers: one compiled query evaluated once
/// per input value while file globals remain stable.
#[divan::bench]
fn eval_compiled_reused_single_input_with_globals(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.define_string_value("__FILE__", "input.md");
    engine.define_string_value("__FILE_NAME__", "input.md");
    engine.define_string_value("__FILE_STEM__", "input");
    let compiled = engine.compile("__FILE__").unwrap();
    engine
        .eval_compiled(&compiled, std::iter::once(mq_lang::RuntimeValue::None))
        .unwrap();

    bencher.bench_local(|| {
        engine
            .eval_compiled(&compiled, std::iter::once(mq_lang::RuntimeValue::None))
            .unwrap()
    });
}

#[divan::bench]
fn eval_compiled_array_map(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    bench_compiled(
        bencher,
        &mut engine,
        r#"range(0, 1000, 1) | map(fn(x): x * 2;)"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

#[divan::bench]
fn eval_compiled_array_filter(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    bench_compiled(
        bencher,
        &mut engine,
        r#"range(0, 1000, 1) | filter(fn(x): x % 2 == 0;)"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

#[divan::bench]
fn eval_compiled_array_fold(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    bench_compiled(
        bencher,
        &mut engine,
        r#"def sum(acc, x): add(acc, x); | fold(range(0, 100, 1), 0, sum)"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

#[divan::bench]
fn eval_compiled_array_chained_operations(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    bench_compiled(
        bencher,
        &mut engine,
        r#"range(0, 500, 1) | filter(fn(x): x % 2 == 0;) | map(fn(x): x * 3;) | filter(fn(x): x > 100;)"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

/// Isolates repeated non-capturing user-function calls after bytecode compilation.
#[divan::bench]
fn eval_compiled_function_call_overhead(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    bench_compiled(
        bencher,
        &mut engine,
        r#"def identity(x): x; | foreach(i, range(0, 1000, 1)): identity(i);"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

/// Measures calls through a local holding a native function, which use the VM's generic call
/// path instead of the fixed-arity closure fast path.
#[divan::bench]
fn eval_compiled_dynamic_builtin_call(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    bench_compiled(
        bencher,
        &mut engine,
        r#"let transform = upcase | foreach(i, range(0, 1000, 1)): transform("value");"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

/// Tracks the direct `CallBuiltin` path for the one- and two-argument forms used in tight loops.
#[divan::bench]
fn eval_compiled_direct_builtin_calls(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    bench_compiled(
        bencher,
        &mut engine,
        r#"foreach(i, range(0, 1000, 1)): contains(upcase("value"), "A");"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

/// Isolates nested call-frame setup and teardown after bytecode compilation.
#[divan::bench]
fn eval_compiled_nested_function_calls(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    bench_compiled(
        bencher,
        &mut engine,
        r#"def add1(x): x + 1; | def add2(x): add1(add1(x)); | def add4(x): add2(add2(x));
            | foreach(i, range(0, 100, 1)): add4(i);"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

/// Isolates repeated `get()` lookups on a small dict, the common `set()`/`get()` shape.
#[divan::bench]
fn eval_compiled_dict_field_access(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    bench_compiled(
        bencher,
        &mut engine,
        r#"let obj = dict()
        | let obj = set(obj, "a", 1) | let obj = set(obj, "b", 2) | let obj = set(obj, "c", 3)
        | let obj = set(obj, "d", 4) | let obj = set(obj, "e", 5)
        | foreach(i, range(0, 1000, 1)): add(add(add(add(get(obj, "a"), get(obj, "b")), get(obj, "c")), get(obj, "d")), get(obj, "e"));"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

#[divan::bench]
fn eval_compiled_large_dict_field_access(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    bench_compiled(
        bencher,
        &mut engine,
        r#"let d = fold(range(0, 100, 1), dict(), fn(acc, i): set(acc, to_string(i), i);)
        | foreach(i, range(0, 2000, 1)): get(d, to_string(i % 100));"#,
        || vec![mq_lang::RuntimeValue::String(Shared::new(String::new()))],
    );
}

fn owned_markdown_tree() -> mq_lang::RuntimeValue {
    mq_lang::RuntimeValue::new_markdown(mq_markdown::Node::Fragment(mq_markdown::Fragment {
        values: (0..1_000)
            .map(|index| {
                mq_markdown::Node::Text(mq_markdown::Text {
                    value: index.to_string(),
                    position: None,
                })
            })
            .collect(),
    }))
}

/// A matching tree walk catches unnecessary copies when the VM returns each source node.
#[divan::bench]
fn eval_compiled_owned_markdown_tree(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    let compiled = engine.compile(".").unwrap();
    engine
        .eval_compiled(&compiled, std::iter::once(owned_markdown_tree()))
        .unwrap();

    bencher.bench_local(|| {
        engine
            .eval_compiled(&compiled, std::iter::once(owned_markdown_tree()))
            .unwrap()
    });
}

/// A rejecting tree walk catches copies in the child-preserving fallback path.
#[divan::bench]
fn eval_compiled_owned_markdown_tree_without_matches(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    let compiled = engine.compile(".h1").unwrap();
    engine
        .eval_compiled(&compiled, std::iter::once(owned_markdown_tree()))
        .unwrap();

    bencher.bench_local(|| {
        engine
            .eval_compiled(&compiled, std::iter::once(owned_markdown_tree()))
            .unwrap()
    });
}

/// Covers selector dispatch plus the `nodes` module's tree traversal and a native builtin.
#[divan::bench]
fn eval_compiled_nodes(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    bench_compiled(bencher, &mut engine, ".h | nodes | map(upcase)", || {
        mq_markdown::Markdown::from_markdown_str("# heading\n- item1\n- item2\n## heading2\n- item1\n- item2\n")
            .unwrap()
            .nodes
            .into_iter()
            .map(mq_lang::RuntimeValue::from)
            .collect()
    });
}

const CSV_PARSE_INPUT: &str =
    "a,b,c\n\"1,2\",\"2,3\",\"3,4\"\n4,5,6\n\"multi\nline\",7,8\n9,10,\"quoted,comma\"\n\"\",11,12\n13,14,15\n";

/// Reuses optimized bytecode and the standard CSV module for one input record.
#[divan::bench]
fn eval_compiled_csv_parse(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.load_module("csv").unwrap();
    bench_compiled(bencher, &mut engine, "csv_parse(true)", || {
        vec![mq_lang::RuntimeValue::String(Shared::new(CSV_PARSE_INPUT.to_string()))]
    });
}

fn section_markdown_input() -> impl Iterator<Item = mq_lang::RuntimeValue> {
    let markdown_content = (0..30)
        .map(|i| {
            format!(
                "# Section {i}\n\nIntro paragraph for section {i}.\n\n## Subsection {i}\n\nSome detail text.\n\n- point a\n- point b\n\n"
            )
        })
        .collect::<String>();
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(&markdown_content).unwrap();
    markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from)
}

/// Covers a document-scale Markdown query through the cached standard `section` module.
#[divan::bench]
fn eval_compiled_section_sections(bencher: divan::Bencher) {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.load_module("section").unwrap();
    bench_compiled(bencher, &mut engine, "nodes | sections() | len()", || {
        section_markdown_input().collect()
    });
}

#[divan::bench]
fn parse_fibonacci() -> Vec<Shared<mq_lang::AstNode>> {
    let token_arena = Shared::new(SharedCell::new(mq_lang::Arena::new(100)));
    mq_lang::parse(
        "
     def fibonacci(x):
      if (x == 0):
        0
      elif (x == 1):
        1
      else:
        fibonacci(sub(x, 1)) + fibonacci(sub(x, 2)); | fibonacci(20)",
        Shared::clone(&token_arena),
    )
    .unwrap()
}

/// Exercises byte-string lexing without charging construction of the input to the parser.
#[divan::bench]
fn parse_large_byte_string() -> Vec<Shared<mq_lang::AstNode>> {
    static CODE: LazyLock<String> = LazyLock::new(|| format!(r#"b"{}""#, "a".repeat(16 * 1024)));
    let token_arena = Shared::new(SharedCell::new(mq_lang::Arena::new(4)));
    mq_lang::parse(&CODE, token_arena).unwrap()
}

/// Exercises flattening of a long logical-expression chain during AST construction.
#[divan::bench]
fn parse_long_and_chain() -> Vec<Shared<mq_lang::AstNode>> {
    static CODE: LazyLock<String> =
        LazyLock::new(|| std::iter::repeat_n("true", 4_096).collect::<Vec<_>>().join(" && "));
    let token_arena = Shared::new(SharedCell::new(mq_lang::Arena::new(8_192)));
    mq_lang::parse(&CODE, token_arena).unwrap()
}
