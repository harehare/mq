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
      if(eq(x, 0)):
        0
      else:
        if(eq(x, 1)):
          1
        else:
          add(fibonacci(sub(x, 1)), fibonacci(sub(x, 2))); | fibonacci(20)",
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

#[divan::bench]
fn parse_fibonacci() -> Vec<Rc<mq_lang::AstNode>> {
    let token_arena = Rc::new(RefCell::new(mq_lang::Arena::new(100)));
    mq_lang::parse(
        "
     def fibonacci(x):
      if(eq(x, 0)):
        0
      else:
        if(eq(x, 1)):
          1
        else:
          add(fibonacci(sub(x, 1)), fibonacci(sub(x, 2))); | fibonacci(20)",
        Rc::clone(&token_arena),
    )
    .unwrap()
}
