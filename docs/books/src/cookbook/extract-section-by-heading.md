# Extract a section by its heading

Goal: Pull just one section (say, the "Installation" section of a README) out of a larger document, heading included.

Prerequisites: The `section` module. Section functions need all document nodes at once, not one node at a time, so pass `-A` on the command line or pipe through `nodes` in a script.

## Query

`-A` flag (command line):

```bash
$ mq -A 'section::section("Installation")' README.md
```

`import` + `nodes` (inline query or script):

```mq
import "section"
| nodes
| section::section("Installation")
```

`include` (no namespace prefix):

```mq
include "section"
| nodes
| section("Installation")
```

## Input

```markdown
# Introduction

Welcome to the project.

## Installation

Run the following command.

## Usage

Use the tool like this.
```

## Output

```markdown
## Installation

Run the following command.
```

## Notes

- Section objects are automatically expanded back to Markdown nodes in CLI output, so `section::collect` isn't needed there. If you forget `-A`/`nodes` and call `section::*` on a single node, mq prints a warning on stderr and treats that node as a one-element array instead of silently giving you a meaningless result.
- Calling the section module from Rust or other code (not the CLI)? Section objects are plain dicts there and need an explicit `section::collect` to turn back into Markdown:

  ```mq
  import "section"
  | nodes
  | section::section("Installation")
  | section::collect
  ```

- Want the body without the `##` heading itself? Add `| section::bodies | first`.
- Need every section instead of one by name? Use `section::sections`, optionally filtered with `section::by_level(2)`.
- `section::section(...)` always returns an array (it matches by substring, so more than one heading could match). If you just want the one section and `None` when there isn't a match, use `section::find(...)` instead — it's `first(section::section(...))`.
- `section::section`/`section::find` match titles by substring, so `"Installation"` also matches a heading like `"Installation Guide"`. For an exact title match, use `section::by_title(...)` (returns an array) or filter an existing list of sections with `section::title_is(sections, text)`.
- Have a node instead of a title (e.g. a code block you found with `.code`) and want to know which section it's in? See [Find which section a node belongs to](find-section-containing-a-node.md).
