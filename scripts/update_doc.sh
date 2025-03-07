#!/bin/bash

mq -U --arg mq_cli_help "`mq -h | tail -n +3`" '.code("sh") | update(mq_cli_help)' docs/books/src/reference/cli.md > docs/books/src/reference/cli.md.tmp
mv docs/books/src/reference/cli.md.tmp docs/books/src/reference/cli.md
