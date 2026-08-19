# Generate a table of contents from headings

Goal: Turn every heading in a document into a nested, linked table of contents.

Prerequisites: None. Works on any document containing headings.

## Query

```mq
.h
| let text = to_text()
| let anchor = downcase(replace(text, " ", "-"))
| let link = to_link("#" + anchor, text, "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, level - 1)
```

```bash
$ mq '.h | let text = to_text() | let anchor = downcase(replace(text, " ", "-")) | let link = to_link("#" + anchor, text, "") | let level = .h.depth | if (!is_none(level)): to_md_list(link, level - 1)' README.md
```

## Input

```markdown
# Introduction
## Getting Started
### Installation
## Usage
```

## Output

```markdown
- [Introduction](#introduction)
  - [Getting Started](#getting-started)
    - [Installation](#installation)
  - [Usage](#usage)
```

## Notes

- `.h.depth` gives the heading level (1 for `#`, 2 for `##`, ...); it's used here to control the list's indentation via `to_md_list(item, level)`.
- Gotcha: `to_link(url, to_text(), "")` without lowercasing/hyphenating first produces broken anchors for multi-word headings: `#Getting Started` (space and all) instead of `#getting-started`, which mq even wraps in angle brackets (`<#Getting Started>`) since it isn't a valid bare link target. Always slugify (`downcase` + `replace(" ", "-")`) before building the anchor.
- This assumes GitHub-style slugs. Headings with punctuation or non-ASCII text need a more thorough slugifier than a single `replace`.
