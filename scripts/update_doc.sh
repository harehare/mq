#!/bin/bash

CLI_HELP=`mq -h | tail -n +3`
# Update the documentation with the latest CLI help and builtin functions
mq -U -o docs/books/src/reference/cli.md --args mq_run_help "$CLI_HELP" 'select(.code.lang == "sh") | update(mq_run_help)' docs/books/src/reference/cli.md
mq -U -o README.md --args mq_run_help "$CLI_HELP" 'select(.code.lang == "sh") | select(contains("Usage: mq")) | update(mq_run_help)' README.md

# Update the built-in functions documentation
mq docs -- -F html -M json -M csv -M section -M toml -M yaml -M xml -M fuzzy -B  > docs/books/src/builtins.html

# Generate the sitemap
cd scripts && mq 'include "sitemap" | .[] | nodes | sitemap("https://mqlang.org/book/")' ../docs/books/src/SUMMARY.md > ../docs/books/src/sitemap.xml

