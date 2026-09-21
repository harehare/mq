# Navigate the section tree

Goal: Get a document's headings as a nested tree, then move around it: find a section's parent, ancestors and siblings, print the heading path of every section, or flatten the tree back into one list.

Prerequisites: The `section` module, via `-A` or `nodes`.

## Query

Print the heading path (breadcrumb) of every section, in document order:

```bash
$ mq -A 'let t = section::tree(self) | map(section::tree::flatten(t), fn(s): join(section::tree::breadcrumb(t, s), " > ");)' guide.md
```

Or with `nodes`:

```mq
import "section"
| nodes
| let t = section::tree(self)
| map(section::tree::flatten(t), fn(s): join(section::tree::breadcrumb(t, s), " > ");)
```

## Input

````markdown
# Guide

Intro.

## Install

### macOS

brew.

### Linux

apt.

## Usage

Run it.
````

## Output

```
Guide
Guide > Install
Guide > Install > macOS
Guide > Install > Linux
Guide > Usage
```

## Notes

- `section::tree(md_nodes)` nests sections by heading level. Every section gets a `path` field (an array of indexes, e.g. `[0, 0, 1]` is the second subsection of the first subsection of the first top-level section), which the navigation functions use to find their way.
- `section::tree::at(roots, path)` returns the section at a path, `section::tree::parent(roots, s)` its parent (`None` for a top-level section), `section::tree::ancestors(roots, s)` the chain from the top-level section down to the parent, and `section::tree::siblings(roots, s)` the other sections under the same parent.
- `section::tree::prev_sibling(roots, s)` and `section::tree::next_sibling(roots, s)` give the neighbours at the same level, or `None` at either end. For example, `section::title(section::tree::prev_sibling(t, section::tree::at(t, [0, 0, 1])))` is `macOS` for the document above.
- `section::tree::breadcrumb(roots, s)` returns the titles from the top-level section down to `s`, which is handy as the heading path to attach to a chunk of text.
- `section::tree::flatten(roots)` turns the tree into one array of sections in document order (each section followed by its subsections). Sections are returned unchanged, so `section::collect(section::tree::flatten(roots))` gives back every node of the document in order.
- These functions only navigate heading levels. Nesting of other block types (a list item inside a list, a paragraph inside a blockquote) is not covered.
- Sections built by `section::sections` (rather than `section::tree`) carry no `path`, so the navigation functions treat them as having no parent or siblings.
