use mq_lang::{Shared, SharedCell};

fn main() {
    divan::main();
}

#[divan::bench()]
fn eval_fibonacci() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            "
     def fibonacci(x):
      if (x < 2):
        x
      else:
        fibonacci(x - 1) + fibonacci(x - 2); | fibonacci(20)",
            vec![mq_lang::RuntimeValue::Number(20.into())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_while_speed_test() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            "var i = 10000 | while(i > 0): i -= 1; | i",
            vec![mq_lang::RuntimeValue::Number(1.into())].into_iter(),
        )
        .unwrap()
}

#[divan::bench(name = "eval_select_h")]
fn eval_select_h() -> mq_lang::RuntimeValues {
    let markdown: mq_markdown::Markdown =
        mq_markdown::Markdown::from_markdown_str("# heading\n- item1\n- item2\n## heading2\n- item1\n- item2\n")
            .unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.eval(".h1", input.into_iter()).unwrap()
}

#[divan::bench(name = "eval_string_interpolation")]
fn eval_string_interpolation() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            r#"let world = "world" | s"$$Hello, ${world}$$""#, // Semicolon is correct here before pipe
            vec!["".into()].into_iter(),
        )
        .unwrap()
}

#[divan::bench(name = "eval_nodes")]
fn eval_nodes() -> mq_lang::RuntimeValues {
    let markdown: mq_markdown::Markdown =
        mq_markdown::Markdown::from_markdown_str("# heading\n- item1\n- item2\n## heading2\n- item1\n- item2\n")
            .unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.eval(".h | nodes | map(upcase)", input.into_iter()).unwrap()
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

#[divan::bench(name = "eval_foreach")]
fn eval_foreach() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(r#"foreach(x, range(0, 1000, 1)): x + 1;"#, vec!["".into()].into_iter())
        .unwrap()
}

#[divan::bench()]
fn eval_csv_parse() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"include "csv" | csv_parse(true)"#,
            vec![mq_lang::RuntimeValue::String("a,b,c\n\"1,2\",\"2,3\",\"3,4\"\n4,5,6\n\"multi\nline\",7,8\n9,10,\"quoted,comma\"\n\"\",11,12\n13,14,15\n".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_yaml_parse() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"include "yaml" | yaml_parse()"#,
            vec![mq_lang::RuntimeValue::String("---\nstring: hello\nnumber: 42\nfloat: 3.14\nbool_true: true\nbool_false: false\nnull_value: null\narray:\n  - item1\n  - item2\n  - item3\nobject:\n  key1: value1\n  key2: value2\nnested:\n  arr:\n    - a\n    - b\n  obj:\n    subkey: subval\nmultiline: |\n  This is a\n  multiline string\nquoted: \"quoted string\"\nsingle_quoted: 'single quoted string'\ndate: 2024-06-01\ntimestamp: 2024-06-01T12:34:56Z\nempty_array: []\nempty_object: {}\nanchors:\n  &anchor_val anchored value\nref: *anchor_val\ncomplex:\n  - foo: bar\n    baz:\n      - qux\n      - quux\n  - corge: grault\nspecial_chars: \"!@#$%^&*()_+-=[]{}|;:',.<>/?\"\nunicode: \"こんにちは世界\"\nbool_list:\n  - true\n  - false\nnull_list:\n  - null\n  - ~".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_json_parse() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"include "json" | json_parse()"#,
            vec![mq_lang::RuntimeValue::String("{\"users\":[{\"id\":1,\"name\":\"Alice\",\"email\":\"alice@example.com\",\"roles\":[\"admin\",\"user\"]},{\"id\":2,\"name\":\"Bob\",\"email\":\"bob@example.com\",\"roles\":[\"user\"]},{\"id\":3,\"name\":\"Charlie\",\"email\":\"charlie@example.com\",\"roles\":[\"editor\",\"user\"]}],\"meta\":{\"count\":3,\"generated_at\":\"2024-06-01T12:00:00Z\"}}".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_qualified_access_to_csv_module() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
       .eval(
            r#"import "csv" | csv::csv_parse(true)"#,
            vec![mq_lang::RuntimeValue::String("a,b,c\n\"1,2\",\"2,3\",\"3,4\"\n4,5,6\n\"multi\nline\",7,8\n9,10,\"quoted,comma\"\n\"\",11,12\n13,14,15\n".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_string_equality() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"
let a1 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1"
| let a2 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa2"
| let a3 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa3"
| let a4 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa4"
| let a5 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa5"
| let a6 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa6"
| let a7 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa7"
| let a8 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa8"
| a1 == a1 |  a1 == a2 |  a1 == a3 |  a1 == a4 |  a1 == a5 |  a1 == a6 |  a1 == a7 |  a1 == a8
| a2 == a1 |  a2 == a2 |  a2 == a3 |  a2 == a4 |  a2 == a5 |  a2 == a6 |  a2 == a7 |  a2 == a8
| a3 == a1 |  a3 == a2 |  a3 == a3 |  a3 == a4 |  a3 == a5 |  a3 == a6 |  a3 == a7 |  a3 == a8
| a4 == a1 |  a4 == a2 |  a4 == a3 |  a4 == a4 |  a4 == a5 |  a4 == a6 |  a4 == a7 |  a4 == a8
| a5 == a1 |  a5 == a2 |  a5 == a3 |  a5 == a4 |  a5 == a5 |  a5 == a6 |  a5 == a7 |  a5 == a8
| a6 == a1 |  a6 == a2 |  a6 == a3 |  a6 == a4 |  a6 == a5 |  a6 == a6 |  a6 == a7 |  a6 == a8
| a7 == a1 |  a7 == a2 |  a7 == a3 |  a7 == a4 |  a7 == a5 |  a7 == a6 |  a7 == a7 |  a7 == a8
| a8 == a1 |  a8 == a2 |  a8 == a3 |  a8 == a4 |  a8 == a5 |  a8 == a6 |  a8 == a7 |  a8 == a8
"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_macro_expansion_simple() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            r#"macro repeat(x): x + x + x + x + x | repeat(5)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_macro_expansion_nested() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            r#"macro double(x): x + x | macro quad(x): double(x) + double(x) | quad(5)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_no_macro_large_program() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            r#"
let a = 1 | let b = 2 | let c = 3 | let d = 4 | let e = 5
| let f = 6 | let g = 7 | let h = 8 | let i = 9 | let j = 10
| let k = 11 | let l = 12 | let m = 13 | let n = 14 | let o = 15
| a + b + c + d + e + f + g + h + i + j + k + l + m + n + o
"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

// Array/Collection Operations Benchmarks

#[divan::bench()]
fn eval_array_map() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"range(0, 1000, 1) | map(fn(x): x * 2;)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_array_filter() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"range(0, 1000, 1) | filter(fn(x): x % 2 == 0;)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_array_fold() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"def sum(acc, x): add(acc, x); | fold(range(0, 100, 1), 0, sum)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_array_chained_operations() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"range(0, 500, 1) | filter(fn(x): x % 2 == 0;) | map(fn(x): x * 3;) | filter(fn(x): x > 100;)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

// Object/Hash Access Benchmarks

#[divan::bench()]
fn eval_object_field_access() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"let obj = dict()
            | let obj = set(obj, "a", 1) | let obj = set(obj, "b", 2) | let obj = set(obj, "c", 3)
            | let obj = set(obj, "d", 4) | let obj = set(obj, "e", 5)
            | foreach(i, range(0, 100, 1)): add(add(add(add(get(obj, "a"), get(obj, "b")), get(obj, "c")), get(obj, "d")), get(obj, "e"));"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_nested_object_access() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"let inner = dict() | let inner = set(inner, "value", 42)
            | let middle = dict() | let middle = set(middle, "inner", inner)
            | let outer = dict() | let outer = set(outer, "middle", middle)
            | let obj = dict() | let obj = set(obj, "outer", outer)
            | foreach(i, range(0, 100, 1)): get(get(get(get(obj, "outer"), "middle"), "inner"), "value");"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

// Function Call Overhead Benchmarks

#[divan::bench()]
fn eval_function_call_overhead() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            r#"def identity(x): x; | foreach(i, range(0, 1000, 1)): identity(i);"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_nested_function_calls() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            r#"def add1(x): x + 1; | def add2(x): add1(add1(x)); | def add4(x): add2(add2(x));
            | foreach(i, range(0, 100, 1)): add4(i);"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

// Pipeline Processing Benchmarks

#[divan::bench()]
fn eval_long_pipeline() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"range(0, 100, 1)
            | map(fn(x): x + 1;)
            | map(fn(x): x * 2;)
            | map(fn(x): x - 3;)
            | map(fn(x): x + 4;)
            | map(fn(x): x * 5;)
            | map(fn(x): x - 6;)
            | map(fn(x): x + 7;)
            | map(fn(x): x * 8;)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_pipeline_with_conditionals() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"range(0, 100, 1)
            | map(fn(x): if (x % 2 == 0): x * 2 else: x + 1;)
            | filter(fn(x): x > 50;)
            | map(fn(x): if (x % 3 == 0): x / 3 else: x;)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

// Real-World Markdown Processing Benchmarks

#[divan::bench()]
fn eval_large_markdown_filtering() -> mq_lang::RuntimeValues {
    let markdown_content = (0..100)
        .map(|i| format!("# Heading {}\n\nSome content here.\n\n- Item 1\n- Item 2\n- Item 3\n\n## Subheading {}\n\nMore content.\n\n", i, i))
        .collect::<String>();
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(&markdown_content).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.eval(".h | nodes", input.into_iter()).unwrap()
}

#[divan::bench()]
fn eval_markdown_complex_query() -> mq_lang::RuntimeValues {
    let markdown_content = (0..50)
        .map(|i| format!("# Heading {}\n\n**Bold text** and *italic text*.\n\n- Item 1\n- Item 2\n\n```rust\nfn main() {{\n    println!(\"Hello\");\n}}\n```\n\n", i))
        .collect::<String>();
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(&markdown_content).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from);
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            ".h1 | nodes | map(upcase) | filter(fn(x): contains(x, \"HEADING\");)",
            input.into_iter(),
        )
        .unwrap()
}

// Variable Assignment and Access Benchmarks

#[divan::bench()]
fn eval_variable_assignment_chain() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(
            r#"foreach(i, range(0, 100, 1)):
            let a = i | let b = a + 1 | let c = b + 2 | let d = c + 3 | let e = d + 4
            | a + b + c + d + e;"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}

// Conditional Execution Benchmarks

#[divan::bench()]
fn eval_if_else_branching() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"def classify(x):
              if (x % 5 == 0):
                x * 5
              elif (x % 3 == 0):
                x * 3
              elif (x % 2 == 0):
                x * 2
              else:
                x;
            | map(range(0, 500, 1), classify)"#,
            vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
        )
        .unwrap()
}
