# Find sections that have no content

**Goal**: Spot placeholder or empty sections — headings with nothing written under them yet — as a quick doc-completeness check.

**Prerequisites**: The `section` module, via `-A` or `nodes`.

## Query

```bash
$ mq -A 'section::sections | filter(fn(s): section::has_content(s);) | section::titles' README.md
```

## Input

```markdown
# Introduction

Welcome to the project.

## Empty Section

## Usage

Use the tool like this.
```

## Output

```
Introduction
Usage
```

## Notes

- This lists sections *with* content; flip the predicate (`!section::has_content(s)`) to list the empty ones you actually want to fill in.
