fn main() {
    let mut engine = mq_lang::Engine::default();
    let code = "
     def fibonacci(x):
      if(eq(x, 0)):
        0
      elif (eq(x, 1)):
        1
      else:
        add(fibonacci(sub(x, 1)), fibonacci(sub(x, 2)))
      ; | fibonacci(20)";
    let input = vec![mq_lang::Value::String("".to_string())].into_iter();
    println!("{:?}", engine.eval(code, input).unwrap());
}
