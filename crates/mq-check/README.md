# mq-check

A syntax and semantic checker for [mq](https://github.com/harehare/mq) files. Validates `.mq` query files for errors and warnings, providing colored diagnostic output with precise line and column information.

## Features

- Detects syntax errors and semantic issues in mq files
- Colored terminal output (red for errors, yellow for warnings)
- Precise error locations with line and column numbers
- Supports checking multiple files at once
- Suitable for CI/CD pipelines (non-zero exit code on errors)
- Available as both a CLI tool and a library

## Installation

### From source

```bash
cargo install mq-check
```

## Usage

### CLI

```bash
# Check a single file
mq-check query.mq

# Check multiple files
mq-check query1.mq query2.mq

# Use in CI/CD
mq-check *.mq && echo "All checks passed"
```

#### Example output

```
Checking: query.mq
  Error: Expected `;` but found `|` at line 2, column 5
  Warning: Function `old_func` is deprecated at line 3, column 10
```

### Library

```rust
use std::path::PathBuf;

fn main() -> miette::Result<()> {
    let files = vec![PathBuf::from("query.mq")];
    mq_check::check_files(&files)
}
```

## License

MIT
