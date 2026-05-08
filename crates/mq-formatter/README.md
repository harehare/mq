<h1 align="center">mq-formatter</h1>

Automatic code formatter for mq query language.

## Installation

### Quick Install (Recommended)

```bash
curl -sSL https://mqlang.org/install_fmt.sh | bash
```

The installer will:
- Download the latest `mq-crawl` binary for your platform
- Install it to `~/.local/bin/`
- Verify the checksum of the downloaded binary
- Update your shell profile to add `mq-fmt` to your PATH

After installation, restart your terminal or source your shell profile, then verify:

```bash
mq-fmt --version
```

### Cargo

```sh
cargo install mq-formatter
```

### From Source

```sh
git clone https://github.com/harehare/mq
cd mq
cargo build --release -p mq-formatter
```

## Usage

```sh
# Format all .mq files under the current directory
mq-fmt

# Format specific files
mq-fmt file.mq another.mq

# Check formatting without modifying files (exits with non-zero if unformatted)
mq-fmt --check file.mq

# Use 4-space indentation
mq-fmt --indent-width 4 file.mq

# Sort imports and functions
mq-fmt --sort-imports --sort-functions file.mq
```

### Via mq

```sh
mq fmt file.mq
mq fmt --check file.mq
```

## Options

| Option             | Short | Description                              | Default |
| ------------------ | ----- | ---------------------------------------- | ------- |
| `--indent-width`   | `-i`  | Number of spaces for indentation         | `2`     |
| `--check`          | `-c`  | Check formatting without modifying files | `false` |
| `--sort-imports`   |       | Sort import statements                   | `false` |
| `--sort-functions` |       | Sort function definitions                | `false` |
| `--sort-fields`    |       | Sort record fields                       | `false` |

## Exit Codes

| Code | Meaning                                                                     |
| ---- | --------------------------------------------------------------------------- |
| `0`  | All files are formatted (or were successfully formatted)                    |
| `1`  | A file is not formatted (only when `--check` is used), or an error occurred |


## Library Usage

```rust
use mq_formatter::{Formatter, FormatterConfig};

let config = FormatterConfig::default();
let mut formatter = Formatter::new(Some(config));
let code = "if(a):1 elif(b):2 else:3";
let formatted = formatter.format(code).unwrap();

assert_eq!(formatted, "if (a): 1 elif (b): 2 else: 3");
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
