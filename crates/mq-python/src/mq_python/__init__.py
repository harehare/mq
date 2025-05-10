from mq_python import mq


def main() -> None:
    markdown = '# Hello\n\nThis is a paragraph\n\n## Section\n\nMore text.\n\n```js\nconsole.log("code")\n```'
    result = mq.run("select(or(.h1, .code)) | to_text()", markdown, None)
    print(result)
