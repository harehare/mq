use std::str::FromStr;

fn main() {
    let markdown_content = "
# Example

```js
console.log('Hello, World!');
```

```python
print('Hello, World!')
```

```js
console.log('Hello, World!')
```
    ";
    let markdown = mq_markdown::Markdown::from_str(markdown_content).unwrap();
    let input = markdown.nodes.into_iter().map(mq_lang::Value::from);
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    let code = r#".code("js") | to_text()?"#;
    println!("{:?}", engine.eval(code, input).unwrap());
}
