# Cookbook

The Cookbook is a collection of task-first recipes for mq. Where the [Example](../start/example.md) page walks through mq's features, each Cookbook page starts from a real problem ("I want to do X") and gets straight to a working query.

## How a recipe is structured

Every recipe follows the same four parts:

- **Goal** — the problem being solved, in one sentence.
- **Prerequisites** — anything the input or environment needs (a module import, an mq flag, a document shape).
- **Query** — the mq query or command to run.
- **Output** — the result you should see.

## Selecting and filtering content

- [Generate a table of contents from headings](generate-toc-from-headings.md)
- [Extract code blocks by language](extract-code-blocks-by-language.md)
- [Extract a specific row from a table](extract-table-row.md)
- [Get the Nth item from every list](extract-nth-list-item.md)
- [Extract MDX components](extract-mdx-components.md)
- [Extract all URLs from links](extract-link-urls.md)

## Working with sections

- [Extract a section by its heading](extract-section-by-heading.md)
- [Keep only sections at a given heading level](filter-sections-by-level.md)
- [Split a document into chunks at a heading level](split-document-by-heading.md)
- [Delete a section by its heading](delete-section-by-heading.md)
- [Find sections that have no content](filter-empty-sections.md)

## Working with tables

- [Extract all tables from a document](extract-tables.md)
- [Add a row to a table](add-row-to-table.md)
- [Convert a Markdown table to CSV](convert-table-to-csv.md)
- [Convert CSV to a Markdown table](convert-csv-to-markdown-table.md)
- [Reshape a table between wide and long form](reshape-table-pivot.md)

## Functions and data

- [Write a reusable custom function](define-custom-function.md)
- [Transform, filter, and reduce arrays](transform-arrays.md)
- [Extract frontmatter metadata](extract-frontmatter.md)
- [Bump a version string across a file](update-text-in-place.md)
- [Track task-list (checkbox) progress](track-task-list-progress.md)
- [Count words in a document](count-words-in-document.md)
- [Inline local images as base64](inline-local-images-as-base64.md)

## Multi-file and LLM workflows

- [Merge multiple Markdown files into one stream](merge-multiple-files.md)
- [Control when large file sets run in parallel](process-files-in-parallel.md)
- [Generate an XML sitemap from Markdown files](generate-sitemap.md)
- [Trim a document down to LLM-sized context](extract-context-for-llm-prompts.md)
- [Generate document statistics](generate-document-statistics.md)

Looking for a full tour of mq's selectors and built-in functions instead? See the [Example](../start/example.md) page.
