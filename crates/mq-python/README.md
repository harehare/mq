# mq

[![ci](https://github.com/harehare/mq/actions/workflows/ci.yml/badge.svg)](https://github.com/harehare/mq/actions/workflows/ci.yml)
![GitHub Release](https://img.shields.io/github/v/release/harehare/mq)
[![codecov](https://codecov.io/gh/harehare/mq/graph/badge.svg?token=E4UD7Q9NC3)](https://codecov.io/gh/harehare/mq)
[![CodSpeed Badge](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://codspeed.io/harehare/mq)

Python bindings for the mq Markdown processor.

## Overview

`markdown-query` provides Python bindings to the [`mq`](https://github.com/harehare/mq), allowing Python developers to use mq's Markdown processing capabilities directly from Python code.

## Installation

```bash
pip install markdown-query
```

## Usage

````python
import mq

# Process a markdown string with an mq query
markdown = '# Hello\n\nThis is a paragraph\n\n## Section\n\nMore text.\n\n```js\nconsole.log("code")\n```'

print(mq.run("select(or(.h1, .code))", markdown, None).values)
# ['# Hello', '```js\nconsole.log("code")\n```']

print(mq.run("select(or(.h1, .code)) | to_text()", markdown, None).values)
# ['Hello', 'console.log("code")']

print(mq.run("select(or(.h1, .code)) | to_text()", markdown, None)[0].text)
# Hello

print([m for m in mq.run("select(or(.h1, .code))", markdown, None)])
# MarkdownType.Heading

print(mq.run("select(or(.h1, .code))", markdown, None)[0].markdown_type)
# MarkdownType.Heading

# Process a html string with an mq query
markdown = '<h1>Title</h1><p>Paragraph</p>'

options = mq.Options()
options.input_format = mq.InputFormat.HTML
print(mq.run(".h1 | upcase()", markdown, options).values)
# ['# TITLE']
````

### Using with markitdown

You can combine `mq` with [markitdown](https://github.com/microsoft/markitdown) for even more powerful Markdown processing workflows:

```python
from markitdown import MarkItDown
import mq

markitdown = MarkItDown()
result = markitdown.convert("https://github.com/harehare/mq")

print(mq.run(".code | to_text()", result, None))
print(mq.run(".[] | to_html()", result, None))
```

For more detailed usage and examples, refer to the [documentation](https://mqlang.org/book/).

## Playground

An [Online Playground](https://mqlang.org/playground) is available, powered by WebAssembly.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
