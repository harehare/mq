# mq-python

Python bindings for the mq Markdown processor.

## Overview

`mq-python` provides Python bindings to the `mq` library, allowing Python developers to use mq's powerful Markdown processing capabilities directly from Python code.

## Installation

```bash
pip install mq-python
```

## Usage

```python
import mq_python

# Process a markdown string with an mq query
markdown = "# Hello\n\nThis is a paragraph\n\n## Section\n\nMore text."
result = mq_python.query(markdown, ".h1")
print(result)
# Output: "# Hello\n## Section\n"
```

## License

MIT License
