from typing import List, Optional

class InputFormat:
    """The format of the input document."""

    MARKDOWN: "InputFormat"  # Markdown format
    MDX: "InputFormat"  # MDX format
    HTML: "InputFormat"  # HTML format
    TEXT: "InputFormat"  # Plain text format

class ListStyle:
    """Style to use for markdown lists."""

    DASH: "ListStyle"  # Lists with dash (-) markers
    PLUS: "ListStyle"  # Lists with plus (+) markers
    STAR: "ListStyle"  # Lists with asterisk (*) markers

class TitleSurroundStyle:
    """Style for surrounding link titles."""

    DOUBLE: "TitleSurroundStyle"  # Double quotes (")
    SINGLE: "TitleSurroundStyle"  # Single quotes (')
    PAREN: "TitleSurroundStyle"  # Parentheses ()

class UrlSurroundStyle:
    """Style for surrounding URLs."""

    ANGLE: "UrlSurroundStyle"  # Angle brackets <>
    NONE: "UrlSurroundStyle"  # No surrounding characters

class Options:
    """Configuration options for mq processing."""

    def __init__(self) -> None: ...
    @property
    def input_format(self) -> InputFormat: ...
    @property
    def list_style(self) -> ListStyle: ...
    @property
    def link_title_style(self) -> TitleSurroundStyle: ...
    @property
    def link_url_style(self) -> UrlSurroundStyle: ...

def run(code: str, content: str, options: Optional[Options]) -> List[str]:
    """
    Run an mq query against markdown content with the specified options.

    Args:
        code: The mq query to run against the content
        content: The markdown content to process
        options: Configuration options for processing

    Returns:
        List of results as strings
    """
