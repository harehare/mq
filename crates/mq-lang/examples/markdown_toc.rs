fn main() {
    let markdown_content = "
# [header1](https://example.com)

- item 1
- item 2

## header2

- item 1
- item 2

### header3

- item 1
- item 2

#### header4

- item 1
- item 2
";
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    let code = r##".h | let link = to_link("#" + to_text(self), to_text(self), "") | let level = .h.depth | if (not(is_none(level))): to_md_list(link, to_number(level))"##;
    println!(
        "{:?}",
        engine
            .eval(
                code,
                mq_lang::parse_markdown_input(markdown_content).unwrap().into_iter()
            )
            .unwrap()
            .compact()
    );
}
