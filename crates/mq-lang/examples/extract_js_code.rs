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
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();

    let code = r#".code("js") | to_text()?"#;
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
