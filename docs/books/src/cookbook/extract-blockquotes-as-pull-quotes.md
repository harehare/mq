# Extract blockquotes as pull quotes

Goal: Pull every blockquote out of an article as plain text, handy for picking pull quotes to share on social media, or for a "quotes from this post" summary.

Prerequisites: None.

## Query

```mq
.blockquote | to_text
```

```bash
$ mq '.blockquote | to_text' post.md
```

## Input

```markdown
# Article

Some intro text.

> This is a great pull quote worth sharing.

More text here.

> Another quote.
> Spanning two lines.
```

## Output

```
This is a great pull quote worth sharing.
Another quote.
Spanning two lines.
```

## Notes

- Drop `| to_text` to keep the `>` Markdown syntax instead of plain text.
- A blockquote spanning multiple lines (like the second one above) comes out as one multi-line result, not split into separate quotes. Group by blank lines in the source if you need them separated.
- Gotcha: a nested blockquote (`> > text`) gets its text glued to the outer quote's with no space or newline between them, e.g. `Outer quote.Nested quote.`. Check for `>>`-style input before relying on this for anything more than flat blockquotes.
