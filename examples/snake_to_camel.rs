fn main() {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module().unwrap();

    let code = "
     def snake_to_camel(x):
        let words = split(x, \"_\")
        | foreach(word, words):
            let first_char = upcase(first(word))
            | let rest_str = downcase(slice(word, 1, len(word)))
            | add(first_char, rest_str);
        | join(\"\");
    | snake_to_camel(\"CAMEL_CASE\")";
    let input = vec![mq_lang::Value::String("".to_string())].into_iter();
    println!("{:?}", engine.eval(&code, input).unwrap());
}
