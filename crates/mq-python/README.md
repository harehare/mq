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

## Usage

### Basic Usage

Use the `run` function to process Markdown with mq queries:

```python
import mq

# Extract all level 1 headings
result = mq.run(".h1", "# Hello World\n\n## Heading2\n\nText")
print(result.values)  # ['# Hello World']

# Extract all level 2 headings
result = mq.run(".h2", "# Main Title\n\n## Section A\n\n## Section B")
print(result.values)  # ['## Section A', '## Section B']

# Get all results as a single string
print(result.text)  # '## Section A\n## Section B'
```

### Filtering and Transforming

Use mq query syntax to filter and transform Markdown:

```python
import mq

markdown = """
# Product

## Features
Great features here.

## Installation
Install instructions.
"""

# Filter headings containing specific text
result = mq.run('.h2 | select(contains("Feature"))', markdown)
print(result.values)  # ['## Features']

# Extract list items
result = mq.run(".[]", "# List\n\n- Item 1\n- Item 2\n- Item 3")
print(result.values)  # ['- Item 1', '- Item 2', '- Item 3']

# Extract code blocks
result = mq.run(".code", "# Code\n\n```python\nprint('Hello')\n```")
print(result.values)  # ["```python\nprint('Hello')\n```"]
```

### Input Formats

mq supports multiple input formats:

```python
import mq

# Markdown (default)
options = mq.Options()
options.input_format = mq.InputFormat.MARKDOWN
result = mq.run(".h1", "# Heading", options)

# MDX (Markdown with JSX)
options = mq.Options()
options.input_format = mq.InputFormat.MDX
result = mq.run("select(is_mdx())", "# MDX\n\n<Component />", options)
print(result.values)  # ['<Component />']

# HTML
options = mq.Options()
options.input_format = mq.InputFormat.HTML
result = mq.run('select(contains("Hello"))', "<h1>Hello</h1><p>World</p>", options)
print(result.values)  # ['# Hello']

# Plain text
options = mq.Options()
options.input_format = mq.InputFormat.TEXT
result = mq.run('select(contains("2"))', "Line 1\nLine 2\nLine 3", options)
print(result.values)  # ['Line 2']
```

Available input formats:
- `InputFormat.MARKDOWN` - Standard Markdown (default)
- `InputFormat.MDX` - Markdown with JSX
- `InputFormat.HTML` - HTML content
- `InputFormat.TEXT` - Plain text
- `InputFormat.RAW` - Raw string input
- `InputFormat.NULL` - Null input

### Rendering Options

Customize the output rendering:

```python
import mq

options = mq.Options()
options.input_format = mq.InputFormat.MARKDOWN
options.list_style = mq.ListStyle.PLUS        # Use '+' for list items
options.link_title_style = mq.TitleSurroundStyle.SINGLE  # Use single quotes for link titles
options.link_url_style = mq.UrlSurroundStyle.ANGLE       # Use angle brackets for URLs

result = mq.run(".", markdown, options)
```

Available options:
- `ListStyle`: `DASH` (default), `PLUS`, `STAR`
- `TitleSurroundStyle`: `DOUBLE` (default), `SINGLE`, `PAREN`
- `UrlSurroundStyle`: `NONE` (default), `ANGLE`

### HTML to Markdown Conversion

Convert HTML to Markdown:

```python
import mq

html = "<h1>Hello World</h1><p>This is a <strong>test</strong>.</p>"
markdown = mq.html_to_markdown(html)
print(markdown)  # '# Hello World\n\nThis is a **test**.'

# With conversion options
options = mq.ConversionOptions()
options.extract_scripts_as_code_blocks = True  # Convert <script> tags to code blocks
options.generate_front_matter = True           # Generate front matter from metadata
options.use_title_as_h1 = True                 # Use <title> as h1 heading

markdown = mq.html_to_markdown(html, options)
```

### Working with Results

The `run` function returns an `MQResult` object:

```python
import mq

result = mq.run(".h", "# H1\n\n## H2\n\n### H3")

# Get the number of results
print(len(result))  # 3

# Access individual results by index
print(result[0].text)  # '# H1'

# Iterate over results
for value in result.values:
    print(value)

# Get all results as a single string
print(result.text)  # '# H1\n## H2\n### H3'

# Check if a value is in the result
print("# H1" in result.values)  # True
```

Each `MQValue` has the following properties:
- `text` - The string representation of the value
- `values` - For arrays, returns the list of values
- `markdown_type` - The type of Markdown element (e.g., `Heading`, `Code`, `List`)
- `is_array()` - Check if the value is an array
- `is_markdown()` - Check if the value is a Markdown element

### Error Handling

Invalid queries raise a `RuntimeError`:

```python
import mq

try:
    result = mq.run(".invalid!!!", "# Heading")
except RuntimeError as e:
    print(f"Query error: {e}")
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
