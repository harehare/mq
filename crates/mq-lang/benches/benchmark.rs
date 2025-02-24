use std::{cell::RefCell, rc::Rc};

use criterion::{Criterion, criterion_group, criterion_main};
use mq_lang;

fn eval_fibonacci(c: &mut Criterion) {
    c.bench_function("eval_fibonacci", |b| {
        b.iter(|| {
            let mut engine = mq_lang::Engine::default();
            engine.eval(
                "
     def fibonacci(x):
      if(eq(x, 0)):
        0
      else:
        if(eq(x, 1)):
          1
        else:
          add(fibonacci(sub(x, 1)), fibonacci(sub(x, 2))); | fibonacci(20)",
                vec![mq_lang::Value::Number(30.into())].into_iter(),
            )
        })
    });
}

fn eval_speed_test(c: &mut Criterion) {
    c.bench_function("eval_speed_test", |b| {
        b.iter(|| {
            let mut engine = mq_lang::Engine::default();
            engine.eval(
                "until(gt(0)): sub(1);",
                vec![mq_lang::Value::Number(1_000_00.into())].into_iter(),
            )
        })
    });
}

fn parse_fibonacci(c: &mut Criterion) {
    let token_arena = Rc::new(RefCell::new(mq_lang::Arena::new(100)));
    c.bench_function("parse_fibonacci", |b| {
        b.iter(|| {
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
        })
    });
}

criterion_group!(benches, eval_speed_test, eval_fibonacci, parse_fibonacci);
criterion_main!(benches);
