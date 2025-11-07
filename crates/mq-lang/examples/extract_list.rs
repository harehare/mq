fn main() {
    let markdown_content = "
- item 1
- item 2
- item 3
    ";
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    let code = r#".[] | select(contains("2")) | to_html()?"#;
    println!(
        "{:?}",
        engine
            .eval(
                code,
                mq_lang::parse_markdown_input(markdown_content).unwrap().into_iter()
            )
            .unwrap()
    );
}
