from typing import List, Optional

class InputFormat:
    """The format of the input document."""

    MARKDOWN: "InputFormat"  # Markdown format
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

    def __init__(
        self,
        format: InputFormat = ...,  # Input document format
        is_mdx: bool = ...,  # Whether to treat input as MDX
        is_update: bool = ...,  # Whether to update document in-place
        input_format: InputFormat = None,  # Alternative input format specification
        list_style: ListStyle = None,  # Style to use for lists in output
        link_title_style: TitleSurroundStyle = None,  # Style for surrounding link titles
        link_url_style: UrlSurroundStyle = None,  # Style for surrounding URLs
    ) -> None: ...
    @property
    def format(self) -> InputFormat: ...
    @property
    def is_mdx(self) -> bool: ...
    @property
    def is_update(self) -> bool: ...
    @property
    def input_format(self) -> InputFormat | None: ...
    @property
    def list_style(self) -> ListStyle | None: ...
    @property
    def link_title_style(self) -> TitleSurroundStyle | None: ...
    @property
    def link_url_style(self) -> UrlSurroundStyle | None: ...

def query(content: str, query: str, options: Optional[Options]) -> List[str]:
    """
    Run an mq query against markdown content with the specified options.

    Args:
        content: The markdown content to process
        query: The mq query to run against the content
        options: Configuration options for processing

    Returns:
        List of results as strings
    """
    ...
