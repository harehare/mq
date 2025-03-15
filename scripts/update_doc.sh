#!/bin/bash

# Update the documentation with the latest CLI help and builtin functions
mq -U --args mq_cli_help "`mq -h | tail -n +3`" '.code("sh") | update(mq_cli_help)' docs/books/src/reference/cli.md > docs/books/src/reference/cli.md.tmp \
&& mv docs/books/src/reference/cli.md.tmp docs/books/src/reference/cli.md

mq '.h' docs/books/src/reference/builtin_functions.md > docs/books/src/reference/builtin_functions.md.tmp \
&& mq docs >> docs/books/src/reference/builtin_functions.md.tmp \
&& mv docs/books/src/reference/builtin_functions.md.tmp docs/books/src/reference/builtin_functions.md
