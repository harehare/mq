//! OpTree vs Recursive Evaluator Performance Comparison Benchmarks
//!
//! This benchmark suite compares the performance of the OpTree-based evaluator
//! against the traditional recursive AST evaluator across various workloads.

fn main() {
    divan::main();
}

// ============================================================================
// Small File Benchmarks (< 1KB)
// ============================================================================

/// Benchmark: Small fibonacci computation with OpTree
#[divan::bench(name = "small_fibonacci_optree")]
fn small_fibonacci_optree() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine
        .eval(
            "def fib(n): if (n < 2): n else: fib(n - 1) + fib(n - 2); | fib(15)",
            vec![mq_lang::RuntimeValue::Number(15.into())].into_iter(),
        )
        .unwrap()
}

/// Benchmark: Small fibonacci computation with recursive evaluator
#[divan::bench(name = "small_fibonacci_recursive")]
fn small_fibonacci_recursive() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine
        .eval(
            "def fib(n): if (n < 2): n else: fib(n - 1) + fib(n - 2); | fib(15)",
            vec![mq_lang::RuntimeValue::Number(15.into())].into_iter(),
        )
        .unwrap()
}

/// Benchmark: Small markdown selector with OpTree
#[divan::bench(name = "small_markdown_select_optree")]
fn small_markdown_select_optree() -> mq_lang::RuntimeValues {
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str("# H1\n## H2\n### H3\n").unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine.eval(".h", input).unwrap()
}

/// Benchmark: Small markdown selector with recursive evaluator
#[divan::bench(name = "small_markdown_select_recursive")]
fn small_markdown_select_recursive() -> mq_lang::RuntimeValues {
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str("# H1\n## H2\n### H3\n").unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine.eval(".h", input).unwrap()
}

// ============================================================================
// Medium File Benchmarks (1KB - 100KB)
// ============================================================================

/// Benchmark: Medium loop iteration with OpTree
#[divan::bench(name = "medium_loop_optree")]
fn medium_loop_optree() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine.load_builtin_module();
    engine
        .eval(
            "foreach(x, range(0, 1000, 1)): x + 1;",
            vec![mq_lang::RuntimeValue::Number(0.into())].into_iter(),
        )
        .unwrap()
}

/// Benchmark: Medium loop iteration with recursive evaluator
#[divan::bench(name = "medium_loop_recursive")]
fn medium_loop_recursive() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine.load_builtin_module();
    engine
        .eval(
            "foreach(x, range(0, 1000, 1)): x + 1;",
            vec![mq_lang::RuntimeValue::Number(0.into())].into_iter(),
        )
        .unwrap()
}

/// Benchmark: Medium markdown processing with OpTree
#[divan::bench(name = "medium_markdown_optree")]
fn medium_markdown_optree() -> mq_lang::RuntimeValues {
    // Generate markdown with ~50 headings
    let mut md = String::new();
    for i in 1..=50 {
        md.push_str(&format!("# Heading {}\n- item1\n- item2\n", i));
    }

    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(&md).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine.load_builtin_module();
    engine.eval(".h | nodes | map(upcase)", input).unwrap()
}

/// Benchmark: Medium markdown processing with recursive evaluator
#[divan::bench(name = "medium_markdown_recursive")]
fn medium_markdown_recursive() -> mq_lang::RuntimeValues {
    // Generate markdown with ~50 headings
    let mut md = String::new();
    for i in 1..=50 {
        md.push_str(&format!("# Heading {}\n- item1\n- item2\n", i));
    }

    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(&md).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine.load_builtin_module();
    engine.eval(".h | nodes | map(upcase)", input).unwrap()
}

// ============================================================================
// Large File Benchmarks (> 100KB)
// ============================================================================

/// Benchmark: Large markdown document with OpTree
#[divan::bench(name = "large_markdown_optree")]
fn large_markdown_optree() -> mq_lang::RuntimeValues {
    // Generate markdown with ~500 headings (~100KB+)
    let mut md = String::new();
    for i in 1..=500 {
        md.push_str(&format!(
            "# Heading {}\n\nThis is paragraph {} with some content.\n\n- item1\n- item2\n- item3\n\n",
            i, i
        ));
    }

    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(&md).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine.load_builtin_module();
    engine.eval(".h1 | nodes | map(upcase)", input).unwrap()
}

/// Benchmark: Large markdown document with recursive evaluator
#[divan::bench(name = "large_markdown_recursive")]
fn large_markdown_recursive() -> mq_lang::RuntimeValues {
    // Generate markdown with ~500 headings (~100KB+)
    let mut md = String::new();
    for i in 1..=500 {
        md.push_str(&format!(
            "# Heading {}\n\nThis is paragraph {} with some content.\n\n- item1\n- item2\n- item3\n\n",
            i, i
        ));
    }

    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(&md).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine.load_builtin_module();
    engine.eval(".h1 | nodes | map(upcase)", input).unwrap()
}

