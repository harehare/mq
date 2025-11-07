fn main() {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    let code = r#"
     def snake_to_camel(x):
        let words = split(x, "_")
        | foreach(word, words):
            let first_char = upcase(first(word))
            | let rest_str = downcase(slice(word, 1, len(word)))
            | s"${first_char}${rest_str}";
        | join("");
    | snake_to_camel("CAMEL_CASE")"#;
    println!("{:?}", engine.eval(code, mq_lang::null_input().into_iter()).unwrap());
}
