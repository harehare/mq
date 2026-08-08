# Bump a version string (or any text) across a file

**Goal**: Replace every occurrence of a string — like a version number — throughout a document, without losing the parts of the document that don't match.

**Prerequisites**: The `-U` flag.

## Query

```bash
$ mq -U 'select(contains("1.2.0")) | replace("1.2.0", "1.3.0")' CHANGELOG.md
```

## Input

```markdown
# My Project v1.2.0

Install version 1.2.0 to get started.

See the changelog for details.
```

## Output

```markdown
# My Project v1.3.0

Install version 1.3.0 to get started.

See the changelog for details.
```

## Notes

- **Without `-U`**, mq prints only the nodes that matched `select(...)` — here, the "See the changelog for details." paragraph would silently disappear from the output, since it never matched the `contains("1.2.0")` filter.
- **With `-U`**, mq prints the whole document back out, with only the matched-and-transformed nodes changed. This is the mode you want whenever the query's job is "edit part of this file," not "extract part of this file."
- `-U` writes to stdout, not the file itself. To edit a file on disk, redirect to a temp file and move it back: `mq -U '...' file.md > file.md.tmp && mv file.md.tmp file.md`.

## Previewing changes first

Before piping `-U` output into a file, preview what would change with `--diff`, which prints a unified diff instead of the full content:

```bash
$ mq -U --diff 'select(contains("1.2.0")) | replace("1.2.0", "1.3.0")' CHANGELOG.md
--- CHANGELOG.md
+++ CHANGELOG.md
@@ -1,4 +1,4 @@
-# My Project v1.2.0
+# My Project v1.3.0

-Install version 1.2.0 to get started.
+Install version 1.3.0 to get started.

 See the changelog for details.
```

`--diff` never writes to the file — `-U` never has. It exits with code `1` if anything would change (`0` otherwise), which is handy for a CI check. With multiple files, each gets its own diff headed by its path.
