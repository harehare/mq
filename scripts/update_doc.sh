#!/bin/bash

# Update the documentation with the latest CLI help and builtin functions
mq -U -o docs/books/src/reference/cli.md --args mq_cli_help "`mq -h | tail -n +3`" '.code("sh") | update(mq_cli_help)' docs/books/src/reference/cli.md

# Update the builtin functions documentation
mq '.h' docs/books/src/reference/builtin_functions.md > docs/books/src/reference/builtin_functions.md.tmp \
&& mq docs >> docs/books/src/reference/builtin_functions.md.tmp \
&& mv docs/books/src/reference/builtin_functions.md.tmp docs/books/src/reference/builtin_functions.md

# Generate the sitemap
echo '<?xml version="1.0" encoding="UTF-8"?>' > docs/books/src/sitemap.xml
echo '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' >> docs/books/src/sitemap.xml
mq -f scripts/sitemap.mq docs/books/src/SUMMARY.md >> docs/books/src/sitemap.xml
echo '</urlset>' >> docs/books/src/sitemap.xml
