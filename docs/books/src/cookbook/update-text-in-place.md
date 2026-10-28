# Bump a version string (or any text) across a file

Goal: Replace every occurrence of a string (like a version number) throughout a document, without losing the parts of the document that don't match.

Prerequisites: The `-U` flag.

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

- Without `-U`, mq prints only the nodes that matched `select(...)`: here, the "See the changelog for details." paragraph would silently disappear from the output, since it never matched the `contains("1.2.0")` filter.
- With `-U`, mq prints the whole document back out, with only the matched-and-transformed nodes changed. This is the mode you want whenever the query's job is "edit part of this file," not "extract part of this file."
- `-U` writes to stdout, not the file itself. To edit files on disk, add `-i` / `--in-place`: `mq -U -i '...' docs/*.md`. See [Editing files in place](#editing-files-in-place).

## Previewing changes first

Before writing anything, preview what would change with `--diff`, which prints a unified diff instead of the full content:

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

`--diff` never writes to the file, just as `-U` never has on its own. It exits with code `1` if anything would change (`0` otherwise), which is handy for a CI check. With multiple files, each gets its own diff headed by its path.

## Editing files in place

`-i` / `--in-place` rewrites each input file with the `-U` result instead of printing it:

```bash
mq -U -i 'select(contains("1.2.0")) | replace("1.2.0", "1.3.0")' docs/*.md
```

- Each file is replaced atomically, keeps its mode, owner and group (best effort, as an unprivileged user cannot chown), and a symlink is followed to its target. Extended attributes, ACLs and hard links are not preserved.
- A file whose content would not change is not touched.
- `--backup=SUFFIX` first copies each changed file to `<file>SUFFIX` (for example `--backup=.bak` gives `CHANGELOG.md.bak`).
- With `--sandbox` or `--allow-write=PATH`, every file must be covered by `--allow-write`, otherwise `-i` fails before writing anything.
- Combine it with `--diff` to preview without writing; the exit code is still `1` if anything would change.
- stdin, gzip input, `-o`, and non-Markdown `-F` formats are rejected. The same file given twice is rejected too.
- Files are written one at a time, so a failure on a later file leaves earlier files already rewritten.
