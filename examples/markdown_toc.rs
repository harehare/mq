use std::str::FromStr;

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
    let markdown = mq_md::Markdown::from_str(markdown_content).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::Value::from);
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module().unwrap();

    let code = ".h
| let link = md_link(add(\"#\", to_text(self)), to_text(self))
| if (eq(md_name(), \"h1\")):
    md_list(link, 1)
  elif (eq(md_name(), \"h2\")):
    md_list(link, 2)
  elif (eq(md_name(), \"h3\")):
    md_list(link, 3)
  elif (eq(md_name(), \"h4\")):
    md_list(link, 4)
  elif (eq(md_name(), \"h5\")):
    md_list(link, 5)
  else:
    None
";
    println!("{:?}", engine.eval(&code, input).unwrap().compact());
}
