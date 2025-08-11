from typing import List, Optional
from enum import Enum

class InputFormat(Enum):
    """The format of the input document."""

    MARKDOWN: 1
    MDX: 2
    TEXT: 3
    HTML: 4
    RAW: 5
    NULL: 6

class ListStyle(Enum):
    """Style to use for markdown lists."""

    DASH: 1
    PLUS: 2
    STAR: 3

class TitleSurroundStyle(Enum):
    """Style for surrounding link titles."""

    DOUBLE: 1
    SINGLE: 2
    PAREN: 3

class UrlSurroundStyle(Enum):
    """Style for surrounding URLs."""

    ANGLE: 1
    NONE: 2

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

class MarkdownType(Enum):
    """Types of Markdown elements."""

    Blockquote: int = 1
    Break: int = 2
    Definition: int = 3
    Delete: int = 4
    Heading: int = 5
    Emphasis: int = 6
    Footnote: int = 7
    FootnoteRef: int = 8
    Html: int = 9
    Yaml: int = 10
    Toml: int = 11
    Image: int = 12
    ImageRef: int = 13
    CodeInline: int = 14
    MathInline: int = 15
    Link: int = 16
    LinkRef: int = 17
    Math: int = 18
    List: int = 19
    TableHeader: int = 20
    TableRow: int = 21
    TableCell: int = 22
    Code: int = 23
    Strong: int = 24
    HorizontalRule: int = 25
    MdxFlowExpression: int = 26
    MdxJsxFlowElement: int = 27
    MdxJsxTextElement: int = 28
    MdxTextExpression: int = 29
    MdxJsEsm: int = 30
    Text: int = 31
    Empty: int = 32

class MQValue:
    """
    Represents a value in the mq query result.
    """

    @property
    def text(self) -> str:
        """
        Get the text representation of the value.

        Returns:
            str: The text representation of the value
        """

    @property
    def array(self) -> List["MQValue"]:
        """
        Get the value as an array.

        Returns:
            List[MQValue]: The value as an array of MQValue objects
        """

    @property
    def markdown_type(self) -> Optional[MarkdownType]:
        """
        Get the markdown type of the document.

        Returns:
            Optional[MarkdownType]: The markdown type of the document, or None if not applicable.
        """

    def is_array(self) -> bool:
        """
        Check if this value is an array.

        Returns:
            True if this value is an array, False otherwise
        """

    def is_markdown(self) -> bool:
        """
        Check if this value is a markdown node.

        Returns:
            True if this value is a markdown node, False otherwise
        """

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __bool__(self) -> bool: ...
    def __len__(self) -> int: ...
    def __eq__(self, other: "MQValue") -> bool: ...
    def __ne__(self, other: "MQValue") -> bool: ...
    def __lt__(self, other: "MQValue") -> bool: ...
    def __gt__(self, other: "MQValue") -> bool: ...

class MQResult:
    """
    Result of a query execution.
    Attributes:
        values: A list of MQValue objects returned by the query
    """

    values: List[MQValue]

    @property
    def text(self) -> str:
        """
        Get the text representation of all values.

        Returns:
            Text representation of all values joined by newlines
        """

    @property
    def values(self) -> List[str]:
        """
        Get a list of non-empty text values as strings.

        This returns the text representations of all non-empty values
        in the result set.

        Returns:
            List of non-empty text values as strings
        """

    def __contains__(self, item: str) -> bool: ...
    def __getitem__(self, idx: int) -> MQValue: ...
    def __len__(self) -> int: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: "MQResult") -> bool: ...
    def __ne__(self, other: "MQResult") -> bool: ...
    def __lt__(self, other: "MQResult") -> bool: ...
    def __gt__(self, other: "MQResult") -> bool: ...

# Function to run mq queries
def run(code: str, content: str, options: Optional[Options] = None) -> MQResult:
    """
    Run an mq query against markdown content with the specified options.

    This is the main entry point for processing markdown with mq from Python.
    It takes a query written in the mq query language, applies it to the provided
    markdown content, and returns the results.

    Args:
        code: The mq query string to run against the content
        content: The markdown content to process (or text depending on options)
        options: Configuration options for processing. If None, default options are used.

    Returns:
        MQResult object containing the query results

    Raises:
        RuntimeError: If there's an error parsing the markdown or evaluating the query

    Example:
        ```python
        import mq

        # Create query to extract all headings
        query = ".h1"

        # Markdown content
        content = "# Title\\n\\nSome content\\n\\n## Subtitle\\n\\nMore content"

        # Run the query
        result = mq.run(query, content)

        # Print the extracted headings
        print(result.text)
        # Output: "# Title\n## Subtitle"
        ```
    """
