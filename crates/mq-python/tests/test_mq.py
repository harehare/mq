import pytest
import mq


@pytest.mark.parametrize(
    "code, content, expected",
    [
        (".h1", "# Hello World\n\n## Heading2\n\nText", ["# Hello World"]),
        (".h2", "# Hello World\n\n## Heading2\n\nText", ["## Heading2"]),
        (
            ".h2",
            "# Main Title\n\n## Heading2A\n\nText\n\n## Heading2B\n\nMore text",
            ["## Heading2A", "## Heading2B"],
        ),
        (
            '.h2 | select(contains("Feature"))',
            "# Product\n\n## Features\n\nText\n\n## Installation\n\nMore text",
            ["## Features"],
        ),
        (
            ".[]",
            "# List\n\n- Item 1\n- Item 2\n- Item 3",
            ["- Item 1", "- Item 2", "- Item 3"],
        ),
        (
            ".code",
            "# Code\n\n```python\nprint('Hello')\n```",
            ["```python\nprint('Hello')\n```"],
        ),
    ],
)
def test_mq_queries(code, content, expected):
    result = mq.run(code, content, None)
    assert result == expected


@pytest.mark.parametrize(
    "input_format, code, content, expected",
    [
        (
            mq.InputFormat.TEXT,
            'select(contains("2"))',
            "Line 1\nLine 2\nLine 3",
            ["Line 2"],
        ),
        (
            mq.InputFormat.MDX,
            "select(is_mdx())",
            "# MDX Content\n\n<Component />",
            ["<Component />"],
        ),
        (
            mq.InputFormat.HTML,
            ".h1",
            "<h1>Title</h1><p>Paragraph</p>",
            ["# Title"],
        ),
    ],
)
def test_input_formats(input_format, code, content, expected):
    options = mq.Options()
    options.input_format = input_format

    result = mq.run(code, content, options)
    assert result == expected


def test_invalid_query():
    with pytest.raises(Exception) as exc_info:
        mq.run(".invalid_selector!!!", "# Heading", None)

    assert "Error evaluating query" in str(exc_info.value)
