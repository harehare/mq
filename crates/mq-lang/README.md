# mq-lang

`mq-lang` is provides a parser and evaluator for a [mq](https://github.com/harehare/mq).

### Examples

```rs
use mq_lang::Engine;

let code = "add(\"world!\")";
let input = vec![mq_lang::Value::Markdown(
  mq_md::Markdown::from_str("Hello,").unwrap()
)].into_iter();
let mut engine = mq_lang::Engine::default();

assert!(matches!(engine.eval(&code, input).unwrap(), mq_lang::Value::String("Hello,world!".to_string())));
```
