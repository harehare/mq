use std::str::FromStr;

fn main() {
    let markdown_content = "
# Example

```javascript
const numbers = [1, 2, 3, 4, 5];
const doubled = numbers.map(n => n * 2);
console.log(doubled);
```
    ";
    let markdown = mq_md::Markdown::from_str(markdown_content).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::Value::from);
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module().unwrap();

    let code = ".code | to_text()?";
    println!("{:?}", engine.eval(&code, input).unwrap());
}
