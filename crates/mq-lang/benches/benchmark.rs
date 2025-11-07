use mq_lang::{Shared, SharedCell};

fn main() {
    divan::main();
}

#[divan::bench()]
fn eval_fibonacci() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::Engine::default();
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
fn eval_until_speed_test() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            "let i = 10000 | until(i > 0): let i = i - 1 | i;",
            vec![mq_lang::RuntimeValue::Number(1.into())].into_iter(),
        )
        .unwrap()
}

#[divan::bench()]
fn eval_while_speed_test() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            "let i = 10000 | while(i > 0): let i = i - 1 | i;",
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
    let mut engine = mq_lang::Engine::default();
    engine.eval(".h1", input.into_iter()).unwrap()
}

#[divan::bench(name = "eval_string_interpolation")]
fn eval_string_interpolation() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::Engine::default();
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
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();
    engine.eval(".h | nodes | map(upcase)", input.into_iter()).unwrap()
}

#[divan::bench]
fn eval_boolean_folding() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            r#"
            let t = true
            | let f = false
            | let r1 = t && f
            | let r2 = t || f
            | let r3 = !t
            | let r4 = !f
            | let r5 = (t && !f) || (!t && f)
            | r5
            "#, // No semicolons between lets, final variable is the result
            vec!["".into()].into_iter(),
        )
        .unwrap()
}

#[divan::bench]
fn eval_comparison_folding() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            r#"
            let n1 = 10
            | let n2 = 20
            | let n3 = 10
            | let s1 = "apple"
            | let s2 = "banana"
            | let s3 = "apple"

            | let r1 = eq(n1, n2)
            | let r2 = ne(n1, n2)
            | let r3 = gt(n2, n1)
            | let r4 = gte(n1, n3)
            | let r5 = lt(n1, n2)
            | let r6 = lte(n3, n1)

            | let r7 = eq(s1, s2)
            | let r8 = ne(s1, s3)

            | and(and(and(and(and(and(not(r1), r2), r3), r4), r5), r6), not(r8))
            "#, // No semicolons between lets, final expression is the result
            vec!["".into()].into_iter(),
        )
        .unwrap()
}

#[divan::bench]
fn eval_dead_code_elimination_benchmark() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            r#"
            let unused_num = 100
            | let used_num1 = 200
            | let unused_str = "hello"
            | let used_num2 = add(used_num1, 50)
            | let unused_bool = true
            | let unused_calc = mul(unused_num, 2)
            | let used_str = "world"
            | add(used_num2, len(used_str))
            "#, // No semicolons between lets, final expression is the result
            vec!["".into()].into_iter(),
        )
        .unwrap()
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
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();
    engine
        .eval(r#"foreach(x, range(0, 1000, 1)): x + 1;"#, vec!["".into()].into_iter())
        .unwrap()
}

#[divan::bench()]
fn eval_csv_parse() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::Engine::default();
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
    let mut engine = mq_lang::Engine::default();
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
    let mut engine = mq_lang::Engine::default();
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
    let mut engine = mq_lang::Engine::default();
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
    let mut engine = mq_lang::Engine::default();
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
