# mdq-formatter

This module provides functionality for formatting data within the `mdq-formatter` crate.

The `formatter` module contains the core structures and configurations used for formatting.

## Examples

```rust
use mdq_formatter::{Formatter, FormatterConfig};

let config = FormatterConfig::default();
let mut formatter = Formatter::new(Some(config));
let data = "if(a): 1 elif(b): 2 else: 3";
let formatted_data = formatter.format(data);

assert_eq!(formatted_data.unwrap(), "if (a):1 elif (b):2 else:3");
```
