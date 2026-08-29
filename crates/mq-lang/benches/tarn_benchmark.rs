//! M1 speed-signal check: bytecode VM dispatch vs. the tree-walking evaluator, for the
//! narrow subset the VM prototype currently supports (arithmetic, if, recursive calls).

fn main() {
    divan::main();
}

const FIB: &str = "
     def fibonacci(x):
      if (x < 2):
        x
      else:
        fibonacci(x - 1) + fibonacci(x - 2); | fibonacci(20)";

#[divan::bench(name = "vm_fibonacci")]
fn vm_fibonacci() -> mq_lang::RuntimeValue {
    mq_lang::__tarn_bench_eval(FIB).unwrap()
}

#[divan::bench(name = "tree_walk_fibonacci")]
fn tree_walk_fibonacci() -> mq_lang::RuntimeValues {
    let mut engine = mq_lang::DefaultEngine::default();
    engine
        .eval(FIB, vec![mq_lang::RuntimeValue::Number(20.into())].into_iter())
        .unwrap()
}
