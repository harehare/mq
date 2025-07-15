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
      if (x == 0):
        0
      elif (x == 1):
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
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(
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
    let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_markdown_str(
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
      if (x == 0):
        0
      elif (x == 1):
        1
      else:
        fibonacci(sub(x, 1)) + fibonacci(sub(x, 2)); | fibonacci(20)",
        Rc::clone(&token_arena),
    )
    .unwrap()
}

#[divan::bench(name = "eval_foreach")]
fn eval_foreach() -> mq_lang::Values {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"foreach(x, range(0, 1000, 1)): x + 1;"#,
            vec!["".into()].into_iter(),
        )
        .unwrap()
}

#[divan::bench(name = "eval_rule110")]
fn eval_rule110() -> mq_lang::Values {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();
    engine
        .eval(
            r#"# Rule110 Cellular Automaton
# Rule110 is a cellular automaton with the following rules:
# 000 → 0, 001 → 1, 010 → 1, 011 → 1, 100 → 0, 101 → 1, 110 → 1, 111 → 0
def rule110(left, center, right):
  let pattern = s"${left}${center}${right}"
  | if (pattern == "000"): 0
  elif (pattern == "001"): 1
  elif (pattern == "010"): 1
  elif (pattern == "011"): 1
  elif (pattern == "100"): 0
  elif (pattern == "101"): 1
  elif (pattern == "110"): 1
  elif (pattern == "111"): 0
  else: 0;

def safe_get(arr, index):
  if (and(index >= 0, index < len(arr))):
    nth(arr, index)
  else:
    0;

def next_generation(current_gen):
  let width = len(current_gen)
  | map(range(0, width, 1),
  fn(i):
    let left = safe_get(current_gen, sub(i, 1))
    | let center = nth(current_gen, i)
    | let right = safe_get(current_gen, add(i, 1))
    | rule110(left, center, right);
);

def generation_to_string(gen):
  map(gen, fn(cell): if (cell == 1): "█" else: " ";) | join("");

def run_rule110(initial_state, generations):
  let result = [initial_state]
  | let i = 0
  | until (i < generations):
      let current_gen = last(result)
      | let next_gen = next_generation(current_gen)
      | let result = result + [next_gen]
      | let i = i + 1
      | result;;

let width = 81
| let initial_state = map(range(0, width, 1), fn(i): if (i == floor(div(width, 2))): 1 else: 0;)
| let generations = run_rule110(initial_state, 50)
| foreach (gen, generations):
    generation_to_string(gen);
| join("\n")"#,
            vec!["".into()].into_iter(),
        )
        .unwrap()
}
