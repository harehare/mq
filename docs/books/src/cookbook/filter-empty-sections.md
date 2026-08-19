# Find sections that have no content

Goal: Spot placeholder or empty sections, headings with nothing written under them yet, as a quick doc-completeness check.

Prerequisites: The `section` module, via `-A` or `nodes`.

## Query

```bash
$ mq -A 'section::sections | filter(fn(s): !section::has_content(s);) | section::titles' README.md
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
Empty Section
```

## Notes

- Flip the predicate (drop the `!`) to list sections *with* content instead, e.g. as a sanity check that nothing got filtered out by mistake.
- "No content" means no non-heading nodes directly under it. A subsection's own text doesn't count as its parent's content: a heading followed only by a deeper heading (`# Parent` then `## Child` with text under `Child`) still flags `Parent` as empty.
