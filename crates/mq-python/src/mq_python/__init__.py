from mq_python import mq_python


def main() -> None:
    print(mq_python.query("# Hello World\n\n## Heading2", ".h1", None))