/// Benchmark: Large loop with complex operations - OpTree
#[divan::bench(name = "large_complex_loop_optree")]
fn large_complex_loop_optree() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine.load_builtin_module();
    engine
        .eval(
            r#"
            def process(n):
                let x = n * 2
                | let y = x + 10
                | if (y > 100):
                    y - 50
                  else:
                    y + 50;
            | foreach(i, range(0, 5000, 1)): process(i);
            "#,
            vec![mq_lang::RuntimeValue::Number(0.into())].into_iter(),
        )
        .unwrap()
}

/// Benchmark: Large loop with complex operations - Recursive
#[divan::bench(name = "large_complex_loop_recursive")]
fn large_complex_loop_recursive() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine.load_builtin_module();
    engine
        .eval(
            r#"
            def process(n):
                let x = n * 2
                | let y = x + 10
                | if (y > 100):
                    y - 50
                  else:
                    y + 50;
            | foreach(i, range(0, 5000, 1)): process(i);
            "#,
            vec![mq_lang::RuntimeValue::Number(0.into())].into_iter(),
        )
        .unwrap()
}

// ============================================================================
// String Processing Benchmarks
// ============================================================================

/// Benchmark: String interpolation stress test - OpTree
#[divan::bench(name = "string_interpolation_optree")]
fn string_interpolation_optree() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine
        .eval(
            r#"
            let a = "foo"
            | let b = "bar"
            | let c = "baz"
            | s"${a}-${b}-${c}-${a}-${b}-${c}"
            "#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

/// Benchmark: String interpolation stress test - Recursive
#[divan::bench(name = "string_interpolation_recursive")]
fn string_interpolation_recursive() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine
        .eval(
            r#"
            let a = "foo"
            | let b = "bar"
            | let c = "baz"
            | s"${a}-${b}-${c}-${a}-${b}-${c}"
            "#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

// ============================================================================
// Pattern Matching Benchmarks
// ============================================================================

/// Benchmark: Pattern matching with OpTree
#[divan::bench(name = "pattern_matching_optree")]
fn pattern_matching_optree() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine.load_builtin_module();
    engine
        .eval(
            r#"
            def classify(n):
                match n:
                    0 => "zero"
                    1 => "one"
                    2 => "two"
                    _ => "many";
            | foreach(i, range(0, 1000, 1)): classify(i % 10);
            "#,
            vec![mq_lang::RuntimeValue::Number(0.into())].into_iter(),
        )
        .unwrap()
}

/// Benchmark: Pattern matching with recursive evaluator
#[divan::bench(name = "pattern_matching_recursive")]
fn pattern_matching_recursive() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine.load_builtin_module();
    engine
        .eval(
            r#"
            def classify(n):
                match n:
                    0 => "zero"
                    1 => "one"
                    2 => "two"
                    _ => "many";
            | foreach(i, range(0, 1000, 1)): classify(i % 10);
            "#,
            vec![mq_lang::RuntimeValue::Number(0.into())].into_iter(),
        )
        .unwrap()
}

// ============================================================================
// Control Flow Benchmarks
// ============================================================================

/// Benchmark: Nested if-else with OpTree
#[divan::bench(name = "nested_if_optree")]
fn nested_if_optree() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(true);
    engine.load_builtin_module();
    engine
        .eval(
            r#"
            def categorize(n):
                if (n < 10):
                    if (n < 5):
                        "very small"
                    else:
                        "small"
                elif (n < 50):
                    if (n < 25):
                        "medium-small"
                    else:
                        "medium"
                else:
                    if (n < 75):
                        "large"
                    else:
                        "very large";
            | foreach(i, range(0, 1000, 1)): categorize(i % 100);
            "#,
            vec![mq_lang::RuntimeValue::Number(0.into())].into_iter(),
        )
        .unwrap()
}

/// Benchmark: Nested if-else with recursive evaluator
#[divan::bench(name = "nested_if_recursive")]
fn nested_if_recursive() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.set_optree_enabled(false);
    engine.load_builtin_module();
    engine
        .eval(
            r#"
            def categorize(n):
                if (n < 10):
                    if (n < 5):
                        "very small"
                    else:
                        "small"
                elif (n < 50):
                    if (n < 25):
                        "medium-small"
                    else:
                        "medium"
                else:
                    if (n < 75):
                        "large"
                    else:
                        "very large";
            | foreach(i, range(0, 1000, 1)): categorize(i % 100);
            "#,
            vec![mq_lang::RuntimeValue::Number(0.into())].into_iter(),
        )
        .unwrap()
}
