use std::str::FromStr;

fn main() {
    let markdown_content = "
- item 1
  - sub item 1
- item 2
- item 3
    ";
    let markdown = mq_markdown::Markdown::from_str(markdown_content).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::Value::from);
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    // .[] or .[t ]
    let code = ".[] | select(is_list2()) | to_html()?";
    println!("{:?}", engine.eval(code, input).unwrap());
}
