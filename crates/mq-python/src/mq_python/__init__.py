from mq_python import mq_python


def main() -> None:
    print(mq_python.run(".h1", "# Hello World\n\n## Heading2", None))
