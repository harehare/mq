fn main() {
    let mut engine = mq_lang::Engine::default();
    let code = "
     def fibonacci(x):
      if(x == 0):
        0
      elif (x == 1):
        1
      else:
        fibonacci(sub(x, 1)) + fibonacci(sub(x, 2)); | fibonacci(20)";
    println!(
        "{:?}",
        engine
            .eval(code, mq_lang::null_input().into_iter())
            .unwrap()
    );
}
