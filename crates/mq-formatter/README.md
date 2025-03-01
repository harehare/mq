# mq-formatter

This crate provides an automatic code formatter to enforce consistent code style across [mq](https://github.com/harehare/mq) source files.
By applying standardized formatting rules, it helps developers maintain clean and readable [mq](https://github.com/harehare/mq) code.

### Examples

```rust
use mq_formatter::{Formatter, FormatterConfig};

let config = FormatterConfig::default();
let mut formatter = Formatter::new(Some(config));
let data = "if(a):1 elif(b):2 else:3";
let formatted_data = formatter.format(data);

assert_eq!(formatted_data.unwrap(), "if (a): 1 elif (b): 2 else: 3");
```
