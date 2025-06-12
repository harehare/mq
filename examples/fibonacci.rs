fn main() {
    let mut engine = mq_lang::Engine::default();
    let code = "
     def fibonacci(x):
      if(x == 0):
        0
      elif (x == 1):
        1
      else:
        fibonacci(sub(x, 1) + fibonacci(sub(x, 2)); | fibonacci(20)";
    let input = vec![mq_lang::Value::String("".to_string())].into_iter();
    println!("{:?}", engine.eval(code, input).unwrap());
}
