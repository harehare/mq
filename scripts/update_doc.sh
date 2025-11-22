#!/bin/bash

CLI_HELP=`mq -h | tail -n +3`
# Update the documentation with the latest CLI help and builtin functions
mq -U -o docs/books/src/reference/cli.md --args mq_cli_help "$CLI_HELP" 'select(.code.lang == "sh") | update(mq_cli_help)' docs/books/src/reference/cli.md
mq -U -o README.md --args mq_cli_help "$CLI_HELP" 'select(.code.lang == "sh") | select(contains("Usage: mq")) | update(mq_cli_help)' README.md

# Update the builtin functions documentation
mq '.h' docs/books/src/reference/builtin_functions.md > docs/books/src/reference/builtin_functions.md.tmp \
&& mq docs >> docs/books/src/reference/builtin_functions.md.tmp \
&& mv docs/books/src/reference/builtin_functions.md.tmp docs/books/src/reference/builtin_functions.md

# Generate the sitemap
cd scripts && mq 'include "sitemap" | .[] | nodes | sitemap("https://mqlang.org/book/")' ../docs/books/src/SUMMARY.md > ../docs/books/src/sitemap.xml
