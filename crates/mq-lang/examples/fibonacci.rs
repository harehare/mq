fn main() {
    let mut engine = mq_lang::DefaultEngine::default();
    let code = "
     def fibonacci(x):
      if(x < 2):
        x
      else:
        fibonacci(sub(x, 1)) + fibonacci(sub(x, 2)); | fibonacci(20)";
    println!("{:?}", engine.eval(code, mq_lang::null_input().into_iter()).unwrap());
}
