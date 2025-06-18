use std::{cell::RefCell, rc::Rc};

fn main() {
    divan::main();
}

#[divan::bench(args = [20])]
fn eval_fibonacci(n: u64) -> mq_lang::Values {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            "
     def fibonacci(x):
      if(x == 0):
        0
      elif(x == 1):
          1
      else:
        fibonacci(sub(x, 1)) + fibonacci(sub(x, 2)); | fibonacci(20)",
            vec![mq_lang::Value::Number(n.into())].into_iter(),
        )
        .unwrap()
}

#[divan::bench(args = [100_000])]
fn eval_speed_test(n: u64) -> mq_lang::Values {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            "until(gt(0)): sub(1);",
            vec![mq_lang::Value::Number(n.into())].into_iter(),
        )
        .unwrap()
}

#[divan::bench(name = "eval_select_h")]
fn eval_select_h() -> mq_lang::Values {
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_str(
        "# heading\n- item1\n- item2\n## heading2\n- item1\n- item2\n",
    )
    .unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::Value::from);
    let mut engine = mq_lang::Engine::default();
    engine.eval(".h1", input.into_iter()).unwrap()
}

#[divan::bench(name = "eval_string_interpolation")]
fn eval_string_interpolation() -> mq_lang::Values {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            r#"let world = "world" | s"$$Hello, ${world}$$""#, // Semicolon is correct here before pipe
            vec!["".into()].into_iter(),
        )
        .unwrap()
}

#[divan::bench(name = "eval_nodes")]
fn eval_nodes() -> mq_lang::Values {
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_str(
        "# heading\n- item1\n- item2\n## heading2\n- item1\n- item2\n",
    )
    .unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::Value::from);
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();
    engine
        .eval(".h | nodes | map(upcase)", input.into_iter())
        .unwrap()
}

#[divan::bench]
fn eval_boolean_folding() -> mq_lang::Values {
    let mut engine = mq_lang::Engine::default();
    engine
        .eval(
            r#"
            let t = true
            | let f = false
            | let r1 = and(t, f)
            | let r2 = or(t, f)
            | let r3 = not(t)
            | let r4 = not(f)
            | let r5 = or(and(t, not(f)), and(not(t), f))
            | r5
            "#, // No semicolons between lets, final variable is the result
            vec!["".into()].into_iter(),
        )
        .unwrap()
}

#[divan::bench]
fn eval_comparison_folding() -> mq_lang::Values {
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
fn eval_dead_code_elimination_benchmark() -> mq_lang::Values {
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
fn parse_fibonacci() -> Vec<Rc<mq_lang::AstNode>> {
    let token_arena = Rc::new(RefCell::new(mq_lang::Arena::new(100)));
    mq_lang::parse(
        "
     def fibonacci(x):
      if(x == 0):
        0
      else:
        if(x == 1):
          1
        else:
          fibonacci(sub(x, 1)) + fibonacci(sub(x, 2)); | fibonacci(20)",
        Rc::clone(&token_arena),
    )
    .unwrap()
}
