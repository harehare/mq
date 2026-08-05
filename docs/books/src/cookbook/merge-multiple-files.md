# Merge multiple Markdown files into one stream

**Goal**: Concatenate several files, with the source path visible before each one, to review a set of docs in one pass.

**Prerequisites**: None.

## Query

```bash
$ mq -S 's"\n${__FILE__}\n"' 'identity' docs/**/*.md
```

## Input

`docs/intro.md`:

```markdown
# Introduction
Welcome.
```

`docs/usage.md`:

```markdown
# Usage
Use it like this.
```

## Output

```markdown

docs/intro.md
# Introduction
Welcome.

docs/usage.md
# Usage
Use it like this.
```

## Notes

- `-S <query>` inserts the result of `<query>` as a separator between files; `__FILE__` is a built-in variable holding the current file's path.
- `identity` passes each node through unchanged — swap it for a real query to filter/transform while merging.
