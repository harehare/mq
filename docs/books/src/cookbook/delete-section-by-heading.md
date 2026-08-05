# Delete a section by its heading

**Goal**: Drop a section you no longer want — e.g. remove a "Deprecated" section before publishing docs — while leaving the rest of the document intact.

**Prerequisites**: The `section` module, via `-A` or `nodes`.

## Query

```bash
$ mq -A 'section::filter_sections(fn(s): section::title(s) != "Deprecated";)' README.md
```

Or with `nodes`:

```mq
import "section"
| nodes
| section::filter_sections(fn(s): section::title(s) != "Deprecated";)
```

## Input

```markdown
# Introduction

Welcome to the project.

## Installation

Run the following command.

## Deprecated

Do not use this anymore.
```

## Output

```markdown
# Introduction

Welcome to the project.

## Installation

Run the following command.
```

## Notes

- `filter_sections` keeps a section when the predicate returns true, so the same pattern works for any condition — e.g. matching against a list of titles to drop, instead of a single `!=` check.
- `section::collect` (flattening section objects back to plain Markdown nodes) isn't needed here — mq expands section objects to Markdown automatically in query output. It's only required when consuming section objects from Rust or other embedding code; see the equivalent note in [Extract a section by its heading](extract-section-by-heading.md).
