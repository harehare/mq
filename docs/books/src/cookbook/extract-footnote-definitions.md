# Extract footnote definitions

**Goal**: Collect every footnote definition in a document — e.g. to pull out a paper's citations as a standalone reference list.

**Prerequisites**: None.

## Query

```mq
.footnote
```

```bash
$ mq '.footnote' paper.md
```

## Input

```markdown
# Doc

Here is a claim[^1] and another[^2].

[^1]: First source.
[^2]: Second source.
```

## Output

```markdown
[^1]: First source.
[^2]: Second source.
```

## Notes

- This selects the footnote *definitions* (the `[^1]: ...` lines), not the inline reference marks (`[^1]`) in the body text.
- Pipe through `to_text` to strip the `[^n]:` marker and get just the citation text.
