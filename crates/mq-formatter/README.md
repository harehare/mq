<h1 align="center">mq-formatter</h1>

Automatic code formatter for mq query language.

### Library Usage

```rust
use mq_formatter::{Formatter, FormatterConfig};

let config = FormatterConfig::default();
let mut formatter = Formatter::new(Some(config));
let code = "if(a):1 elif(b):2 else:3";
let formatted = formatter.format(code).unwrap();

assert_eq!(formatted, "if (a): 1 elif (b): 2 else: 3");
```

## Development

### Building from Source

```sh
git clone https://github.com/harehare/mq
cd mq
cargo build --release -p mq-formatter
```

### Running Tests

```sh
cargo test -p mq-formatter
```

### Running Benchmarks

```sh
cargo bench -p mq-formatter
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
