<h1 align="center">mq-python</h1>

[![PyPI](https://img.shields.io/pypi/v/markdown-query.svg)](https://pypi.org/project/markdown-query/)
[![ci](https://github.com/harehare/mq/actions/workflows/ci.yml/badge.svg)](https://github.com/harehare/mq/actions/workflows/ci.yml)
![GitHub Release](https://img.shields.io/github/v/release/harehare/mq)
[![codecov](https://codecov.io/gh/harehare/mq/graph/badge.svg?token=E4UD7Q9NC3)](https://codecov.io/gh/harehare/mq)
[![CodSpeed Badge](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://codspeed.io/harehare/mq)

Python bindings for the mq Markdown processor.

## Installation

```bash
pip install markdown-query
```

## Development

### Building from Source

```bash
git clone https://github.com/harehare/mq
cd mq/crates/mq-python
pip install maturin
maturin develop
```

### Running Tests

```bash
pytest tests/
```
## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)
- 📦 [PyPI package](https://pypi.org/project/markdown-query/)

## License

Licensed under the MIT License.
