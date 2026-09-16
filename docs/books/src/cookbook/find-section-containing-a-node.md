# Find which section a node belongs to

Goal: You already have a single node (say, the first code block in a doc) and want to know which heading section it lives under, e.g. to report "found this in the *Installation* section".

Prerequisites: The `section` module, via `-A` or `nodes`.

## Query

```bash
$ mq -A 'let n = first(compact(.code)) | section::title(section::containing(., n))' README.md
```

Or with `nodes`:

```mq
import "section"
| nodes
| let n = first(compact(.code))
| section::title(section::containing(., n))
```

## Input

````markdown
# Introduction

Welcome.

## Installation

```bash
npm install
```

## Usage

Run it.
````

## Output

```
Installation
```

## Notes

- `section::containing(md_nodes, node)` is the reverse of picking a section apart: given all the document's nodes and one node you already have, it returns the `section::sections`-style dict that node belongs to (or `None` if it doesn't belong to any).
- It matches the given node against each section's `header` and `children` by equality, so it works with any node you found some other way — a plain selector match (`.code`, `.h2`, ...), a `filter`/`is_*` predicate, or an item you pulled out of `section::body`.
- Matching is structural: a document with duplicate content (e.g. two identical code blocks) resolves to the first section that contains a matching node.
- Pass `true` as the third argument to match against depth-extended sections (see `section::sections`'s `depth` parameter) instead of the immediate section.
