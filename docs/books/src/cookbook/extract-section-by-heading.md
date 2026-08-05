# Extract a section by its heading

**Goal**: Pull just one section — say, the "Installation" section of a README — out of a larger document, heading included.

**Prerequisites**: The `section` module. Section functions need all document nodes at once, not one node at a time, so pass `-A` on the command line or pipe through `nodes` in a script.

## Query

**`-A` flag** (command line):

```bash
$ mq -A 'section::section("Installation")' README.md
```

**`import` + `nodes`** (inline query or script):

```mq
import "section"
| nodes
| section::section("Installation")
```

**`include`** (no namespace prefix):

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
